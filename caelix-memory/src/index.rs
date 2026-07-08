use crate::schema::{IndexEntry, Layer, ReverseIndex, Snippet};
use serde_json;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug)]
pub struct ReverseIndexManager {
    root_dir: PathBuf,
    index: ReverseIndex,
}

impl ReverseIndexManager {
    pub fn new(root_dir: &Path) -> Self {
        Self {
            root_dir: root_dir.join("Index"),
            index: ReverseIndex {
                entries: HashMap::new(),
            },
        }
    }

    pub async fn load(&mut self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        let path = self.root_dir.join("reverse_index.json");

        if path.exists() {
            let content = fs::read_to_string(&path).await?;
            self.index = serde_json::from_str(&content)?;
        }

        Ok(())
    }

    pub async fn save(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        let path = self.root_dir.join("reverse_index.json");
        let content = serde_json::to_string_pretty(&self.index)?;
        fs::write(&path, content).await?;
        Ok(())
    }

    pub fn add_entry(
        &mut self,
        entity_name: &str,
        file: &str,
        layer: Layer,
        mtime: i64,
        snippets: Vec<Snippet>,
    ) {
        let entry = IndexEntry {
            file: file.to_string(),
            layer,
            mtime,
            snippets,
        };

        self.index
            .entries
            .entry(entity_name.to_string())
            .or_insert_with(Vec::new)
            .push(entry);
    }

    pub fn get_entries(&self, entity_name: &str) -> Option<&Vec<IndexEntry>> {
        self.index.entries.get(entity_name)
    }

    pub fn remove_entries_for_file(&mut self, file: &str) {
        for (_name, entries) in self.index.entries.iter_mut() {
            entries.retain(|e| e.file != file);
        }
        self.index.entries.retain(|_, v| !v.is_empty());
    }

    pub fn update_mtime(&mut self, file: &str, new_mtime: i64) {
        for entries in self.index.entries.values_mut() {
            for entry in entries.iter_mut() {
                if entry.file == file {
                    entry.mtime = new_mtime;
                }
            }
        }
    }

    pub fn rebuild_from_files(&mut self, files: &[(PathBuf, Layer, i64, String)]) {
        self.index.entries.clear();

        for (path, layer, mtime, content) in files {
            let file_str = path_to_rel_string(path);
            let snippets = extract_snippets(content);
            let entity_names = extract_entity_names(content);

            for name in entity_names {
                self.add_entry(&name, &file_str, layer.clone(), *mtime, snippets.clone());
            }

            for snippet in &snippets {
                self.add_entry(
                    &snippet.heading,
                    &file_str,
                    layer.clone(),
                    *mtime,
                    snippets.clone(),
                );
            }
        }
    }

    pub fn search(&self, query: &str, top_k: usize) -> Vec<(IndexEntry, Snippet)> {
        let mut results = Vec::new();

        for (entity_name, entries) in &self.index.entries {
            if entity_name.contains(query) {
                for entry in entries {
                    for snippet in &entry.snippets {
                        results.push((entry.clone(), snippet.clone()));
                    }
                }
            } else {
                for entry in entries {
                    for snippet in &entry.snippets {
                        if snippet.preview.contains(query) {
                            results.push((entry.clone(), snippet.clone()));
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| {
            let layer_order = |layer: &Layer| match layer {
                Layer::Axiom => 0,
                Layer::Wiki => 1,
                Layer::Raw => 2,
            };

            let order_a = layer_order(&a.0.layer);
            let order_b = layer_order(&b.0.layer);
            match order_a.cmp(&order_b) {
                std::cmp::Ordering::Equal => a.0.mtime.cmp(&b.0.mtime).reverse(),
                other => other,
            }
        });

        results.truncate(top_k);
        results
    }

    pub fn get_all_entity_names(&self) -> HashSet<String> {
        self.index.entries.keys().cloned().collect()
    }

    pub fn count_entries(&self) -> usize {
        self.index.entries.len()
    }

    pub fn rename_entity(&mut self, old_name: &str, new_name: &str) {
        if let Some(entries) = self.index.entries.remove(old_name) {
            self.index.entries.insert(new_name.to_string(), entries);
        }
    }
}

fn path_to_rel_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn extract_snippets(content: &str) -> Vec<Snippet> {
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
