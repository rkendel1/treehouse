use serde::{Deserialize, Serialize};
use treehouse_drift::{detect_drift, DriftEvent, OwnershipPolicy};
use treehouse_system_graph::SystemGraphVersion;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchitectureChangeEvent {
    pub event: String,
    pub changes: Vec<String>,
    pub drift_events: Vec<DriftEvent>,
}

pub fn detect_architecture_change(
    previous: Option<&SystemGraphVersion>,
    current: &SystemGraphVersion,
    ownership: &[OwnershipPolicy],
) -> Option<ArchitectureChangeEvent> {
    let drift_events = detect_drift(previous, current, ownership);
    let mut changes = Vec::new();

    if let Some(before) = previous {
        for subsystem in &current.subsystems {
            let existed = before.subsystems.iter().any(|old| old.id == subsystem.id);
            if !existed {
                changes.push(format!("new subsystem {}", subsystem.id));
            }
        }
    }

    for subsystem in &current.subsystems {
        if subsystem.entities.iter().any(|entity| {
            entity.eq_ignore_ascii_case("subscription") || entity.eq_ignore_ascii_case("invoice")
        }) {
            changes.push("new entity Subscription/Invoice family".to_string());
            break;
        }
    }

    if changes.is_empty() && drift_events.is_empty() {
        return None;
    }

    Some(ArchitectureChangeEvent {
        event: "architecture_change".to_string(),
        changes,
        drift_events,
    })
}

#[cfg(test)]
mod tests {
    use treehouse_system_graph::{build_system_graph_version, Subsystem};

    use super::*;

    #[test]
    fn emits_architecture_change_event() {
        let previous = build_system_graph_version(
            1,
            vec![Subsystem {
                id: "Identity".to_string(),
                entities: vec!["User".to_string()],
                ..Subsystem::default()
            }],
            vec![],
        );
        let current = build_system_graph_version(
            2,
            vec![
                Subsystem {
                    id: "Identity".to_string(),
                    entities: vec!["User".to_string()],
                    ..Subsystem::default()
                },
                Subsystem {
                    id: "Billing".to_string(),
                    entities: vec!["Subscription".to_string()],
                    ..Subsystem::default()
                },
            ],
            vec![],
        );

        let event = detect_architecture_change(Some(&previous), &current, &[]).unwrap();
        assert_eq!(event.event, "architecture_change");
        assert!(event
            .changes
            .iter()
            .any(|change| change.contains("new subsystem Billing")));
    }
}
