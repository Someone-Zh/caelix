use crate::alias::AliasManager;
use crate::axiom::AxiomLayer;
use crate::budget::LlmBudgetManager;
use crate::conflict::ConflictManager;
use crate::index::ReverseIndexManager;
use crate::link::{Link, LinkValidator};
use crate::raw::RawLayer;
use crate::schema::{
    AliasEntry, ConflictValue, Layer, MemoryVaultConfig, PromoteConfig, RawSource,
    WikiEntityCategory, WikiEntityStatus, WikiEventStatus,
};
use crate::wiki::entity::WikiEntityLayer;
use crate::wiki::event::WikiEventLayer;
use chrono::{NaiveDate, DateTime, Utc};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct RecallResult {
    pub layer: String,
    pub file: String,
    pub heading: String,
    pub preview: String,
    pub confidence: Option<f64>,
}

#[derive(Debug)]
pub struct MemoryVault {
    root_dir: PathBuf,
    config: MemoryVaultConfig,
    raw: RawLayer,
    wiki_entity: WikiEntityLayer,
    wiki_event: WikiEventLayer,
    axiom: AxiomLayer,
    alias: Arc<RwLock<AliasManager>>,
    index: Arc<RwLock<ReverseIndexManager>>,
    conflict: Arc<RwLock<ConflictManager>>,
    budget: Arc<RwLock<LlmBudgetManager>>,
}

impl MemoryVault {
    pub fn new(config: MemoryVaultConfig) -> Self {
        let root_dir = resolve_root_dir(&config.root_dir);
        Self {
            root_dir: root_dir.clone(),
            config,
            raw: RawLayer::new(&root_dir),
            wiki_entity: WikiEntityLayer::new(&root_dir),
            wiki_event: WikiEventLayer::new(&root_dir),
            axiom: AxiomLayer::new(&root_dir),
            alias: Arc::new(RwLock::new(AliasManager::new(&root_dir))),
            index: Arc::new(RwLock::new(ReverseIndexManager::new(&root_dir))),
            conflict: Arc::new(RwLock::new(ConflictManager::new(&root_dir))),
            budget: Arc::new(RwLock::new(LlmBudgetManager::new(&root_dir))),
        }
    }

    pub async fn init(&self) -> anyhow::Result<()> {
        self.raw.ensure_dir().await?;
        self.wiki_entity.ensure_dir().await?;
        self.wiki_event.ensure_dir().await?;
        self.axiom.ensure_dir().await?;

        self.alias.write().await.load().await?;
        self.index.write().await.load().await?;
        self.conflict.write().await.load().await?;
        self.budget.write().await.load().await?;

        if self.config.auto_rebuild_index {
            self.rebuild_index().await?;
        }

        Ok(())
    }

    pub async fn write_raw(
        &self,
        date: NaiveDate,
        source: RawSource,
        tags: Vec<String>,
        heading: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        self.raw.write_entry(date, source, tags, heading, content).await?;
        self.update_index_for_raw(date).await?;
        self.validate_links_and_record_pending(content, &format!("Raw/{}.md", date.format("%Y-%m-%d")))
            .await?;
        Ok(())
    }

    pub async fn write_wiki_entity(
        &self,
        name: &str,
        category: WikiEntityCategory,
        aliases: Vec<String>,
        tags: Vec<String>,
        confidence: f64,
        derived_from: Vec<String>,
        body: &str,
    ) -> anyhow::Result<()> {
        self.wiki_entity
            .write(name, category, aliases.clone(), tags, WikiEntityStatus::Active, confidence, derived_from, body)
            .await?;

        for alias in aliases {
            self.alias.write().await.add_alias(&alias, name);
        }

        self.alias.read().await.save().await?;
        self.update_index_for_wiki_entity(name).await?;
        self.validate_links_and_record_pending(body, &format!("Wiki/Entities/{}.md", name)).await?;
        Ok(())
    }

