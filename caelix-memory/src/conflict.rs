use crate::schema::{
    CandidateStatus, ConflictStatus, ConflictType, Contradiction, Flags, PendingLink, AxiomCandidate, ConflictValue,
};
use chrono::{DateTime, Utc};
use serde_json;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug)]
pub struct ConflictManager {
    root_dir: PathBuf,
    flags: Flags,
}

impl ConflictManager {
    pub fn new(root_dir: &Path) -> Self {
        Self {
            root_dir: root_dir.join("Index"),
            flags: Flags::default(),
        }
    }

    pub async fn load(&mut self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        let path = self.root_dir.join("flags.json");

        if path.exists() {
            let content = fs::read_to_string(&path).await?;
            self.flags = serde_json::from_str(&content)?;
        }

        Ok(())
    }

    pub async fn save(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        let path = self.root_dir.join("flags.json");
        let content = serde_json::to_string_pretty(&self.flags)?;
        fs::write(&path, content).await?;
        Ok(())
    }

    pub fn add_entity_attribute_conflict(
        &mut self,
        entity: &str,
        field: &str,
        values: Vec<ConflictValue>,
    ) -> String {
        let id = format!("c-{}", self.flags.contradictions.len() + 1);
        let conflict = Contradiction {
            id: id.clone(),
            r#type: ConflictType::EntityAttribute,
            entity: entity.to_string(),
            field: Some(field.to_string()),
            values,
            status: ConflictStatus::Pending,
            created_at: Utc::now(),
        };
        self.flags.contradictions.push(conflict);
        id
    }

    pub fn add_axiom_conflict(&mut self, axiom: &str, values: Vec<ConflictValue>) -> String {
        let id = format!("c-{}", self.flags.contradictions.len() + 1);
        let conflict = Contradiction {
            id: id.clone(),
            r#type: ConflictType::AxiomConflict,
            entity: axiom.to_string(),
            field: None,
            values,
            status: ConflictStatus::Pending,
            created_at: Utc::now(),
        };
        self.flags.contradictions.push(conflict);
        id
    }

    pub fn add_pending_link(&mut self, from: &str, link: &str) {
        let pending = PendingLink {
            from: from.to_string(),
            link: link.to_string(),
            target_exists: false,
        };

        if !self
            .flags
            .pending_links
            .iter()
            .any(|p| p.from == from && p.link == link)
        {
            self.flags.pending_links.push(pending);
        }
    }

    pub fn add_axiom_candidate(&mut self, draft: &str, derived_from: Vec<String>, confidence: f64) -> String {
        let id = format!("ac-{}", self.flags.axiom_candidates.len() + 1);
        let candidate = AxiomCandidate {
            id: id.clone(),
            draft: draft.to_string(),
            derived_from,
            confidence,
            status: CandidateStatus::Pending,
            created_at: Utc::now(),
        };
        self.flags.axiom_candidates.push(candidate);
        id
    }

    pub fn resolve_conflict(&mut self, id: &str, resolved_values: Vec<ConflictValue>) -> bool {
        if let Some(conflict) = self.flags.contradictions.iter_mut().find(|c| c.id == id) {
            conflict.status = ConflictStatus::Resolved;
            conflict.values = resolved_values;
            true
        } else {
            false
        }
    }

    pub fn approve_candidate(&mut self, id: &str) -> bool {
        if let Some(candidate) = self.flags.axiom_candidates.iter_mut().find(|c| c.id == id) {
            candidate.status = CandidateStatus::Approved;
            true
        } else {
            false
        }
    }

    pub fn reject_candidate(&mut self, id: &str) -> bool {
        if let Some(candidate) = self.flags.axiom_candidates.iter_mut().find(|c| c.id == id) {
            candidate.status = CandidateStatus::Rejected;
            true
        } else {
            false
        }
    }

    pub fn remove_pending_link(&mut self, from: &str, link: &str) {
        self.flags
            .pending_links
            .retain(|p| !(p.from == from && p.link == link));
    }

    pub fn get_pending_conflicts(&self) -> Vec<&Contradiction> {
        self.flags
            .contradictions
            .iter()
            .filter(|c| c.status == ConflictStatus::Pending)
            .collect()
    }

    pub fn get_pending_candidates(&self) -> Vec<&AxiomCandidate> {
        self.flags
            .axiom_candidates
            .iter()
            .filter(|c| c.status == CandidateStatus::Pending)
            .collect()
    }

    pub fn get_pending_links(&self) -> Vec<&PendingLink> {
        self.flags.pending_links.iter().collect()
    }

    pub fn get_all_conflicts(&self) -> Vec<&Contradiction> {
        self.flags.contradictions.iter().collect()
    }

    pub fn get_all_candidates(&self) -> Vec<&AxiomCandidate> {
        self.flags.axiom_candidates.iter().collect()
    }

    pub fn has_pending_conflicts(&self) -> bool {
        !self.get_pending_conflicts().is_empty()
    }

    pub fn has_pending_candidates(&self) -> bool {
        !self.get_pending_candidates().is_empty()
    }
}