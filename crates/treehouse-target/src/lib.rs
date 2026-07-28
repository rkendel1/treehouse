use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchitectureStyle {
    EventDriven,
    ModularMonolith,
    Layered,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanTarget {
    pub name: String,
    pub description: String,
    pub constraints: Vec<String>,
    pub desired_capabilities: Vec<String>,
    pub style: ArchitectureStyle,
}

impl ScanTarget {
    pub fn slug(&self) -> String {
        sanitize_target_name(&self.name)
    }
}

pub fn load_scan_target(repo_root: &Path, target: &str) -> Result<ScanTarget> {
    let target_path = resolve_target_path(repo_root, target)?;
    let content = fs::read_to_string(&target_path)
        .with_context(|| format!("failed to read target file {}", target_path.display()))?;
    let mut parsed = parse_target_markdown(&content);
    if parsed.name.is_empty() {
        parsed.name = target_path
            .file_stem()
            .and_then(|part| part.to_str())
            .unwrap_or("target")
            .to_string();
    }
    Ok(parsed)
}

pub fn parse_target_markdown(markdown: &str) -> ScanTarget {
    let mut name = String::new();
    let mut constraints = Vec::new();
    let mut desired_capabilities = Vec::new();
    let mut description_lines = Vec::new();

    enum Section {
        General,
        Constraints,
        Capabilities,
    }

    let mut section = Section::General;

    for raw_line in markdown.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line.strip_prefix("# ") {
            if name.is_empty() {
                name = header.trim().to_string();
            }
            continue;
        }
        if line.starts_with("##") {
            let lower = line.to_ascii_lowercase();
            section = if lower.contains("constraint") {
                Section::Constraints
            } else if lower.contains("capabil") || lower.contains("goal") {
                Section::Capabilities
            } else {
                Section::General
            };
            continue;
        }

        if let Some(item) = line.strip_prefix("- ") {
            match section {
                Section::Constraints => constraints.push(item.trim().to_string()),
                Section::Capabilities => desired_capabilities.push(item.trim().to_string()),
                Section::General => desired_capabilities.push(item.trim().to_string()),
            }
            continue;
        }

        if description_lines.is_empty() {
            description_lines.push(line.to_string());
        }
    }

    let description = if description_lines.is_empty() {
        "Target architecture".to_string()
    } else {
        description_lines.join(" ")
    };

    ScanTarget {
        name,
        description,
        constraints,
        desired_capabilities,
        style: infer_style(markdown),
    }
}

pub fn sanitize_target_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();

    cleaned
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn resolve_target_path(repo_root: &Path, target: &str) -> Result<PathBuf> {
    let provided = PathBuf::from(target);
    if provided.exists() {
        return Ok(provided);
    }

    let candidates = [
        repo_root.join("targets").join(target),
        repo_root.join("targets").join(format!("{target}.md")),
    ];

    for candidate in candidates {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    bail!("target `{target}` not found. Provide a file path or create targets/{target}.md")
}

fn infer_style(markdown: &str) -> ArchitectureStyle {
    let lowered = markdown.to_ascii_lowercase();
    if lowered.contains("event-driven") || lowered.contains("event driven") {
        ArchitectureStyle::EventDriven
    } else if lowered.contains("modular monolith") {
        ArchitectureStyle::ModularMonolith
    } else if lowered.contains("layered") {
        ArchitectureStyle::Layered
    } else {
        ArchitectureStyle::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markdown_target() {
        let target = parse_target_markdown(
            "# Event Driven\nBuild around events\n## Constraints\n- keep local\n## Capabilities\n- emit events",
        );

        assert_eq!(target.name, "Event Driven");
        assert_eq!(target.description, "Build around events");
        assert_eq!(target.constraints, vec!["keep local"]);
        assert_eq!(target.desired_capabilities, vec!["emit events"]);
        assert_eq!(target.style, ArchitectureStyle::EventDriven);
    }

    #[test]
    fn resolves_named_target_from_targets_directory() {
        let temp = std::env::temp_dir().join("treehouse-target-load-test");
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(temp.join("targets")).unwrap();
        fs::write(temp.join("targets/event-driven.md"), "# Event Driven").unwrap();

        let target = load_scan_target(&temp, "event-driven").unwrap();
        assert_eq!(target.name, "Event Driven");
    }
}
