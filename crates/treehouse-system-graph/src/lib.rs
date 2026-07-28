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
}