    pub async fn write_wiki_event(
        &self,
        name: &str,
        date_range: Vec<NaiveDate>,
        participants: Vec<String>,
        related_entities: Vec<String>,
        confidence: f64,
        derived_from: Vec<String>,
        body: &str,
    ) -> anyhow::Result<()> {
        self.wiki_event
            .write(name, date_range, WikiEventStatus::Active, confidence, participants, related_entities, derived_from, body)
            .await?;

        self.update_index_for_wiki_event(name).await?;
        self.validate_links_and_record_pending(body, &format!("Wiki/Events/{}.md", name)).await?;
        Ok(())
    }

    pub async fn write_axiom(
        &self,
        name: &str,
        category: crate::schema::AxiomCategory,
        confidence: f64,
        derived_from: Vec<String>,
        body: &str,
    ) -> anyhow::Result<()> {
        let category_dir = category_to_dir(&category);
        self.axiom.write(name, category.clone(), confidence, derived_from, Vec::new(), body).await?;
        self.update_index_for_axiom(category.clone(), name).await?;
        self.validate_links_and_record_pending(body, &format!("Axioms/{}/{}.md", category_dir, name))
            .await?;
        Ok(())
    }

    pub async fn recall(&self, query: &str, top_k: usize) -> anyhow::Result<Vec<RecallResult>> {
        let alias = self.alias.read().await;
        let canonical = alias.get_canonical(query).unwrap_or(query).to_string();

        let index = self.index.read().await;
        let results = index.search(&canonical, top_k);
        let mut recall_results = Vec::new();

        for (entry, snippet) in results {
            let confidence = match entry.layer {
                Layer::Axiom => {
                    let path = PathBuf::from(&entry.file);
                    self.axiom.read_by_path(&path).await?.map(|a| a.frontmatter.confidence)
                }
                Layer::Wiki => {
                    let name = path_to_entity_name(&entry.file);
                    if entry.file.contains("Entities") {
                        self.wiki_entity.read(&name).await?.map(|e| e.frontmatter.confidence)
                    } else {
                        self.wiki_event.read(&name).await?.map(|e| e.frontmatter.confidence)
                    }
                }
                Layer::Raw => None,
            };

            recall_results.push(RecallResult {
                layer: format!("{:?}", entry.layer),
                file: entry.file,
                heading: snippet.heading,
                preview: snippet.preview,
                confidence,
            });
        }

        Ok(recall_results)
    }

    pub async fn rename_entity(&self, old_name: &str, new_name: &str) -> anyhow::Result<()> {
        if !self.wiki_entity.exists(old_name).await {
            return Err(anyhow::anyhow!("Entity {} not found", old_name));
        }

        if self.wiki_entity.exists(new_name).await {
            return Err(anyhow::anyhow!("Entity {} already exists", new_name));
        }

        self.wiki_entity.rename(old_name, new_name).await?;
        self.alias.write().await.update_canonical(old_name, new_name);
        self.index.write().await.rename_entity(old_name, new_name);

        self.update_all_links(old_name, new_name).await?;

        self.alias.read().await.save().await?;
        self.index.read().await.save().await?;
        Ok(())
    }

    pub async fn rename_event(&self, old_name: &str, new_name: &str) -> anyhow::Result<()> {
        if !self.wiki_event.exists(old_name).await {
            return Err(anyhow::anyhow!("Event {} not found", old_name));
        }

        if self.wiki_event.exists(new_name).await {
            return Err(anyhow::anyhow!("Event {} already exists", new_name));
        }

        self.wiki_event.rename(old_name, new_name).await?;
        self.index.write().await.rename_entity(old_name, new_name);

        self.update_all_links(old_name, new_name).await?;

        self.index.read().await.save().await?;
        Ok(())
    }

