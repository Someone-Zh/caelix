use crate::schema::{RawFrontmatter, RawSource};
use chrono::{NaiveDate, Utc};
use serde_yaml;
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

#[derive(Debug)]
pub struct RawLayer {
    root_dir: PathBuf,
}

impl RawLayer {
    pub fn new(root_dir: &Path) -> Self {
        Self {
            root_dir: root_dir.join("Raw"),
        }
    }

    pub async fn ensure_dir(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        Ok(())
    }

    pub fn get_file_path(&self, date: NaiveDate) -> PathBuf {
        self.root_dir.join(format!("{}.md", date.format("%Y-%m-%d")))
    }

    pub async fn write_entry(
        &self,
        date: NaiveDate,
        source: RawSource,
        tags: Vec<String>,
        heading: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        self.ensure_dir().await?;
        let file_path = self.get_file_path(date);

        let frontmatter = RawFrontmatter {
            doc_type: "raw".to_string(),
            date,
            source,
            tags,
        };

        let yaml = serde_yaml::to_string(&frontmatter)?;
        let heading_text = format!("## {heading}\n");

        let mut file = File::options()
            .create(true)
            .append(true)
            .open(&file_path)
            .await?;

        let exists = file_path.exists();
        if !exists {
            let header = format!("---\n{yaml}---\n# {} 原始素材\n\n", date.format("%Y-%m-%d"));
            file.write_all(header.as_bytes()).await?;
        }

        file.write_all(heading_text.as_bytes()).await?;
        file.write_all(content.as_bytes()).await?;
        file.write_all(b"\n\n").await?;

        Ok(())
    }

    pub async fn read_file(&self, date: NaiveDate) -> anyhow::Result<Option<String>> {
        let file_path = self.get_file_path(date);
        if !file_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&file_path).await?;
        Ok(Some(content))
    }

    pub async fn list_files(&self) -> anyhow::Result<Vec<PathBuf>> {
        self.ensure_dir().await?;
        let mut files = Vec::new();
        for entry in walkdir::WalkDir::new(&self.root_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
                files.push(entry.path().to_path_buf());
            }
        }
        Ok(files)
    }

    pub async fn read_all_entries(&self) -> anyhow::Result<Vec<(NaiveDate, String)>> {
        let files = self.list_files().await?;
        let mut entries = Vec::new();
        for file in files {
            if let Some(date_str) = file.file_stem().and_then(|s| s.to_str()) {
                if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    if let Some(content) = self.read_file(date).await? {
                        entries.push((date, content));
                    }
                }
            }
        }
        entries.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(entries)
    }

    pub async fn get_today_entries(&self) -> anyhow::Result<Vec<(String, String)>> {
        let today = Utc::now().date_naive();
        let content = match self.read_file(today).await? {
            Some(c) => c,
            None => return Ok(Vec::new()),
        };

        let mut entries = Vec::new();
        let mut current_heading = String::new();
        let mut current_content = String::new();

        for line in content.lines() {
            if line.starts_with("## ") {
                if !current_heading.is_empty() && !current_content.is_empty() {
                    entries.push((current_heading.clone(), current_content.trim().to_string()));
                }
                current_heading = line[3..].to_string();
                current_content = String::new();
            } else if !line.starts_with("#") && !line.starts_with("---") {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }

        if !current_heading.is_empty() && !current_content.is_empty() {
            entries.push((current_heading, current_content.trim().to_string()));
        }

        Ok(entries)
    }

    pub async fn count_today_paragraphs(&self) -> anyhow::Result<usize> {
        Ok(self.get_today_entries().await?.len())
    }

    pub async fn count_entity_mentions_today(&self, entity_name: &str) -> anyhow::Result<usize> {
        let entries = self.get_today_entries().await?;
        let mut count = 0;
        for (_heading, content) in entries {
            count += content.matches(entity_name).count();
        }
        Ok(count)
    }
}