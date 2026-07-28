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
}