    pub async fn rebuild_index(&self) -> anyhow::Result<()> {
        let mut files = Vec::new();

        for (date, content) in self.raw.read_all_entries().await? {
            let path = self.raw.get_file_path(date);
            let mtime = fs::metadata(&path).await?.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            files.push((path, Layer::Raw, mtime, content));
        }

        for name in self.wiki_entity.list().await? {
            let path = self.wiki_entity.get_file_path(&name);
            if let Ok(content) = fs::read_to_string(&path).await {
                let mtime = fs::metadata(&path).await?.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
                files.push((path, Layer::Wiki, mtime, content));
            }
        }

        for name in self.wiki_event.list().await? {
            let path = self.wiki_event.get_file_path(&name);
            if let Ok(content) = fs::read_to_string(&path).await {
                let mtime = fs::metadata(&path).await?.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
                files.push((path, Layer::Wiki, mtime, content));
            }
        }

        for axiom in self.axiom.list_all().await? {
            let path = self.axiom.get_file_path(axiom.frontmatter.category, &axiom.name);
            if let Ok(content) = fs::read_to_string(&path).await {
                let mtime = fs::metadata(&path).await?.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
                files.push((path, Layer::Axiom, mtime, content));
            }
        }

        {
            let mut index = self.index.write().await;
            index.rebuild_from_files(&files);
        }
        self.index.read().await.save().await?;

        Ok(())
    }

    pub async fn check_promote_triggers(&self) -> Vec<PromoteTrigger> {
        let config = &self.config.promote;
        let mut triggers = Vec::new();

        let today_paragraphs = self.raw.count_today_paragraphs().await.unwrap_or(0);
        if today_paragraphs >= config.raw_paragraphs_per_day as usize {
            triggers.push(PromoteTrigger::RawParagraphThreshold(today_paragraphs));
        }

        for name in self.wiki_entity.list().await.unwrap_or_default() {
            if let Some(entity) = self.wiki_entity.read(&name).await.unwrap_or(None) {
                if entity.frontmatter.confidence >= config.wiki_confidence_threshold
                    && entity.frontmatter.derived_from.len() >= config.wiki_derived_from_min as usize
                {
                    triggers.push(PromoteTrigger::WikiEntityReady(name, entity.frontmatter.confidence));
                }
            }
        }

        triggers
    }

    pub fn get_budget_manager(&self) -> Arc<RwLock<LlmBudgetManager>> {
        self.budget.clone()
    }

    pub fn get_conflict_manager(&self) -> Arc<RwLock<ConflictManager>> {
        self.conflict.clone()
    }

    pub fn get_alias_manager(&self) -> Arc<RwLock<AliasManager>> {
        self.alias.clone()
    }

    pub fn get_index_manager(&self) -> Arc<RwLock<ReverseIndexManager>> {
        self.index.clone()
    }

    pub fn config(&self) -> &MemoryVaultConfig {
        &self.config
    }

    pub fn get_raw_layer(&self) -> &RawLayer {
        &self.raw
    }

    pub fn get_wiki_entity_layer(&self) -> &WikiEntityLayer {
        &self.wiki_entity
    }

    pub fn get_wiki_event_layer(&self) -> &WikiEventLayer {
        &self.wiki_event
    }

    pub fn get_axiom_layer(&self) -> &AxiomLayer {
        &self.axiom
    }

    pub async fn stats(&self) -> anyhow::Result<MemoryStats> {
        Ok(MemoryStats {
            raw_files: self.raw.list_files().await?.len(),
            wiki_entities: self.wiki_entity.list().await?.len(),
            wiki_events: self.wiki_event.list().await?.len(),
            axioms: self.axiom.list_all().await?.len(),
            axioms_active: self.axiom.list_active().await?.len(),
            pending_conflicts: self.conflict.read().await.get_pending_conflicts().len(),
            pending_candidates: self.conflict.read().await.get_pending_candidates().len(),
            pending_links: self.conflict.read().await.get_pending_links().len(),
            llm_budget_used: self.budget.read().await.get_used(),
            llm_budget_total: self.budget.read().await.get_budget(),
        })
    }

