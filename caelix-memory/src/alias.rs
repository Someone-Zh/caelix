use crate::schema::{AliasEntry, AliasTable};
use serde_json;
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
pub struct AliasManager {
    root_dir: PathBuf,
    table: AliasTable,
}

impl AliasManager {
    pub fn new(root_dir: &Path) -> Self {
        Self {
            root_dir: root_dir.join("Index"),
            table: AliasTable {
                entries: std::collections::HashMap::new(),
            },
        }
    }

    pub async fn load(&mut self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        let path = self.root_dir.join("aliases.json");

        if path.exists() {
            let content = fs::read_to_string(&path).await?;
            self.table = serde_json::from_str(&content)?;
        }

        Ok(())
    }

    pub async fn save(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        let path = self.root_dir.join("aliases.json");
        let content = serde_json::to_string_pretty(&self.table)?;
        fs::write(&path, content).await?;
        Ok(())
    }

    pub fn add_alias(&mut self, alias: &str, canonical: &str) {
        if let Some(entry) = self.table.entries.get_mut(alias) {
            if entry.canonical != canonical {
                if !entry.conflicts.contains(&canonical.to_string()) {
                    entry.conflicts.push(canonical.to_string());
                }
            }
        } else {
            self.table.entries.insert(
                alias.to_string(),
                AliasEntry {
                    canonical: canonical.to_string(),
                    conflicts: Vec::new(),
                },
            );
        }
    }

    pub fn resolve(&self, alias: &str) -> Option<(&str, Vec<&str>)> {
        let entry = self.table.entries.get(alias)?;
        let conflicts: Vec<&str> = entry.conflicts.iter().map(|s| s.as_str()).collect();
        Some((entry.canonical.as_str(), conflicts))
    }

    pub fn get_canonical(&self, alias: &str) -> Option<&str> {
        self.table.entries.get(alias).map(|e| e.canonical.as_str())
    }

    pub fn has_conflicts(&self, alias: &str) -> bool {
        self.table
            .entries
            .get(alias)
            .map(|e| !e.conflicts.is_empty())
            .unwrap_or(false)
    }

    pub fn remove_alias(&mut self, alias: &str) {
        self.table.entries.remove(alias);
    }

    pub fn list_all(&self) -> Vec<(String, String)> {
        self.table
            .entries
            .iter()
            .map(|(alias, entry)| (alias.clone(), entry.canonical.clone()))
            .collect()
    }

    pub fn update_canonical(&mut self, old_canonical: &str, new_canonical: &str) {
        for entry in self.table.entries.values_mut() {
            if entry.canonical == old_canonical {
                entry.canonical = new_canonical.to_string();
            }
            entry.conflicts.retain(|c| c != old_canonical);
        }
    }
}