use crate::schema::{WikiEntityCategory, WikiEntityFrontmatter, WikiEntityStatus};
use serde_yaml;
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone)]
pub struct WikiEntity {
    pub name: String,
    pub frontmatter: WikiEntityFrontmatter,
    pub body: String,
}

#[derive(Debug)]
pub struct WikiEntityLayer {
    root_dir: PathBuf,
}

impl WikiEntityLayer {
    pub fn new(root_dir: &Path) -> Self {
        Self {
            root_dir: root_dir.join("Wiki").join("Entities"),
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
        category: WikiEntityCategory,
        aliases: Vec<String>,
        tags: Vec<String>,
        status: WikiEntityStatus,
        confidence: f64,
        derived_from: Vec<String>,
        body: &str,
    ) -> anyhow::Result<()> {
        self.ensure_dir().await?;
        let file_path = self.get_file_path(name);

        let version = if file_path.exists() {
            self.read(name)
                .await?
                .map_or(1, |e| e.frontmatter.version + 1)
        } else {
            1
        };

        let frontmatter = WikiEntityFrontmatter {
            doc_type: "wiki_entity".to_string(),
            category,
            aliases,
            tags,
            status,
            confidence,
            derived_from,
            version,
        };

        frontmatter.validate()?;

        let yaml = serde_yaml::to_string(&frontmatter)?;
        let content = format!("---\n{yaml}---\n# {name}\n\n{body}");

        fs::write(&file_path, content).await?;
        Ok(())
    }

    pub async fn read(&self, name: &str) -> anyhow::Result<Option<WikiEntity>> {
        let file_path = self.get_file_path(name);
        if !file_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&file_path).await?;

        let (yaml, body) = match crate::schema::parse_yaml_frontmatter(&content) {
            Some((y, b)) => (y, b.to_string()),
            None => {
                return Err(anyhow::anyhow!(
                    "Invalid YAML frontmatter in {}",
                    file_path.display()
                ))
            }
        };

        let frontmatter: WikiEntityFrontmatter = serde_yaml::from_value(yaml)?;
        frontmatter.validate()?;

        Ok(Some(WikiEntity {
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

    pub async fn update_confidence(&self, name: &str, new_confidence: f64) -> anyhow::Result<()> {
        let mut entity = self
            .read(name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Entity {} not found", name))?;
        entity.frontmatter.confidence = new_confidence;
        entity.frontmatter.version += 1;
        self.write_entity(&entity).await
    }

    pub async fn add_derived_from(&self, name: &str, source: &str) -> anyhow::Result<()> {
        let mut entity = self
            .read(name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Entity {} not found", name))?;
        if !entity.frontmatter.derived_from.iter().any(|s| s == source) {
            entity.frontmatter.derived_from.push(source.to_string());
            entity.frontmatter.version += 1;
            self.write_entity(&entity).await?;
        }
        Ok(())
    }

    pub async fn write_entity(&self, entity: &WikiEntity) -> anyhow::Result<()> {
        self.ensure_dir().await?;
        let file_path = self.get_file_path(&entity.name);

        let yaml = serde_yaml::to_string(&entity.frontmatter)?;
        let content = format!("---\n{yaml}---\n# {}\n\n{}", entity.name, entity.body);

        fs::write(&file_path, content).await?;
        Ok(())
    }

    pub async fn rename(&self, old_name: &str, new_name: &str) -> anyhow::Result<()> {
        let old_path = self.get_file_path(old_name);
        let new_path = self.get_file_path(new_name);

        if !old_path.exists() {
            return Err(anyhow::anyhow!("Entity {} not found", old_name));
        }

        if new_path.exists() {
            return Err(anyhow::anyhow!("Entity {} already exists", new_name));
        }

        fs::rename(old_path, new_path).await?;

        if let Some(mut entity) = self.read(new_name).await? {
            entity.name = new_name.to_string();
            self.write_entity(&entity).await?;
        }

        Ok(())
    }
}
