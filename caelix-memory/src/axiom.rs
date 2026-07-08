use crate::schema::{AxiomCategory, AxiomFrontmatter, AxiomStatus};
use chrono::{DateTime, Utc};
use serde_yaml;
use std::path::{Path, PathBuf};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct Axiom {
    pub name: String,
    pub frontmatter: AxiomFrontmatter,
    pub body: String,
}

#[derive(Debug)]
pub struct AxiomLayer {
    root_dir: PathBuf,
}

impl AxiomLayer {
    pub fn new(root_dir: &Path) -> Self {
        Self {
            root_dir: root_dir.join("Axioms"),
        }
    }

    pub async fn ensure_dir(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.root_dir).await?;
        fs::create_dir_all(self.root_dir.join("methodology")).await?;
        fs::create_dir_all(self.root_dir.join("rules")).await?;
        fs::create_dir_all(self.root_dir.join("principles")).await?;
        Ok(())
    }

    pub fn get_file_path(&self, category: AxiomCategory, name: &str) -> PathBuf {
        let category_dir = match category {
            AxiomCategory::Methodology => "methodology",
            AxiomCategory::Rule => "rules",
            AxiomCategory::Principle => "principles",
        };
        self.root_dir
            .join(category_dir)
            .join(format!("{}.md", name))
    }

    pub async fn exists(&self, category: AxiomCategory, name: &str) -> bool {
        self.get_file_path(category, name).exists()
    }

    pub async fn write(
        &self,
        name: &str,
        category: AxiomCategory,
        confidence: f64,
        derived_from: Vec<String>,
        contradicts: Vec<String>,
        body: &str,
    ) -> anyhow::Result<()> {
        self.ensure_dir().await?;
        let file_path = self.get_file_path(category.clone(), name);

        if file_path.exists() {
            return Err(anyhow::anyhow!("Axiom {} already exists", name));
        }

        let frontmatter = AxiomFrontmatter {
            doc_type: "axiom".to_string(),
            category,
            confidence,
            status: AxiomStatus::Active,
            derived_from,
            contradicts,
            deprecated_by: None,
            deprecated_reason: None,
            deprecated_at: None,
            replaced_by: None,
            version: 1,
            created_at: Utc::now(),
            last_reviewed: None,
        };

        frontmatter.validate()?;

        let yaml = serde_yaml::to_string(&frontmatter)?;
        let content = format!("---\n{yaml}---\n# {name}\n\n{body}");

        fs::write(&file_path, content).await?;
        Ok(())
    }

    pub async fn read(&self, category: AxiomCategory, name: &str) -> anyhow::Result<Option<Axiom>> {
        let file_path = self.get_file_path(category, name);
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

        let frontmatter: AxiomFrontmatter = serde_yaml::from_value(yaml)?;
        frontmatter.validate()?;

        Ok(Some(Axiom {
            name: name.to_string(),
            frontmatter,
            body,
        }))
    }

    pub async fn read_by_path(&self, path: &Path) -> anyhow::Result<Option<Axiom>> {
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(path).await?;

        let (yaml, body) = match crate::schema::parse_yaml_frontmatter(&content) {
            Some((y, b)) => (y, b.to_string()),
            None => {
                return Err(anyhow::anyhow!(
                    "Invalid YAML frontmatter in {}",
                    path.display()
                ))
            }
        };

        let frontmatter: AxiomFrontmatter = serde_yaml::from_value(yaml)?;
        frontmatter.validate()?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        Ok(Some(Axiom {
            name,
            frontmatter,
            body,
        }))
    }

    pub async fn list(&self, category: Option<AxiomCategory>) -> anyhow::Result<Vec<String>> {
        self.ensure_dir().await?;
        let mut names = Vec::new();

        let dirs = if let Some(cat) = category {
            vec![self
                .get_file_path(cat, "dummy")
                .parent()
                .unwrap()
                .to_path_buf()]
        } else {
            vec![
                self.root_dir.join("methodology"),
                self.root_dir.join("rules"),
                self.root_dir.join("principles"),
            ]
        };

        for dir in dirs {
            for entry in walkdir::WalkDir::new(dir)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if let Some(name) = entry.path().file_stem().and_then(|s| s.to_str()) {
                    names.push(name.to_string());
                }
            }
        }
        names.sort();
        Ok(names)
    }

    pub async fn list_all(&self) -> anyhow::Result<Vec<Axiom>> {
        let mut axioms = Vec::new();
        for category in [
            AxiomCategory::Methodology,
            AxiomCategory::Rule,
            AxiomCategory::Principle,
        ] {
            let names = self.list(Some(category.clone())).await?;
            for name in names {
                if let Some(axiom) = self.read(category.clone(), &name).await? {
                    axioms.push(axiom);
                }
            }
        }
        Ok(axioms)
    }

    pub async fn list_active(&self) -> anyhow::Result<Vec<Axiom>> {
        let all = self.list_all().await?;
        Ok(all
            .into_iter()
            .filter(|a| a.frontmatter.status == AxiomStatus::Active)
            .collect())
    }

    pub async fn deprecate(
        &self,
        category: AxiomCategory,
        name: &str,
        deprecated_by: &str,
        deprecated_reason: &str,
        replaced_by: Option<String>,
    ) -> anyhow::Result<()> {
        let mut axiom = self
            .read(category, name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Axiom {} not found", name))?;

        axiom.frontmatter.status = AxiomStatus::Deprecated;
        axiom.frontmatter.deprecated_by = Some(deprecated_by.to_string());
        axiom.frontmatter.deprecated_reason = Some(deprecated_reason.to_string());
        axiom.frontmatter.deprecated_at = Some(Utc::now());
        axiom.frontmatter.replaced_by = replaced_by;
        axiom.frontmatter.version += 1;

        self.write_axiom(&axiom).await
    }

    pub async fn add_contradiction(
        &self,
        category: AxiomCategory,
        name: &str,
        other: &str,
    ) -> anyhow::Result<()> {
        let mut axiom = self
            .read(category, name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Axiom {} not found", name))?;

        if !axiom.frontmatter.contradicts.iter().any(|s| s == other) {
            axiom.frontmatter.contradicts.push(other.to_string());
            axiom.frontmatter.version += 1;
            self.write_axiom(&axiom).await?;
        }
        Ok(())
    }

    pub async fn write_axiom(&self, axiom: &Axiom) -> anyhow::Result<()> {
        self.ensure_dir().await?;
        let file_path = self.get_file_path(axiom.frontmatter.category.clone(), &axiom.name);

        let yaml = serde_yaml::to_string(&axiom.frontmatter)?;
        let content = format!("---\n{yaml}---\n# {}\n\n{}", axiom.name, axiom.body);

        fs::write(&file_path, content).await?;
        Ok(())
    }
}