    pub async fn list_raw_files(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.raw.list_files().await?.into_iter().map(|p| p.file_name().unwrap().to_string_lossy().to_string()).collect())
    }

    pub async fn list_wiki_entities(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.wiki_entity.list().await?)
    }

    pub async fn list_wiki_events(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.wiki_event.list().await?)
    }

    pub async fn list_axioms(&self, include_deprecated: bool) -> anyhow::Result<Vec<AxiomInfo>> {
        let all = self.axiom.list_all().await?;
        let filtered = if include_deprecated {
            all
        } else {
            all.into_iter().filter(|a| a.frontmatter.status == crate::schema::AxiomStatus::Active).collect()
        };

        Ok(filtered.into_iter().map(|a| AxiomInfo {
            name: a.name,
            category: format!("{:?}", a.frontmatter.category),
            confidence: a.frontmatter.confidence,
            status: format!("{:?}", a.frontmatter.status),
            created_at: a.frontmatter.created_at,
            deprecated_reason: a.frontmatter.deprecated_reason,
        }).collect())
    }

    pub async fn get_budget_info(&self) -> BudgetInfo {
        let budget = self.budget.read().await;
        BudgetInfo {
            used: budget.get_used(),
            budget: budget.get_budget(),
            remaining: budget.get_remaining(),
            exhausted: budget.is_exhausted(),
        }
    }

    pub async fn list_conflicts(&self, all: bool) -> anyhow::Result<Vec<ConflictInfo>> {
        let conflict = self.conflict.read().await;
        let conflicts = if all {
            conflict.get_all_conflicts()
        } else {
            conflict.get_pending_conflicts()
        };

        Ok(conflicts.iter().map(|c| ConflictInfo {
            id: c.id.clone(),
            r#type: format!("{:?}", c.r#type),
            entity: c.entity.clone(),
            field: c.field.clone(),
            status: format!("{:?}", c.status),
            created_at: c.created_at,
            values: c.values.iter().map(|v| format!("{} ({}%)", v.value, (v.confidence * 100.0) as u32)).collect(),
        }).collect())
    }

    pub async fn list_candidates(&self, all: bool) -> anyhow::Result<Vec<CandidateInfo>> {
        let conflict = self.conflict.read().await;
        let candidates = if all {
            conflict.get_all_candidates()
        } else {
            conflict.get_pending_candidates()
        };

        Ok(candidates.iter().map(|c| CandidateInfo {
            id: c.id.clone(),
            confidence: c.confidence,
            status: format!("{:?}", c.status),
            created_at: c.created_at,
            preview: if c.draft.len() > 50 { format!("{}...", &c.draft[..50]) } else { c.draft.clone() },
        }).collect())
    }

    pub async fn resolve_conflict(&self, id: &str) -> anyhow::Result<bool> {
        let mut conflict = self.conflict.write().await;
        let resolved = conflict.resolve_conflict(id, Vec::new());
        conflict.save().await?;
        Ok(resolved)
    }

    pub async fn approve_candidate(&self, id: &str) -> anyhow::Result<bool> {
        let mut conflict = self.conflict.write().await;
        let approved = conflict.approve_candidate(id);
        conflict.save().await?;
        Ok(approved)
    }

    pub async fn reject_candidate(&self, id: &str) -> anyhow::Result<bool> {
        let mut conflict = self.conflict.write().await;
        let rejected = conflict.reject_candidate(id);
        conflict.save().await?;
        Ok(rejected)
    }

    async fn update_index_for_raw(&self, date: NaiveDate) -> anyhow::Result<()> {
        let path = self.raw.get_file_path(date);
        if let Some(content) = self.raw.read_file(date).await? {
            let mtime = fs::metadata(&path).await?.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            
            {
                let mut index = self.index.write().await;
                index.remove_entries_for_file(&path_to_rel_string(&path));
                let snippets = extract_snippets(&content);
                let entity_names = extract_entity_names(&content);

                for name in entity_names {
                    index.add_entry(&name, &path_to_rel_string(&path), Layer::Raw, mtime, snippets.clone());
                }

                for snippet in &snippets {
                    index.add_entry(&snippet.heading, &path_to_rel_string(&path), Layer::Raw, mtime, snippets.clone());
                }
            }

            self.index.read().await.save().await?;
        }
        Ok(())
    }

    async fn update_index_for_wiki_entity(&self, name: &str) -> anyhow::Result<()> {
        let path = self.wiki_entity.get_file_path(name);
        if let Ok(content) = fs::read_to_string(&path).await {
            let mtime = fs::metadata(&path).await?.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            self.index.write().await.remove_entries_for_file(&path_to_rel_string(&path));
            let snippets = extract_snippets(&content);
            let entity_names = extract_entity_names(&content);

            for entity_name in entity_names {
                self.index.write().await.add_entry(&entity_name, &path_to_rel_string(&path), Layer::Wiki, mtime, snippets.clone());
            }

            self.index.read().await.save().await?;
        }
        Ok(())
    }

    async fn update_index_for_wiki_event(&self, name: &str) -> anyhow::Result<()> {
        let path = self.wiki_event.get_file_path(name);
        if let Ok(content) = fs::read_to_string(&path).await {
            let mtime = fs::metadata(&path).await?.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            self.index.write().await.remove_entries_for_file(&path_to_rel_string(&path));
            let snippets = extract_snippets(&content);
            let entity_names = extract_entity_names(&content);

            for entity_name in entity_names {
                self.index.write().await.add_entry(&entity_name, &path_to_rel_string(&path), Layer::Wiki, mtime, snippets.clone());
            }

            self.index.read().await.save().await?;
        }
        Ok(())
    }

    async fn update_index_for_axiom(
        &self,
        category: crate::schema::AxiomCategory,
        name: &str,
    ) -> anyhow::Result<()> {
        let path = self.axiom.get_file_path(category, name);
        if let Ok(content) = fs::read_to_string(&path).await {
            let mtime = fs::metadata(&path).await?.modified()?.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
            self.index.write().await.remove_entries_for_file(&path_to_rel_string(&path));
            let snippets = extract_snippets(&content);
            let entity_names = extract_entity_names(&content);

            for entity_name in entity_names {
                self.index.write().await.add_entry(&entity_name, &path_to_rel_string(&path), Layer::Axiom, mtime, snippets.clone());
            }

            self.index.read().await.save().await?;
        }
        Ok(())
    }

    async fn validate_links_and_record_pending(&self, content: &str, from_file: &str) -> anyhow::Result<()> {
        let links = Link::parse(content);

        let entity_names: HashSet<String> = self.wiki_entity.list().await?.into_iter().collect();
        let event_names: HashSet<String> = self.wiki_event.list().await?.into_iter().collect();
        let axiom_names: HashSet<String> = self.axiom.list(None).await?.into_iter().collect();

        let validator = LinkValidator::new(entity_names, event_names, axiom_names);
        let pending_links = validator.validate(&links);

        for link in pending_links {
            self.conflict.write().await.add_pending_link(from_file, &link.original);
        }

        self.conflict.read().await.save().await?;
        Ok(())
    }

    async fn update_all_links(&self, old_name: &str, new_name: &str) -> anyhow::Result<()> {
        self.update_raw_links(old_name, new_name).await?;
        self.update_wiki_entity_links(old_name, new_name).await?;
        self.update_wiki_event_links(old_name, new_name).await?;
        self.update_axiom_links(old_name, new_name).await?;
        Ok(())
    }

    async fn update_raw_links(&self, old_name: &str, new_name: &str) -> anyhow::Result<()> {
        for (date, content) in self.raw.read_all_entries().await? {
            let new_content = LinkValidator::replace_entity_links(&content, old_name, new_name);
            if content != new_content {
                let path = self.raw.get_file_path(date);
                fs::write(&path, new_content).await?;
            }
        }
        Ok(())
    }

    async fn update_wiki_entity_links(&self, old_name: &str, new_name: &str) -> anyhow::Result<()> {
        for name in self.wiki_entity.list().await? {
            if let Some(mut entity) = self.wiki_entity.read(&name).await? {
                entity.body = LinkValidator::replace_entity_links(&entity.body, old_name, new_name);
                self.wiki_entity.write_entity(&entity).await?;
            }
        }
        Ok(())
    }

    async fn update_wiki_event_links(&self, old_name: &str, new_name: &str) -> anyhow::Result<()> {
        for name in self.wiki_event.list().await? {
            if let Some(mut event) = self.wiki_event.read(&name).await? {
                event.body = LinkValidator::replace_entity_links(&event.body, old_name, new_name);
                self.wiki_event.write_event(&event).await?;
            }
        }
        Ok(())
    }

    async fn update_axiom_links(&self, old_name: &str, new_name: &str) -> anyhow::Result<()> {
        for mut axiom in self.axiom.list_all().await? {
            axiom.body = LinkValidator::replace_entity_links(&axiom.body, old_name, new_name);
            self.axiom.write_axiom(&axiom).await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum PromoteTrigger {
    RawParagraphThreshold(usize),
    WikiEntityReady(String, f64),
    NewEntity(String),
}

#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub raw_files: usize,
    pub wiki_entities: usize,
    pub wiki_events: usize,
    pub axioms: usize,
    pub axioms_active: usize,
    pub pending_conflicts: usize,
    pub pending_candidates: usize,
    pub pending_links: usize,
    pub llm_budget_used: u32,
    pub llm_budget_total: u32,
}

#[derive(Debug, Clone)]
pub struct AxiomInfo {
    pub name: String,
    pub category: String,
    pub confidence: f64,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub deprecated_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BudgetInfo {
    pub used: u32,
    pub budget: u32,
    pub remaining: u32,
    pub exhausted: bool,
}

#[derive(Debug, Clone)]
pub struct ConflictInfo {
    pub id: String,
    pub r#type: String,
    pub entity: String,
    pub field: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub values: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CandidateInfo {
    pub id: String,
    pub confidence: f64,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub preview: String,
}

fn resolve_root_dir(config_dir: &str) -> PathBuf {
    if config_dir.starts_with("~") {
        let home_dir = dirs::home_dir().expect("Unable to get home directory");
        PathBuf::from(config_dir.replace("~", home_dir.to_str().unwrap()))
    } else {
        PathBuf::from(config_dir)
    }
}

fn path_to_rel_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn path_to_entity_name(path_str: &str) -> String {
    let path = PathBuf::from(path_str);
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

fn category_to_dir(category: &crate::schema::AxiomCategory) -> &str {
    match category {
        crate::schema::AxiomCategory::Methodology => "methodology",
        crate::schema::AxiomCategory::Rule => "rules",
        crate::schema::AxiomCategory::Principle => "principles",
    }
}

fn extract_snippets(content: &str) -> Vec<crate::schema::Snippet> {
    use crate::schema::Snippet;
    let mut snippets = Vec::new();
    let mut current_heading = String::new();
    let mut current_content = String::new();

    for line in content.lines() {
        if line.starts_with("## ") {
            if !current_heading.is_empty() && !current_content.is_empty() {
                let hash = crate::schema::compute_snippet_hash(&current_heading, &current_content);
                let preview = if current_content.len() > 100 {
                    format!("{}...", &current_content[..100])
                } else {
                    current_content.clone()
                };
                snippets.push(Snippet {
                    heading: current_heading.clone(),
                    hash,
                    preview,
                });
            }
            current_heading = line[3..].to_string();
            current_content = String::new();
        } else if line.starts_with("# ") {
            current_heading = line[2..].to_string();
            current_content = String::new();
        } else if !line.starts_with("---") {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    if !current_heading.is_empty() && !current_content.is_empty() {
        let hash = crate::schema::compute_snippet_hash(&current_heading, &current_content);
        let preview = if current_content.len() > 100 {
            format!("{}...", &current_content[..100])
        } else {
            current_content.clone()
        };
        snippets.push(Snippet {
            heading: current_heading,
            hash,
            preview,
        });
    }

    snippets
}

fn extract_entity_names(content: &str) -> HashSet<String> {
    use regex::Regex;
    let re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    let mut names = HashSet::new();

    for cap in re.captures_iter(content) {
        let content = &cap[1];
        if !content.ends_with('?') {
            let name = if content.starts_with("Event:") {
                &content[6..]
            } else if content.starts_with("Axiom:") {
                &content[6..]
            } else {
                content
            };
            names.insert(name.to_string());
        }
    }

    names
}

use dirs;