use serde::{Deserialize, Serialize};

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
}
