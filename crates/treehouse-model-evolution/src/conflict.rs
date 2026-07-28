//! Conflict detection and resolution for model evolution.

use serde::{Deserialize, Serialize};
use treehouse_evidence::Confidence;

use crate::delta::ChangeKind;

/// The kind of conflict detected during model evolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictKind {
    /// Two high-confidence evidences imply incompatible entity shapes.
    IncompatibleEntityShape {
        entity: String,
        description: String,
    },
    /// Ambiguity between rename and delete+create.
    RenameAmbiguity {
        possible_rename_from: String,
        possible_rename_to: String,
    },
    /// Entity ownership violation across subsystem boundaries.
    OwnershipViolation {
        entity: String,
        claimed_by: Vec<String>,
    },
    /// Capability duplication across subsystem boundaries.
    CapabilityDuplication {
        capability: String,
        subsystems: Vec<String>,
    },
    /// Contradicting evidence about an entity.
    ContradictingEvidence {
        entity: String,
        evidence_ids: Vec<String>,
    },
    /// Field type mismatch between evidence sources.
    FieldTypeMismatch {
        entity: String,
        field: String,
        types: Vec<String>,
    },
    /// Relationship direction mismatch.
    RelationshipMismatch {
        from_entity: String,
        to_entity: String,
        description: String,
    },
}

impl ConflictKind {
    /// Returns a human-readable description of the conflict kind.
    pub fn description(&self) -> String {
        match self {
            ConflictKind::IncompatibleEntityShape { entity, description } => {
                format!("Incompatible shape for entity '{}': {}", entity, description)
            }
            ConflictKind::RenameAmbiguity {
                possible_rename_from,
                possible_rename_to,
            } => {
                format!(
                    "Ambiguous: '{}' might be renamed to '{}' or deleted and recreated",
                    possible_rename_from, possible_rename_to
                )
            }
            ConflictKind::OwnershipViolation { entity, claimed_by } => {
                format!(
                    "Entity '{}' is claimed by multiple subsystems: {}",
                    entity,
                    claimed_by.join(", ")
                )
            }
            ConflictKind::CapabilityDuplication { capability, subsystems } => {
                format!(
                    "Capability '{}' is duplicated across subsystems: {}",
                    capability,
                    subsystems.join(", ")
                )
            }
            ConflictKind::ContradictingEvidence { entity, evidence_ids } => {
                format!(
                    "Contradicting evidence for entity '{}' from: {}",
                    entity,
                    evidence_ids.join(", ")
                )
            }
            ConflictKind::FieldTypeMismatch { entity, field, types } => {
                format!(
                    "Field '{}' in entity '{}' has conflicting types: {}",
                    field, entity,
                    types.join(", ")
                )
            }
            ConflictKind::RelationshipMismatch {
                from_entity,
                to_entity,
                description,
            } => {
                format!(
                    "Relationship between '{}' and '{}' has conflicts: {}",
                    from_entity, to_entity, description
                )
            }
        }
    }
}

/// How a conflict was resolved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Resolution {
    /// Conflict was automatically resolved using confidence weighting.
    AutomaticConfidenceWeighted { chosen_change: ChangeKind },
    /// Conflict was resolved by choosing the highest provenance.
    HighestProvenance { chosen_source: String },
    /// Conflict was marked as unresolved, requiring manual intervention.
    Unresolved,
    /// Conflict was resolved manually by a user.
    ManualResolution {
        chosen_change: ChangeKind,
        resolved_by: String,
    },
    /// Conflict was resolved by a policy hook.
    PolicyResolution {
        policy_name: String,
        chosen_change: ChangeKind,
    },
}

/// A conflict detected during model evolution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Conflict {
    /// The kind of conflict.
    pub kind: ConflictKind,
    /// The conflicting changes.
    pub conflicting_changes: Vec<ChangeKind>,
    /// Evidence IDs involved in this conflict.
    pub evidence_ids: Vec<String>,
    /// Confidence scores for each conflicting change.
    pub confidences: Vec<Confidence>,
    /// How the conflict was resolved (if at all).
    pub resolution: Option<Resolution>,
}

impl Conflict {
    /// Creates a new unresolved conflict.
    pub fn new(
        kind: ConflictKind,
        conflicting_changes: Vec<ChangeKind>,
        evidence_ids: Vec<String>,
        confidences: Vec<Confidence>,
    ) -> Self {
        Self {
            kind,
            conflicting_changes,
            evidence_ids,
            confidences,
            resolution: None,
        }
    }

