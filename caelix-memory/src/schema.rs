use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RawSource {
    #[serde(rename = "chat")]
    Chat,
    #[serde(rename = "meeting")]
    Meeting,
    #[serde(rename = "tweet")]
    Tweet,
    #[serde(rename = "paper")]
    Paper,
    #[serde(rename = "note")]
    Note,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawFrontmatter {
    #[serde(rename = "type")]
    pub doc_type: String,
    pub date: NaiveDate,
    pub source: RawSource,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WikiEntityCategory {
    #[serde(rename = "person")]
    Person,
    #[serde(rename = "project")]
    Project,
    #[serde(rename = "technology")]
    Technology,
    #[serde(rename = "organization")]
    Organization,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WikiEntityStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "archived")]
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiEntityFrontmatter {
    #[serde(rename = "type")]
    pub doc_type: String,
    pub category: WikiEntityCategory,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub status: WikiEntityStatus,
    pub confidence: f64,
    pub derived_from: Vec<String>,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WikiEventStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "resolved")]
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WikiEventFrontmatter {
    #[serde(rename = "type")]
    pub doc_type: String,
    pub date_range: Vec<NaiveDate>,
    pub status: WikiEventStatus,
    pub confidence: f64,
    #[serde(default)]
    pub participants: Vec<String>,
    #[serde(default)]
    pub related_entities: Vec<String>,
    pub derived_from: Vec<String>,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AxiomCategory {
    #[serde(rename = "methodology")]
    Methodology,
    #[serde(rename = "rule")]
    Rule,
    #[serde(rename = "principle")]
    Principle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AxiomStatus {
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "deprecated")]
    Deprecated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxiomFrontmatter {
    #[serde(rename = "type")]
    pub doc_type: String,
    pub category: AxiomCategory,
    pub confidence: f64,
    pub status: AxiomStatus,
    pub derived_from: Vec<String>,
    #[serde(default)]
    pub contradicts: Vec<String>,
    #[serde(default)]
    pub deprecated_by: Option<String>,
    #[serde(default)]
    pub deprecated_reason: Option<String>,
    #[serde(default)]
    pub deprecated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub replaced_by: Option<String>,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub last_reviewed: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasEntry {
    pub canonical: String,
    #[serde(default)]
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasTable {
    #[serde(flatten)]
    pub entries: HashMap<String, AliasEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConflictType {
    #[serde(rename = "entity_attribute")]
    EntityAttribute,
    #[serde(rename = "axiom_conflict")]
    AxiomConflict,
    #[serde(rename = "pending_link")]
    PendingLink,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConflictStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "resolved")]
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictValue {
    pub value: String,
    pub source: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contradiction {
    pub id: String,
    pub r#type: ConflictType,
    pub entity: String,
    #[serde(default)]
    pub field: Option<String>,
    #[serde(default)]
    pub values: Vec<ConflictValue>,
    pub status: ConflictStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CandidateStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "approved")]
    Approved,
    #[serde(rename = "rejected")]
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AxiomCandidate {
    pub id: String,
    pub draft: String,
    pub derived_from: Vec<String>,
    pub confidence: f64,
    pub status: CandidateStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingLink {
    pub from: String,
    pub link: String,
    pub target_exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flags {
    #[serde(default)]
    pub contradictions: Vec<Contradiction>,
    #[serde(default)]
    pub axiom_candidates: Vec<AxiomCandidate>,
    #[serde(default)]
    pub pending_links: Vec<PendingLink>,
}

impl Default for Flags {
    fn default() -> Self {
        Self {
            contradictions: Vec::new(),
            axiom_candidates: Vec::new(),
            pending_links: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snippet {
    pub heading: String,
    pub hash: String,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Layer {
    #[serde(rename = "raw")]
    Raw,
    #[serde(rename = "wiki")]
    Wiki,
    #[serde(rename = "axiom")]
    Axiom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub file: String,
    pub layer: Layer,
    pub mtime: i64,
    pub snippets: Vec<Snippet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReverseIndex {
    #[serde(flatten)]
    pub entries: HashMap<String, Vec<IndexEntry>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmBudgetCounter {
    pub date: NaiveDate,
    pub used: u32,
    pub budget: u32,
    pub last_call_at: i64,
    #[serde(default)]
    pub deferred_tasks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryVaultConfig {
    #[serde(default = "default_root_dir")]
    pub root_dir: String,
    #[serde(default = "default_auto_rebuild_index")]
    pub auto_rebuild_index: bool,
    #[serde(default = "default_notify_on_promote")]
    pub notify_on_promote: bool,
    #[serde(default)]
    pub promote: PromoteConfig,
}

impl Default for MemoryVaultConfig {
    fn default() -> Self {
        Self {
            root_dir: default_root_dir(),
            auto_rebuild_index: default_auto_rebuild_index(),
            notify_on_promote: default_notify_on_promote(),
            promote: PromoteConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PromoteConfig {
    #[serde(default = "default_daily_llm_budget")]
    pub daily_llm_budget: u32,
    #[serde(default = "default_raw_mentions_per_entity")]
    pub raw_mentions_per_entity: u32,
    #[serde(default = "default_raw_paragraphs_per_day")]
    pub raw_paragraphs_per_day: u32,
    #[serde(default = "default_wiki_confidence_threshold")]
    pub wiki_confidence_threshold: f64,
    #[serde(default = "default_wiki_derived_from_min")]
    pub wiki_derived_from_min: u32,
    #[serde(default = "default_event_active_days_threshold")]
    pub event_active_days_threshold: u32,
    #[serde(default = "default_axiom_auto_promote_confidence")]
    pub axiom_auto_promote_confidence: f64,
    #[serde(default = "default_axiom_candidate_confidence_min")]
    pub axiom_candidate_confidence_min: f64,
}

fn default_root_dir() -> String {
    String::from("~/.caelix/memory_vault")
}

fn default_auto_rebuild_index() -> bool {
    true
}

fn default_notify_on_promote() -> bool {
    true
}

fn default_daily_llm_budget() -> u32 {
    100
}

fn default_raw_mentions_per_entity() -> u32 {
    3
}

fn default_raw_paragraphs_per_day() -> u32 {
    10
}

fn default_wiki_confidence_threshold() -> f64 {
    0.85
}

fn default_wiki_derived_from_min() -> u32 {
    3
}

fn default_event_active_days_threshold() -> u32 {
    30
}

fn default_axiom_auto_promote_confidence() -> f64 {
    0.9
}

fn default_axiom_candidate_confidence_min() -> f64 {
    0.8
}

pub fn parse_yaml_frontmatter(content: &str) -> Option<(serde_yaml::Value, &str)> {
    if !content.starts_with("---") {
        return None;
    }
    let content = content[3..].trim_start();
    let end_pos = content.find("---")?;
    let yaml_str = &content[..end_pos];
    let body = content[end_pos + 3..].trim_start();
    let yaml: serde_yaml::Value = serde_yaml::from_str(yaml_str).ok()?;
    Some((yaml, body))
}

pub fn format_yaml_frontmatter(frontmatter: &serde_yaml::Value) -> String {
    let yaml = serde_yaml::to_string(frontmatter).unwrap_or_default();
    format!("---\n{yaml}---\n")
}

pub fn compute_snippet_hash(heading: &str, content: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    heading.hash(&mut hasher);
    content.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

impl RawFrontmatter {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.doc_type != "raw" {
            return Err(anyhow::anyhow!("Invalid raw type: {}", self.doc_type));
        }
        Ok(())
    }
}

impl WikiEntityFrontmatter {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.doc_type != "wiki_entity" {
            return Err(anyhow::anyhow!(
                "Invalid wiki_entity type: {}",
                self.doc_type
            ));
        }
        if self.confidence < 0.0 || self.confidence > 1.0 {
            return Err(anyhow::anyhow!("confidence must be between 0.0 and 1.0"));
        }
        if self.derived_from.is_empty() {
            return Err(anyhow::anyhow!("derived_from is required"));
        }
        Ok(())
    }
}

impl WikiEventFrontmatter {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.doc_type != "wiki_event" {
            return Err(anyhow::anyhow!(
                "Invalid wiki_event type: {}",
                self.doc_type
            ));
        }
        if self.confidence < 0.0 || self.confidence > 1.0 {
            return Err(anyhow::anyhow!("confidence must be between 0.0 and 1.0"));
        }
        if self.derived_from.is_empty() {
            return Err(anyhow::anyhow!("derived_from is required"));
        }
        if self.date_range.len() != 2 {
            return Err(anyhow::anyhow!("date_range must have exactly 2 dates"));
        }
        Ok(())
    }
}

impl AxiomFrontmatter {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.doc_type != "axiom" {
            return Err(anyhow::anyhow!("Invalid axiom type: {}", self.doc_type));
        }
        if self.confidence < 0.0 || self.confidence > 1.0 {
            return Err(anyhow::anyhow!("confidence must be between 0.0 and 1.0"));
        }
        if self.derived_from.is_empty() {
            return Err(anyhow::anyhow!("derived_from is required"));
        }
        Ok(())
    }
}
