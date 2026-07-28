use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use treehouse_system_graph::SystemGraphVersion;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DriftType {
    DuplicateCapability,
    SubsystemOverlap,
    OwnershipViolation,
    ArchitectureDrift,
    ModelFragmentation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Recommendation {
    pub action: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DriftEvent {
    pub drift_type: DriftType,
    pub affected_subsystems: Vec<String>,
    pub evidence: Vec<String>,
    pub recommendation: Recommendation,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct OwnershipPolicy {
    pub subsystem: String,
    pub owns: Vec<String>,
}

pub fn detect_drift(
    previous: Option<&SystemGraphVersion>,
    current: &SystemGraphVersion,
    ownership_policies: &[OwnershipPolicy],
) -> Vec<DriftEvent> {
    let mut events = Vec::new();
    events.extend(detect_duplicate_capabilities(current));
    events.extend(detect_subsystem_overlap(current));
    events.extend(detect_ownership_violations(current, ownership_policies));
    events.extend(detect_model_fragmentation(current));
    if let Some(before) = previous {
        events.extend(detect_architecture_drift(before, current));
    }
    events
}

fn detect_duplicate_capabilities(current: &SystemGraphVersion) -> Vec<DriftEvent> {
    let mut events = Vec::new();
    for (left_index, left) in current.subsystems.iter().enumerate() {
        for right in current.subsystems.iter().skip(left_index + 1) {
            let left_set: BTreeSet<&str> = left.entities.iter().map(String::as_str).collect();
            let right_set: BTreeSet<&str> = right.entities.iter().map(String::as_str).collect();
            let shared: Vec<String> = left_set
                .intersection(&right_set)
                .map(|entity| (*entity).to_string())
                .collect();
            if shared.len() >= 2 {
                events.push(DriftEvent {
                    drift_type: DriftType::DuplicateCapability,
                    affected_subsystems: vec![left.id.clone(), right.id.clone()],
                    evidence: shared,
                    recommendation: Recommendation {
                        action: "Merge".to_string(),
                        details: format!(
                            "Unify overlapping capabilities into `{}` to avoid parallel implementations.",
                            left.id
                        ),
                    },
                });
            }
        }
    }
    events
}

fn detect_subsystem_overlap(current: &SystemGraphVersion) -> Vec<DriftEvent> {
    let mut entity_owners: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for subsystem in &current.subsystems {
        for entity in &subsystem.entities {
            entity_owners
                .entry(entity.to_ascii_lowercase())
                .or_default()
                .push(subsystem.id.clone());
        }
    }
    entity_owners
        .into_iter()
        .filter_map(|(entity, owners)| {
            if owners.len() <= 1 {
                return None;
            }
            Some(DriftEvent {
                drift_type: DriftType::SubsystemOverlap,
                affected_subsystems: owners.clone(),
                evidence: vec![format!("Entity `{entity}` appears in multiple subsystems")],
                recommendation: Recommendation {
                    action: "Extend".to_string(),
                    details: "Consolidate ownership under a single subsystem boundary.".to_string(),
                },
            })
        })
        .collect()
}

fn detect_ownership_violations(
    current: &SystemGraphVersion,
    ownership_policies: &[OwnershipPolicy],
) -> Vec<DriftEvent> {
    let mut policy_owner: BTreeMap<String, String> = BTreeMap::new();
    for policy in ownership_policies {
        for entity in &policy.owns {
            policy_owner.insert(entity.to_ascii_lowercase(), policy.subsystem.clone());
        }
    }

    let mut events = Vec::new();
    for subsystem in &current.subsystems {
        for entity in &subsystem.entities {
            let key = entity.to_ascii_lowercase();
            if let Some(owner) = policy_owner.get(&key) {
                if owner != &subsystem.id {
                    events.push(DriftEvent {
                        drift_type: DriftType::OwnershipViolation,
                        affected_subsystems: vec![subsystem.id.clone(), owner.clone()],
                        evidence: vec![format!(
                            "`{}` owns `{entity}`, but it appears under `{}`",
                            owner, subsystem.id
                        )],
                        recommendation: Recommendation {
                            action: "Reassign ownership".to_string(),
                            details: format!("Move `{entity}` responsibilities to `{owner}`."),
                        },
                    });
                }
            }
        }
    }
    events
}

fn detect_architecture_drift(
    previous: &SystemGraphVersion,
    current: &SystemGraphVersion,
) -> Vec<DriftEvent> {
    let previous_ids: BTreeSet<&str> = previous.subsystems.iter().map(|s| s.id.as_str()).collect();
    let new_ids: Vec<String> = current
        .subsystems
        .iter()
        .filter(|s| !previous_ids.contains(s.id.as_str()))
        .map(|s| s.id.clone())
        .collect();

    if new_ids.is_empty() {
        return Vec::new();
    }

    vec![DriftEvent {
        drift_type: DriftType::ArchitectureDrift,
        affected_subsystems: new_ids.clone(),
        evidence: vec![format!("New subsystem(s) detected: {}", new_ids.join(", "))],
        recommendation: Recommendation {
            action: "Promote subsystem".to_string(),
            details: "Review and formalize boundaries for emerging subsystem(s).".to_string(),
        },
    }]
}

fn detect_model_fragmentation(current: &SystemGraphVersion) -> Vec<DriftEvent> {
    let mut synonyms = BTreeSet::new();
    for subsystem in &current.subsystems {
        let names: BTreeSet<String> = subsystem
            .entities
            .iter()
            .map(|entity| entity.to_ascii_lowercase())
            .collect();
        if names.contains("customer") && names.contains("account") {
            synonyms.insert(subsystem.id.clone());
        }
        if names.contains("customer") && names.contains("client") {
            synonyms.insert(subsystem.id.clone());
        }
    }

    if synonyms.is_empty() {
        return Vec::new();
    }

    vec![DriftEvent {
        drift_type: DriftType::ModelFragmentation,
        affected_subsystems: synonyms.into_iter().collect(),
        evidence: vec![
            "Competing identity model names detected (Customer/Account/Client).".to_string(),
        ],
        recommendation: Recommendation {
            action: "Rename".to_string(),
            details: "Converge on one canonical entity name to avoid divergence.".to_string(),
        },
    }]
}

#[cfg(test)]
mod tests {
    use treehouse_system_graph::{build_system_graph_version, Subsystem};

    use super::*;

    #[test]
    fn detects_multiple_drift_types() {
        let previous = build_system_graph_version(
            1,
            vec![Subsystem {
                id: "Billing".to_string(),
                entities: vec!["Invoice".to_string(), "Payment".to_string()],
                ..Subsystem::default()
            }],
            vec![],
        );
        let current = build_system_graph_version(
            2,
            vec![
                Subsystem {
                    id: "Billing".to_string(),
                    entities: vec![
                        "Invoice".to_string(),
                        "Payment".to_string(),
                        "Customer".to_string(),
                        "Account".to_string(),
                    ],
                    ..Subsystem::default()
                },
                Subsystem {
                    id: "Checkout".to_string(),
                    entities: vec!["Invoice".to_string(), "Payment".to_string()],
                    ..Subsystem::default()
                },
            ],
            vec![],
        );

        let events = detect_drift(
            Some(&previous),
            &current,
            &[OwnershipPolicy {
                subsystem: "Billing".to_string(),
                owns: vec!["Invoice".to_string(), "Payment".to_string()],
            }],
        );

        assert!(events
            .iter()
            .any(|event| event.drift_type == DriftType::DuplicateCapability));
        assert!(events
            .iter()
            .any(|event| event.drift_type == DriftType::SubsystemOverlap));
        assert!(events
            .iter()
            .any(|event| event.drift_type == DriftType::ArchitectureDrift));
        assert!(events
            .iter()
            .any(|event| event.drift_type == DriftType::ModelFragmentation));
        assert!(events
            .iter()
            .any(|event| event.drift_type == DriftType::OwnershipViolation));
    }
}
