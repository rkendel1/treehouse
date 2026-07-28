//! Model delta types representing semantic changes between versions.

use serde::{Deserialize, Serialize};
use treehouse_application_model::{
    ApiEndpoint, Entity, Experience, Field, Integration, PermissionPolicy, Relationship, Workflow,
};
use treehouse_evidence::Confidence;

use crate::conflict::Conflict;
use crate::version::VersionId;

/// Unique identifier for a delta.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeltaId(pub String);

impl DeltaId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn generate() -> Self {
        use std::hash::{Hash, Hasher};
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .hash(&mut hasher);
        std::process::id().hash(&mut hasher);
        COUNTER.fetch_add(1, Ordering::SeqCst).hash(&mut hasher);
        Self(format!("delta-{:016x}", hasher.finish()))
    }
}

impl std::fmt::Display for DeltaId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A patch representing changes to a single field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldPatch {
    pub field_name: String,
    pub old_value: Option<Field>,
    pub new_value: Option<Field>,
}

/// A patch representing changes to an entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityPatch {
    pub fields_added: Vec<Field>,
    pub fields_removed: Vec<String>,
    pub fields_changed: Vec<FieldPatch>,
    pub relationships_added: Vec<Relationship>,
    pub relationships_removed: Vec<String>,
    pub confidence_changed: Option<(f32, f32)>,
}

impl Default for EntityPatch {
    fn default() -> Self {
        Self {
            fields_added: vec![],
            fields_removed: vec![],
            fields_changed: vec![],
            relationships_added: vec![],
            relationships_removed: vec![],
            confidence_changed: None,
        }
    }
}

impl EntityPatch {
    pub fn is_empty(&self) -> bool {
        self.fields_added.is_empty()
            && self.fields_removed.is_empty()
            && self.fields_changed.is_empty()
            && self.relationships_added.is_empty()
            && self.relationships_removed.is_empty()
            && self.confidence_changed.is_none()
    }
}

/// The kind of change in a delta.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChangeKind {
    /// A new entity was added.
    EntityAdded { entity: Entity },
    /// An entity was removed.
    EntityRemoved { name: String },
    /// An entity was updated with specific changes.
    EntityUpdated { name: String, changes: EntityPatch },
    /// An entity was renamed (semantic understanding that it's the same entity).
    EntityRenamed {
        from: String,
        to: String,
        reason: String,
    },

    /// A new relationship was added.
    RelationshipAdded {
        entity: String,
        relationship: Relationship,
    },
    /// A relationship was removed.
    RelationshipRemoved { entity: String, relationship_name: String },

    /// A workflow was added.
    WorkflowAdded { workflow: Workflow },
    /// A workflow was removed.
    WorkflowRemoved { entity: String },
    /// A workflow was changed.
    WorkflowChanged {
        entity: String,
        old_states: Vec<String>,
        new_states: Vec<String>,
    },

    /// An API endpoint was added.
    ApiSurfaceAdded { endpoint: ApiEndpoint },
    /// An API endpoint was removed.
    ApiSurfaceRemoved { method: String, path: String },
    /// An API endpoint was changed.
    ApiSurfaceChanged {
        method: String,
        path: String,
        old: ApiEndpoint,
        new: ApiEndpoint,
    },

    /// Permission policy was added.
    PermissionAdded { permission: PermissionPolicy },
    /// Permission policy was removed.
    PermissionRemoved { entity: String },
    /// Permission policy was changed.
    PermissionChanged {
        entity: String,
        old: PermissionPolicy,
        new: PermissionPolicy,
    },

    /// An experience was added.
    ExperienceAdded { experience: Experience },
    /// An experience was removed.
    ExperienceRemoved { name: String },

    /// An integration was added.
    IntegrationAdded { integration: Integration },
    /// An integration was removed.
    IntegrationRemoved { name: String },

    /// Application metadata changed.
    ApplicationInfoChanged { old_name: String, new_name: String },
}

