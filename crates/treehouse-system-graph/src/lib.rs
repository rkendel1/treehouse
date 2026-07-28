use serde::{Deserialize, Serialize};
use treehouse_evidence::{EvidenceKind, EvidenceSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Subsystem {
    pub id: String,
    pub entities: Vec<String>,
    pub apis: Vec<String>,
    pub workflows: Vec<String>,
    pub events: Vec<String>,
    pub owner: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SystemGraphVersion {
    pub version: u64,
    pub architecture_confidence: f32,
    pub subsystems: Vec<Subsystem>,
    pub relationships: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SystemGraphTimeline {
    pub versions: Vec<SystemGraphVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum KnowledgeNodeType {
    Repository,
    Subsystem,
    Capability,
    Api,
    Workflow,
    Symbol,
    Migration,
    RuntimeEvent,
    Finding,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum KnowledgeEdgeType {
    Owns,
    Exposes,
    DependsOn,
    Observes,
    Produces,
    Documents,
    Violates,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KnowledgeNode {
    pub id: String,
    pub node_type: KnowledgeNodeType,
    pub name: String,
    pub first_seen_unix: u64,
    pub last_seen_unix: u64,
    pub confidence: f32,
    pub owner: Option<String>,
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KnowledgeEdge {
    pub from: String,
    pub to: String,
    pub edge_type: KnowledgeEdgeType,
    pub observed_at_unix: u64,
    pub confidence: f32,
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct KnowledgeDrift {
    pub severity: String,
    pub title: String,
    pub message: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct KnowledgeGraph {
    pub repository: String,
    pub version: u64,
    pub generated_at_unix: u64,
    pub nodes: Vec<KnowledgeNode>,
    pub edges: Vec<KnowledgeEdge>,
    pub drifts: Vec<KnowledgeDrift>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct KnowledgeTimeline {
    pub entries: Vec<KnowledgeTimelineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct KnowledgeTimelineEntry {
    pub version: u64,
    pub generated_at_unix: u64,
    pub nodes: usize,
    pub edges: usize,
    pub drift_events: usize,
}

pub fn build_system_graph_version(
    version: u64,
    mut subsystems: Vec<Subsystem>,
    relationships: Vec<String>,
) -> SystemGraphVersion {
    subsystems.sort_by(|a, b| a.id.cmp(&b.id));
    let architecture_confidence = if subsystems.is_empty() {
        0.0
    } else {
        subsystems.iter().map(|s| s.confidence).sum::<f32>() / subsystems.len() as f32
    };
    SystemGraphVersion {
        version,
        architecture_confidence,
        subsystems,
        relationships,
    }
}

pub fn append_graph_version(
    timeline: &mut SystemGraphTimeline,
    version: SystemGraphVersion,
    keep_last: usize,
) {
    timeline.versions.push(version);
    if keep_last > 0 && timeline.versions.len() > keep_last {
        let overflow = timeline.versions.len() - keep_last;
        timeline.versions.drain(0..overflow);
    }
}

pub fn latest_graph_version(timeline: &SystemGraphTimeline) -> Option<&SystemGraphVersion> {
    timeline.versions.last()
}

pub fn append_knowledge_timeline_entry(
    timeline: &mut KnowledgeTimeline,
    graph: &KnowledgeGraph,
    keep_last: usize,
) {
    timeline.entries.push(KnowledgeTimelineEntry {
        version: graph.version,
        generated_at_unix: graph.generated_at_unix,
        nodes: graph.nodes.len(),
        edges: graph.edges.len(),
        drift_events: graph.drifts.len(),
    });
    if keep_last > 0 && timeline.entries.len() > keep_last {
        let overflow = timeline.entries.len() - keep_last;
        timeline.entries.drain(0..overflow);
    }
}

pub fn build_system_graph_from_evidence_snapshot(
    version: u64,
    snapshot: &EvidenceSnapshot,
) -> SystemGraphVersion {
    let mut by_subsystem = std::collections::BTreeMap::<String, Subsystem>::new();
    for node in &snapshot.nodes {
        let subsystem_name = node
            .subsystem
            .clone()
            .unwrap_or_else(|| "Unassigned".to_string());
        let subsystem = by_subsystem
            .entry(subsystem_name.clone())
            .or_insert_with(|| Subsystem {
                id: subsystem_name.clone(),
                ..Subsystem::default()
            });
        match &node.kind {
            EvidenceKind::Entity { name } => subsystem.entities.push(name.clone()),
            EvidenceKind::ApiSurface { method, path } => {
                subsystem.apis.push(format!("{method} {path}"));
            }
            EvidenceKind::Workflow { name } => subsystem.workflows.push(name.clone()),
            EvidenceKind::RuntimeEvent { event } => subsystem.events.push(event.clone()),
            _ => {}
        }
        subsystem.confidence = subsystem.confidence.max(node.confidence.score);
    }
    let mut subsystems: Vec<Subsystem> = by_subsystem.into_values().collect();
    for subsystem in &mut subsystems {
        subsystem.entities.sort();
        subsystem.entities.dedup();
        subsystem.apis.sort();
        subsystem.apis.dedup();
        subsystem.workflows.sort();
        subsystem.workflows.dedup();
        subsystem.events.sort();
        subsystem.events.dedup();
    }
    build_system_graph_version(version, subsystems, vec![])
}

pub fn build_knowledge_graph_from_evidence_snapshot(
    repository: &str,
    version: u64,
    snapshot: &EvidenceSnapshot,
    drifts: Vec<KnowledgeDrift>,
) -> KnowledgeGraph {
    use std::collections::{BTreeMap, BTreeSet};

    let repository_id = canonical_node_id(KnowledgeNodeType::Repository, repository);
    let mut nodes = BTreeMap::<String, KnowledgeNode>::new();
    let mut edges = BTreeSet::<(String, String, KnowledgeEdgeType)>::new();

    nodes.insert(
        repository_id.clone(),
        KnowledgeNode {
            id: repository_id.clone(),
            node_type: KnowledgeNodeType::Repository,
            name: repository.to_string(),
            first_seen_unix: version,
            last_seen_unix: version,
            confidence: 1.0,
            owner: None,
            attributes: serde_json::json!({}),
        },
    );

    for evidence in &snapshot.nodes {
        let (node_type, name, owner) = match &evidence.kind {
            EvidenceKind::Entity { name } => (
                KnowledgeNodeType::Capability,
                name.clone(),
                evidence.subsystem.clone(),
            ),
            EvidenceKind::ApiSurface { method, path } => {
                (KnowledgeNodeType::Api, format!("{method} {path}"), None)
            }
            EvidenceKind::Workflow { name } => (KnowledgeNodeType::Workflow, name.clone(), None),
            EvidenceKind::Symbol { name, .. } => (KnowledgeNodeType::Symbol, name.clone(), None),
            EvidenceKind::Migration { table, .. } => {
                (KnowledgeNodeType::Migration, table.clone(), None)
            }
            EvidenceKind::RuntimeEvent { event } => {
                (KnowledgeNodeType::RuntimeEvent, event.clone(), None)
            }
            EvidenceKind::SystemDiffFinding { finding } => {
                (KnowledgeNodeType::Finding, finding.clone(), None)
            }
            _ => continue,
        };

        let node_id = canonical_node_id(node_type.clone(), &name);
        nodes
            .entry(node_id.clone())
            .and_modify(|node| {
                node.last_seen_unix = node.last_seen_unix.max(evidence.observed_at_unix);
                node.confidence = node.confidence.max(evidence.confidence.score);
                if node.owner.is_none() {
                    node.owner = owner.clone();
                }
            })
            .or_insert_with(|| KnowledgeNode {
                id: node_id.clone(),
                node_type: node_type.clone(),
                name: name.clone(),
                first_seen_unix: evidence.observed_at_unix,
                last_seen_unix: evidence.observed_at_unix,
                confidence: evidence.confidence.score,
                owner: owner.clone(),
                attributes: evidence.attributes.clone(),
            });

        edges.insert((
            repository_id.clone(),
            node_id.clone(),
            KnowledgeEdgeType::Observes,
        ));

        if let Some(subsystem) = owner {
            let subsystem_id = canonical_node_id(KnowledgeNodeType::Subsystem, &subsystem);
            nodes.entry(subsystem_id.clone()).or_insert_with(|| KnowledgeNode {
                id: subsystem_id.clone(),
                node_type: KnowledgeNodeType::Subsystem,
                name: subsystem,
                first_seen_unix: evidence.observed_at_unix,
                last_seen_unix: evidence.observed_at_unix,
                confidence: evidence.confidence.score,
                owner: None,
                attributes: serde_json::json!({}),
            });
            edges.insert((
                subsystem_id,
                node_id.clone(),
                KnowledgeEdgeType::Owns,
            ));
        }

        if node_type == KnowledgeNodeType::Api {
            let api_lower = name.to_ascii_lowercase();
            for capability in nodes.values() {
                if capability.node_type != KnowledgeNodeType::Capability {
                    continue;
                }
                let token = capability.name.to_ascii_lowercase();
                if !token.is_empty() && api_lower.contains(&token) {
                    edges.insert((
                        node_id.clone(),
                        capability.id.clone(),
                        KnowledgeEdgeType::Exposes,
                    ));
                }
            }
        }
    }

    for drift in &drifts {
        let finding_id = canonical_node_id(
            KnowledgeNodeType::Finding,
            &format!("{} {}", drift.severity, drift.title),
        );
        nodes.entry(finding_id.clone()).or_insert_with(|| KnowledgeNode {
            id: finding_id.clone(),
            node_type: KnowledgeNodeType::Finding,
            name: drift.title.clone(),
            first_seen_unix: version,
            last_seen_unix: version,
            confidence: drift.confidence,
            owner: None,
            attributes: serde_json::json!({
                "severity": drift.severity,
                "message": drift.message,
            }),
        });
        edges.insert((
            repository_id.clone(),
            finding_id,
            KnowledgeEdgeType::Violates,
        ));
    }

    let edges: Vec<KnowledgeEdge> = edges
        .into_iter()
        .map(|(from, to, edge_type)| KnowledgeEdge {
            from,
            to,
            edge_type,
            observed_at_unix: version,
            confidence: 0.80,
            attributes: serde_json::json!({}),
        })
        .collect();

    KnowledgeGraph {
        repository: repository.to_string(),
        version,
        generated_at_unix: snapshot.observed_through_unix,
        nodes: nodes.into_values().collect(),
        edges,
        drifts,
    }
}

pub fn canonical_node_id(node_type: KnowledgeNodeType, name: &str) -> String {
    format!("{}/{}", node_type_slug(node_type), slugify(name))
}

fn node_type_slug(node_type: KnowledgeNodeType) -> &'static str {
    match node_type {
        KnowledgeNodeType::Repository => "repository",
        KnowledgeNodeType::Subsystem => "subsystem",
        KnowledgeNodeType::Capability => "capability",
        KnowledgeNodeType::Api => "api",
        KnowledgeNodeType::Workflow => "workflow",
        KnowledgeNodeType::Symbol => "symbol",
        KnowledgeNodeType::Migration => "migration",
        KnowledgeNodeType::RuntimeEvent => "event",
        KnowledgeNodeType::Finding => "finding",
    }
}

fn slugify(raw: &str) -> String {
    let mut slug = String::new();
    let mut pending_sep = false;
    for ch in raw.chars() {
        let keep = ch.is_ascii_alphanumeric();
        if keep {
            if pending_sep && !slug.is_empty() {
                slug.push('-');
            }
            pending_sep = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            pending_sep = true;
        }
    }
    if slug.is_empty() {
        "unknown".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_version_with_average_confidence() {
        let graph = build_system_graph_version(
            2,
            vec![
                Subsystem {
                    id: "Billing".to_string(),
                    confidence: 0.9,
                    ..Subsystem::default()
                },
                Subsystem {
                    id: "Identity".to_string(),
                    confidence: 0.7,
                    ..Subsystem::default()
                },
            ],
            vec!["Identity -> Billing".to_string()],
        );
        assert_eq!(graph.version, 2);
        assert!((graph.architecture_confidence - 0.8).abs() < 0.001);
    }

    #[test]
    fn appends_and_trims_timeline() {
        let mut timeline = SystemGraphTimeline::default();
        append_graph_version(
            &mut timeline,
            build_system_graph_version(1, vec![], vec![]),
            2,
        );
        append_graph_version(
            &mut timeline,
            build_system_graph_version(2, vec![], vec![]),
            2,
        );
        append_graph_version(
            &mut timeline,
            build_system_graph_version(3, vec![], vec![]),
            2,
        );
        assert_eq!(timeline.versions.len(), 2);
        assert_eq!(latest_graph_version(&timeline).unwrap().version, 3);
    }

    #[test]
    fn builds_system_graph_from_evidence_snapshot() {
        let snapshot = EvidenceSnapshot {
            observed_through_unix: 10,
            nodes: vec![treehouse_evidence::EvidenceNode::new(
                treehouse_evidence::EvidenceKind::Entity {
                    name: "Invoice".to_string(),
                },
                10,
                treehouse_evidence::Confidence::new(0.9, None),
                treehouse_evidence::Provenance::new(
                    treehouse_evidence::SourceKind::Entity,
                    "test",
                    "test",
                ),
                Some("Billing".to_string()),
                serde_json::Value::Null,
            )],
            edges: vec![],
        };
        let graph = build_system_graph_from_evidence_snapshot(1, &snapshot);
        assert_eq!(graph.subsystems.len(), 1);
        assert_eq!(graph.subsystems[0].id, "Billing");
        assert_eq!(graph.subsystems[0].entities, vec!["Invoice".to_string()]);
    }

    #[test]
    fn builds_knowledge_graph_with_stable_ids() {
        let snapshot = EvidenceSnapshot {
            observed_through_unix: 42,
            nodes: vec![
                treehouse_evidence::EvidenceNode::new(
                    treehouse_evidence::EvidenceKind::Entity {
                        name: "Provider Routing".to_string(),
                    },
                    42,
                    treehouse_evidence::Confidence::new(0.9, None),
                    treehouse_evidence::Provenance::new(
                        treehouse_evidence::SourceKind::Entity,
                        "test",
                        "test",
                    ),
                    Some("Planner".to_string()),
                    serde_json::Value::Null,
                ),
                treehouse_evidence::EvidenceNode::new(
                    treehouse_evidence::EvidenceKind::ApiSurface {
                        method: "GET".to_string(),
                        path: "/provider-routing".to_string(),
                    },
                    42,
                    treehouse_evidence::Confidence::new(0.8, None),
                    treehouse_evidence::Provenance::new(
                        treehouse_evidence::SourceKind::Api,
                        "test",
                        "test",
                    ),
                    None,
                    serde_json::Value::Null,
                ),
            ],
            edges: vec![],
        };

        let graph = build_knowledge_graph_from_evidence_snapshot("treehouse", 42, &snapshot, vec![]);
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.id == "capability/provider-routing"));
        assert!(graph
            .nodes
            .iter()
            .any(|node| node.id == "api/get-provider-routing"));
    }
}
