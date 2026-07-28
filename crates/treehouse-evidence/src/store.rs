use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::{EvidenceEdge, EvidenceNode, EvidenceQuery, EvidenceSnapshot, RelationKind};

pub trait EvidenceStore {
    fn append_node(&self, node: &EvidenceNode) -> Result<()>;
    fn append_edge(&self, edge: &EvidenceEdge) -> Result<()>;
    fn snapshot(&self) -> Result<EvidenceSnapshot>;
    fn query(&self, query: &EvidenceQuery) -> Result<Vec<EvidenceNode>>;
}

#[derive(Debug, Clone)]
pub struct FileEvidenceStore {
    base_path: PathBuf,
}

impl FileEvidenceStore {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    fn nodes_path(&self) -> PathBuf {
        self.base_path.join("nodes.jsonl")
    }

    fn edges_path(&self) -> PathBuf {
        self.base_path.join("edges.jsonl")
    }

    fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.base_path)
            .with_context(|| format!("failed creating {}", self.base_path.display()))
    }

    pub fn detect_conflicts(snapshot: &EvidenceSnapshot) -> Vec<EvidenceEdge> {
        snapshot
            .edges
            .iter()
            .filter(|edge| {
                edge.relation == RelationKind::Contradicts
                    && edge.confidence.score >= 0.8
                    && snapshot
                        .nodes
                        .iter()
                        .any(|node| node.id == edge.from && node.confidence.score >= 0.8)
                    && snapshot
                        .nodes
                        .iter()
                        .any(|node| node.id == edge.to && node.confidence.score >= 0.8)
            })
            .cloned()
            .collect()
    }
}

impl EvidenceStore for FileEvidenceStore {
    fn append_node(&self, node: &EvidenceNode) -> Result<()> {
        self.ensure_dirs()?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.nodes_path())
            .context("failed opening node evidence file")?;
        let line = serde_json::to_string(node)?;
        writeln!(file, "{line}").context("failed writing node evidence")?;
        Ok(())
    }

    fn append_edge(&self, edge: &EvidenceEdge) -> Result<()> {
        self.ensure_dirs()?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.edges_path())
            .context("failed opening edge evidence file")?;
        let line = serde_json::to_string(edge)?;
        writeln!(file, "{line}").context("failed writing edge evidence")?;
        Ok(())
    }

    fn snapshot(&self) -> Result<EvidenceSnapshot> {
        let nodes = read_json_lines::<EvidenceNode>(&self.nodes_path())?;
        let edges = read_json_lines::<EvidenceEdge>(&self.edges_path())?;
        let observed_through_unix = nodes
            .last()
            .map(|node| node.observed_at_unix)
            .or_else(|| edges.last().map(|edge| edge.observed_at_unix))
            .unwrap_or(0);
        Ok(EvidenceSnapshot {
            observed_through_unix,
            nodes,
            edges,
        })
    }

    fn query(&self, query: &EvidenceQuery) -> Result<Vec<EvidenceNode>> {
        let snapshot = self.snapshot()?;
        Ok(snapshot
            .nodes
            .into_iter()
            .filter(|node| query.matches(node))
            .collect())
    }
}

fn read_json_lines<T>(path: &Path) -> Result<Vec<T>>
where
    T: serde::de::DeserializeOwned,
{
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content =
        fs::read_to_string(path).with_context(|| format!("failed reading {}", path.display()))?;
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push(
            serde_json::from_str(trimmed).with_context(|| {
                format!("failed parsing JSON evidence line in {}", path.display())
            })?,
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::{Confidence, EvidenceKind, Provenance, SourceKind};

    #[test]
    fn appends_and_queries_nodes() {
        let root = std::env::temp_dir().join("treehouse-evidence-store-test");
        let _ = fs::remove_dir_all(&root);
        let store = FileEvidenceStore::new(&root);

        let node = EvidenceNode::new(
            EvidenceKind::Entity {
                name: "Invoice".to_string(),
            },
            50,
            Confidence::new(0.9, Some("inferred model".to_string())),
            Provenance::new(SourceKind::Entity, "snapshot", "observer"),
            Some("Billing".to_string()),
            serde_json::json!({"source":"fixture"}),
        );
        store.append_node(&node).unwrap();

        let found = store
            .query(&EvidenceQuery::new().kind("entity").since_unix(40))
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, node.id);
    }

    #[test]
    fn detects_high_confidence_conflicts() {
        let left = EvidenceNode::new(
            EvidenceKind::Entity {
                name: "Invoice".to_string(),
            },
            1,
            Confidence::new(0.95, None),
            Provenance::new(SourceKind::Entity, "left", "observer"),
            None,
            serde_json::Value::Null,
        );
        let right = EvidenceNode::new(
            EvidenceKind::Entity {
                name: "Invoice".to_string(),
            },
            2,
            Confidence::new(0.90, None),
            Provenance::new(SourceKind::Entity, "right", "observer"),
            None,
            serde_json::Value::Null,
        );

        let snapshot = EvidenceSnapshot {
            observed_through_unix: 2,
            nodes: vec![left.clone(), right.clone()],
            edges: vec![EvidenceEdge {
                from: left.id,
                to: right.id,
                relation: RelationKind::Contradicts,
                confidence: Confidence::new(0.91, None),
                observed_at_unix: 2,
            }],
        };

        let conflicts = FileEvidenceStore::detect_conflicts(&snapshot);
        assert_eq!(conflicts.len(), 1);
    }
}