    /// Returns true if this conflict has been resolved.
    pub fn is_resolved(&self) -> bool {
        matches!(
            &self.resolution,
            Some(Resolution::AutomaticConfidenceWeighted { .. })
                | Some(Resolution::HighestProvenance { .. })
                | Some(Resolution::ManualResolution { .. })
                | Some(Resolution::PolicyResolution { .. })
        )
    }

    /// Resolves the conflict using confidence weighting.
    pub fn resolve_by_confidence(&mut self) -> Option<&ChangeKind> {
        if self.conflicting_changes.is_empty() || self.confidences.is_empty() {
            return None;
        }

        let max_idx = self
            .confidences
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.score.partial_cmp(&b.score).unwrap())
            .map(|(idx, _)| idx)?;

        if max_idx < self.conflicting_changes.len() {
            let chosen = self.conflicting_changes[max_idx].clone();
            self.resolution = Some(Resolution::AutomaticConfidenceWeighted {
                chosen_change: chosen,
            });
            self.conflicting_changes.get(max_idx)
        } else {
            None
        }
    }

    /// Marks the conflict as unresolved.
    pub fn mark_unresolved(&mut self) {
        self.resolution = Some(Resolution::Unresolved);
    }
}

/// Strategy for resolving conflicts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ResolutionStrategy {
    /// Automatically merge using confidence weighting.
    #[default]
    ConfidenceWeighted,
    /// Choose the change with the highest provenance.
    HighestProvenance,
    /// Mark conflicts as unresolved and require explicit resolution.
    RequireExplicit,
    /// Use a policy hook for resolution decisions.
    Policy { policy_name: String },
}

impl ResolutionStrategy {
    /// Attempts to resolve a conflict using this strategy.
    pub fn resolve(&self, conflict: &mut Conflict) -> bool {
        match self {
            ResolutionStrategy::ConfidenceWeighted => conflict.resolve_by_confidence().is_some(),
            ResolutionStrategy::HighestProvenance => {
                // For now, fall back to confidence weighting
                // In a full implementation, this would look at provenance
                conflict.resolve_by_confidence().is_some()
            }
            ResolutionStrategy::RequireExplicit => {
                conflict.mark_unresolved();
                false
            }
            ResolutionStrategy::Policy { .. } => {
                // Policy hooks would be implemented here
                conflict.mark_unresolved();
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use treehouse_application_model::Entity;

    use super::*;

    #[test]
    fn creates_unresolved_conflict() {
        let conflict = Conflict::new(
            ConflictKind::IncompatibleEntityShape {
                entity: "User".to_string(),
                description: "Different field sets".to_string(),
            },
            vec![],
            vec!["ev-1".to_string(), "ev-2".to_string()],
            vec![],
        );

        assert!(!conflict.is_resolved());
    }

    #[test]
    fn resolves_by_confidence() {
        let change1 = ChangeKind::EntityAdded {
            entity: Entity {
                name: "User".to_string(),
                confidence: 0.9,
                fields: vec![],
                relationships: vec![],
                constraints: vec![],
            },
        };
        let change2 = ChangeKind::EntityAdded {
            entity: Entity {
                name: "Account".to_string(),
                confidence: 0.95,
                fields: vec![],
                relationships: vec![],
                constraints: vec![],
            },
        };

        let mut conflict = Conflict::new(
            ConflictKind::RenameAmbiguity {
                possible_rename_from: "User".to_string(),
                possible_rename_to: "Account".to_string(),
            },
            vec![change1, change2],
            vec!["ev-1".to_string(), "ev-2".to_string()],
            vec![Confidence::new(0.8, None), Confidence::new(0.95, None)],
        );

        let result = conflict.resolve_by_confidence();
        assert!(result.is_some());
        assert!(conflict.is_resolved());
    }

    #[test]
    fn conflict_kind_descriptions() {
        let kind = ConflictKind::OwnershipViolation {
            entity: "Order".to_string(),
            claimed_by: vec!["Billing".to_string(), "Sales".to_string()],
        };
        assert!(kind.description().contains("Order"));
        assert!(kind.description().contains("Billing"));
    }

    #[test]
    fn resolution_strategy_require_explicit() {
        let mut conflict = Conflict::new(
            ConflictKind::CapabilityDuplication {
                capability: "payment".to_string(),
                subsystems: vec!["A".to_string(), "B".to_string()],
            },
            vec![],
            vec![],
            vec![],
        );

        let strategy = ResolutionStrategy::RequireExplicit;
        let resolved = strategy.resolve(&mut conflict);

        assert!(!resolved);
        assert!(matches!(conflict.resolution, Some(Resolution::Unresolved)));
    }
}
