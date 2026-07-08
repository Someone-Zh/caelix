use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkType {
    Entity(String),
    Event(String),
    Axiom(String),
    Pending(String),
}

#[derive(Debug, Clone)]
pub struct Link {
    pub link_type: LinkType,
    pub original: String,
}

impl Link {
    pub fn parse(text: &str) -> Vec<Link> {
        let mut links = Vec::new();
        let re = regex::Regex::new(r"\[\[([^\]]+)\]\]").unwrap();

        for cap in re.captures_iter(text) {
            let content = cap[1].to_string();
            let original = format!("[[{}]]", content);

            if content.ends_with('?') {
                let name = content[..content.len() - 1].to_string();
                links.push(Link {
                    link_type: LinkType::Pending(name),
                    original,
                });
            } else if content.starts_with("Event:") {
                let name = content[6..].to_string();
                links.push(Link {
                    link_type: LinkType::Event(name),
                    original,
                });
            } else if content.starts_with("Axiom:") {
                let name = content[6..].to_string();
                links.push(Link {
                    link_type: LinkType::Axiom(name),
                    original,
                });
            } else {
                links.push(Link {
                    link_type: LinkType::Entity(content),
                    original,
                });
            }
        }

        links
    }

    pub fn extract_entity_names(text: &str) -> HashSet<String> {
        let links = Self::parse(text);
        links
            .into_iter()
            .filter_map(|l| match l.link_type {
                LinkType::Entity(name) => Some(name),
                _ => None,
            })
            .collect()
    }

    pub fn extract_all_linked_names(text: &str) -> HashSet<String> {
        let links = Self::parse(text);
        links
            .into_iter()
            .map(|l| match l.link_type {
                LinkType::Entity(name)
                | LinkType::Event(name)
                | LinkType::Axiom(name)
                | LinkType::Pending(name) => name,
            })
            .collect()
    }
}

pub struct LinkValidator {
    entity_names: HashSet<String>,
    event_names: HashSet<String>,
    axiom_names: HashSet<String>,
}

impl LinkValidator {
    pub fn new(
        entity_names: HashSet<String>,
        event_names: HashSet<String>,
        axiom_names: HashSet<String>,
    ) -> Self {
        Self {
            entity_names,
            event_names,
            axiom_names,
        }
    }

    pub fn validate(&self, links: &[Link]) -> Vec<Link> {
        let mut pending_links = Vec::new();

        for link in links {
            match &link.link_type {
                LinkType::Entity(name) => {
                    if !self.entity_names.contains(name) {
                        pending_links.push(link.clone());
                    }
                }
                LinkType::Event(name) => {
                    if !self.event_names.contains(name) {
                        pending_links.push(link.clone());
                    }
                }
                LinkType::Axiom(name) => {
                    if !self.axiom_names.contains(name) {
                        pending_links.push(link.clone());
                    }
                }
                LinkType::Pending(_) => {
                    pending_links.push(link.clone());
                }
            }
        }

        pending_links
    }

    pub fn has_dead_links(&self, links: &[Link]) -> bool {
        !self.validate(links).is_empty()
    }

    pub fn replace_entity_links(text: &str, old_name: &str, new_name: &str) -> String {
        let re = regex::Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
        re.replace_all(text, |caps: &regex::Captures| {
            let content = &caps[1];
            if content == old_name {
                format!("[[{}]]", new_name)
            } else if content.starts_with("Event:") && &content[6..] == old_name {
                format!("[[Event:{}]]", new_name)
            } else if content.starts_with("Axiom:") && &content[6..] == old_name {
                format!("[[Axiom:{}]]", new_name)
            } else {
                caps[0].to_string()
            }
        })
        .to_string()
    }
}

pub fn find_all_links_in_directory(
    dir: &std::path::Path,
) -> anyhow::Result<Vec<(std::path::PathBuf, Vec<Link>)>> {
    let mut results = Vec::new();

    for entry in walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.path().extension().and_then(|s| s.to_str()) == Some("md") {
            let content = std::fs::read_to_string(entry.path())?;
            let links = Link::parse(&content);
            if !links.is_empty() {
                results.push((entry.path().to_path_buf(), links));
            }
        }
    }

    Ok(results)
}
