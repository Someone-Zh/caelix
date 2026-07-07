use crate::schema::{WikiEventFrontmatter, WikiEventStatus};
use chrono::{NaiveDate, Utc};
use serde_yaml;
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct WikiEvent {
    pub name: String,
    pub frontmatter: WikiEventFrontmatter,
    pub body: String,
}

#[derive(Debug)]
pub struct WikiEventLayer {
    root_dir: PathBuf,
}

impl WikiEventLayer {
    pub fn new(root_dir: &Path) -> Self {
        Self {
            root_dir: root_dir.join("Wiki").join("Events"),
        }
    }

    pub async fn ensure_dir(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        Ok(())
    }

    pub fn get_file_path(&self, name: &str) -> PathBuf {
        self.root_dir.join(format!("{}.md", name))
    }

    pub async fn exists(&self, name: &str) -> bool {
        self.get_file_path(name).exists()
    }

    pub async fn write(
        &self,
        name: &str,
        date_range: Vec<NaiveDate>,
        status: WikiEventStatus,
        confidence: f64,
        participants: Vec<String>,
        related_entities: Vec<String>,
        derived_from: Vec<String>,
        body: &str,
    ) -> anyhow::Result<()> {
        self.ensure_dir().await?;
        let file_path = self.get_file_path(name);

        let version = if file_path.exists() {
            self.read(name).await?.map_or(1, |e| e.frontmatter.version + 1)
        } else {
            1
        };

        let frontmatter = WikiEventFrontmatter {
            doc_type: "wiki_event".to_string(),
            date_range,
            status,
            confidence,
            participants,
            related_entities,
            derived_from,
            version,
        };

        frontmatter.validate()?;

        let yaml = serde_yaml::to_string(&frontmatter)?;
        let content = format!("---\n{yaml}---\n# {name}\n\n{body}");

        fs::write(&file_path, content).await?;
        Ok(())
    }

    pub async fn read(&self, name: &str) -> anyhow::Result<Option<WikiEvent>> {
        let file_path = self.get_file_path(name);
        if !file_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&file_path).await?;

        let (yaml, body) = match crate::schema::parse_yaml_frontmatter(&content) {
            Some((y, b)) => (y, b.to_string()),
            None => {
                return Err(anyhow::anyhow!("Invalid YAML frontmatter in {}", file_path.display()))
            }
        };

        let frontmatter: WikiEventFrontmatter = serde_yaml::from_value(yaml)?;
        frontmatter.validate()?;

        Ok(Some(WikiEvent {
            name: name.to_string(),
            frontmatter,
            body,
        }))
    }

    pub async fn list(&self) -> anyhow::Result<Vec<String>> {
        self.ensure_dir().await?;
        let mut names = Vec::new();
        for entry in walkdir::WalkDir::new(&self.root_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Some(name) = entry.path().file_stem().and_then(|s| s.to_str()) {
                    names.push(name.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    pub async fn update_status(&self, name: &str, new_status: WikiEventStatus) -> anyhow::Result<()> {
        let mut event = self.read(name).await?.ok_or_else(|| {
            anyhow::anyhow!("Event {} not found", name)
        })?;
        event.frontmatter.status = new_status;
        event.frontmatter.version += 1;
        self.write_event(&event).await
    }

    pub async fn write_event(&self, event: &WikiEvent) -> anyhow::Result<()> {
        self.ensure_dir().await?;
        let file_path = self.get_file_path(&event.name);

        let yaml = serde_yaml::to_string(&event.frontmatter)?;
        let content = format!("---\n{yaml}---\n# {}\n\n{}", event.name, event.body);

        fs::write(&file_path, content).await?;
        Ok(())
    }

    pub async fn rename(&self, old_name: &str, new_name: &str) -> anyhow::Result<()> {
        let old_path = self.get_file_path(old_name);
        let new_path = self.get_file_path(new_name);

        if !old_path.exists() {
            return Err(anyhow::anyhow!("Event {} not found", old_name));
        }

        if new_path.exists() {
            return Err(anyhow::anyhow!("Event {} already exists", new_name));
        }

        fs::rename(old_path, new_path).await?;

        if let Some(mut event) = self.read(new_name).await? {
            event.name = new_name.to_string();
            self.write_event(&event).await?;
        }

        Ok(())
    }
}