impl ChangeKind {
    /// Returns a human-readable description of the change.
    pub fn description(&self) -> String {
        match self {
            ChangeKind::EntityAdded { entity } => format!("Added entity '{}'", entity.name),
            ChangeKind::EntityRemoved { name } => format!("Removed entity '{name}'"),
            ChangeKind::EntityUpdated { name, changes } => {
                let mut parts = vec![];
                if !changes.fields_added.is_empty() {
                    parts.push(format!("{} fields added", changes.fields_added.len()));
                }
                if !changes.fields_removed.is_empty() {
                    parts.push(format!("{} fields removed", changes.fields_removed.len()));
                }
                if !changes.fields_changed.is_empty() {
                    parts.push(format!("{} fields changed", changes.fields_changed.len()));
                }
                if parts.is_empty() {
                    format!("Updated entity '{name}'")
                } else {
                    format!("Updated entity '{}': {}", name, parts.join(", "))
                }
            }
            ChangeKind::EntityRenamed { from, to, reason } => {
                format!("Renamed entity '{from}' to '{to}' ({reason})")
            }
            ChangeKind::RelationshipAdded { entity, relationship } => {
                format!(
                    "Added relationship '{}' to entity '{}'",
                    relationship.name, entity
                )
            }
            ChangeKind::RelationshipRemoved {
                entity,
                relationship_name,
            } => {
                format!(
                    "Removed relationship '{relationship_name}' from entity '{entity}'"
                )
            }
            ChangeKind::WorkflowAdded { workflow } => {
                format!("Added workflow for entity '{}'", workflow.entity)
            }
            ChangeKind::WorkflowRemoved { entity } => {
                format!("Removed workflow for entity '{entity}'")
            }
            ChangeKind::WorkflowChanged { entity, .. } => {
                format!("Changed workflow for entity '{entity}'")
            }
            ChangeKind::ApiSurfaceAdded { endpoint } => {
                format!("Added API endpoint {} {}", endpoint.method, endpoint.path)
            }
            ChangeKind::ApiSurfaceRemoved { method, path } => {
                format!("Removed API endpoint {method} {path}")
            }
            ChangeKind::ApiSurfaceChanged { method, path, .. } => {
                format!("Changed API endpoint {method} {path}")
            }
            ChangeKind::PermissionAdded { permission } => {
                format!("Added permission policy for entity '{}'", permission.entity)
            }
            ChangeKind::PermissionRemoved { entity } => {
                format!("Removed permission policy for entity '{entity}'")
            }
            ChangeKind::PermissionChanged { entity, .. } => {
                format!("Changed permission policy for entity '{entity}'")
            }
            ChangeKind::ExperienceAdded { experience } => {
                format!("Added experience '{}'", experience.name)
            }
            ChangeKind::ExperienceRemoved { name } => format!("Removed experience '{name}'"),
            ChangeKind::IntegrationAdded { integration } => {
                format!("Added integration '{}'", integration.name)
            }
            ChangeKind::IntegrationRemoved { name } => format!("Removed integration '{name}'"),
            ChangeKind::ApplicationInfoChanged { old_name, new_name } => {
                format!("Changed application name from '{old_name}' to '{new_name}'")
            }
        }
    }
}

/// A typed, semantic change set between model versions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelDelta {
    /// Unique identifier for this delta.
    pub id: DeltaId,
    /// The version this delta is applied from.
    pub from: VersionId,
    /// The list of changes in this delta.
    pub changes: Vec<ChangeKind>,
    /// References to evidence nodes that support these changes.
    pub evidence_refs: Vec<String>,
    /// Overall confidence of this delta.
    pub confidence: Confidence,
    /// Any conflicts detected during delta creation.
    pub conflicts: Vec<Conflict>,
}

impl ModelDelta {
    /// Creates a new ModelDelta.
    pub fn new(
        from: VersionId,
        changes: Vec<ChangeKind>,
        evidence_refs: Vec<String>,
        confidence: Confidence,
    ) -> Self {
        Self {
            id: DeltaId::generate(),
            from,
            changes,
            evidence_refs,
            confidence,
            conflicts: vec![],
        }
    }

    /// Creates an empty delta.
    pub fn empty(from: VersionId) -> Self {
        Self::new(from, vec![], vec![], Confidence::default())
    }

    /// Returns true if this delta contains no changes.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Returns the number of changes in this delta.
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// Returns true if there are unresolved conflicts.
    pub fn has_conflicts(&self) -> bool {
        self.conflicts.iter().any(|c| !c.is_resolved())
    }

    /// Adds a conflict to this delta.
    pub fn add_conflict(&mut self, conflict: Conflict) {
        self.conflicts.push(conflict);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_delta() {
        let from = VersionId::new("ver-1");
        let delta = ModelDelta::new(
            from.clone(),
            vec![ChangeKind::EntityAdded {
                entity: Entity {
                    name: "User".to_string(),
                    confidence: 0.9,
                    fields: vec![],
                    relationships: vec![],
                    constraints: vec![],
                },
            }],
            vec!["ev-123".to_string()],
            Confidence::new(0.9, None),
        );

        assert_eq!(delta.from, from);
        assert_eq!(delta.len(), 1);
        assert!(!delta.is_empty());
    }

    #[test]
    fn empty_delta() {
        let delta = ModelDelta::empty(VersionId::new("ver-1"));
        assert!(delta.is_empty());
        assert_eq!(delta.len(), 0);
    }

    #[test]
    fn change_descriptions() {
        let entity = Entity {
            name: "Order".to_string(),
            confidence: 0.95,
            fields: vec![],
            relationships: vec![],
            constraints: vec![],
        };

        assert!(
            ChangeKind::EntityAdded {
                entity: entity.clone()
            }
            .description()
            .contains("Added entity 'Order'")
        );

        assert!(ChangeKind::EntityRemoved {
            name: "Order".to_string()
        }
        .description()
        .contains("Removed entity 'Order'"));

        assert!(ChangeKind::EntityRenamed {
            from: "Customer".to_string(),
            to: "Client".to_string(),
            reason: "naming convention".to_string(),
        }
        .description()
        .contains("Renamed entity 'Customer' to 'Client'"));
    }

    #[test]
    fn entity_patch_empty() {
        let patch = EntityPatch::default();
        assert!(patch.is_empty());
    }

    #[test]
    fn entity_patch_with_changes() {
        let patch = EntityPatch {
            fields_added: vec![Field {
                name: "email".to_string(),
                field_type: "string".to_string(),
                required: true,
                primary: false,
                unique: true,
                confidence: 0.9,
            }],
            ..Default::default()
        };
        assert!(!patch.is_empty());
    }
}
