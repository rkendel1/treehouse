//! Semantic diff for comparing ApplicationModels.

use std::collections::{HashMap, HashSet};

use treehouse_application_model::{
    ApiEndpoint, ApplicationModel, Entity, Experience, Integration, PermissionPolicy, Workflow,
};

use crate::delta::{ChangeKind, EntityPatch, FieldPatch, ModelDelta};
use crate::identity::{IdentityMatcher, MatchResult};
use crate::version::VersionId;

/// Configuration for semantic diffing.
#[derive(Debug, Clone)]
pub struct SemanticDiffConfig {
    /// Threshold for detecting renames vs delete+create.
    pub rename_threshold: f64,
    /// Whether to track field-level changes.
    pub track_field_changes: bool,
    /// Whether to detect entity moves across models.
    pub detect_moves: bool,
}

impl Default for SemanticDiffConfig {
    fn default() -> Self {
        Self {
            rename_threshold: 0.8,
            track_field_changes: true,
            detect_moves: true,
        }
    }
}

/// Semantic diff between two ApplicationModels.
#[derive(Debug)]
pub struct SemanticDiff {
    config: SemanticDiffConfig,
    identity_matcher: IdentityMatcher,
}

impl SemanticDiff {
    /// Creates a new SemanticDiff with default configuration.
    pub fn new() -> Self {
        Self {
            config: SemanticDiffConfig::default(),
            identity_matcher: IdentityMatcher::new(),
        }
    }

    /// Creates a new SemanticDiff with custom configuration.
    pub fn with_config(config: SemanticDiffConfig) -> Self {
        Self {
            config,
            identity_matcher: IdentityMatcher::new(),
        }
    }

    /// Sets the identity matcher.
    pub fn with_identity_matcher(mut self, matcher: IdentityMatcher) -> Self {
        self.identity_matcher = matcher;
        self
    }

    /// Computes a semantic diff between two models.
    pub fn diff(&self, from: &ApplicationModel, to: &ApplicationModel) -> Vec<ChangeKind> {
        let mut changes = Vec::new();

        // Application info changes
        if from.application.name != to.application.name {
            changes.push(ChangeKind::ApplicationInfoChanged {
                old_name: from.application.name.clone(),
                new_name: to.application.name.clone(),
            });
        }

        // Entity changes
        changes.extend(self.diff_entities(&from.entities, &to.entities));

        // Workflow changes
        changes.extend(self.diff_workflows(&from.workflows, &to.workflows));

        // API changes
        changes.extend(self.diff_api(&from.api, &to.api));

        // Permission changes
        changes.extend(self.diff_permissions(&from.permissions, &to.permissions));

        // Experience changes
        changes.extend(self.diff_experiences(&from.experiences, &to.experiences));

        // Integration changes
        changes.extend(self.diff_integrations(&from.integrations, &to.integrations));

        changes
    }

    /// Computes a ModelDelta from two models.
    pub fn compute_delta(
        &self,
        from_version: &VersionId,
        from: &ApplicationModel,
        to: &ApplicationModel,
        evidence_refs: Vec<String>,
    ) -> ModelDelta {
        let changes = self.diff(from, to);
        ModelDelta::new(
            from_version.clone(),
            changes,
            evidence_refs,
            treehouse_evidence::Confidence::default(),
        )
    }

    fn diff_entities(&self, from: &[Entity], to: &[Entity]) -> Vec<ChangeKind> {
        let mut changes = Vec::new();

        let from_by_name: HashMap<String, &Entity> =
            from.iter().map(|e| (e.name.to_lowercase(), e)).collect();
        let to_by_name: HashMap<String, &Entity> =
            to.iter().map(|e| (e.name.to_lowercase(), e)).collect();

        let from_names: HashSet<_> = from_by_name.keys().cloned().collect();
        let to_names: HashSet<_> = to_by_name.keys().cloned().collect();

        // Find removed entities
        let mut removed_entities: Vec<_> = from_names.difference(&to_names).collect();
        removed_entities.sort();

        // Find added entities
        let mut added_entities: Vec<_> = to_names.difference(&from_names).collect();
        added_entities.sort();

        // Try to detect renames
        let mut handled_removed = HashSet::new();
        let mut handled_added = HashSet::new();

        if self.config.detect_moves {
            for removed_name in &removed_entities {
                if handled_removed.contains(*removed_name) {
                    continue;
                }
                let removed = from_by_name.get(*removed_name).unwrap();

                for added_name in &added_entities {
                    if handled_added.contains(*added_name) {
                        continue;
                    }
                    let added = to_by_name.get(*added_name).unwrap();

                    let match_result = self.identity_matcher.might_be_same_entity(removed, added);

                    if let MatchResult::HighStructuralSimilarity { similarity }
                    | MatchResult::ModerateStructuralSimilarity { similarity } = match_result
                    {
                        if similarity >= self.config.rename_threshold {
                            changes.push(ChangeKind::EntityRenamed {
                                from: removed.name.clone(),
                                to: added.name.clone(),
                                reason: format!("structural similarity: {:.0}%", similarity * 100.0),
                            });
                            handled_removed.insert((*removed_name).clone());
                            handled_added.insert((*added_name).clone());
                            break;
                        }
                    }
                }
            }
        }

        // Add remaining removals
        for removed_name in removed_entities {
            if !handled_removed.contains(removed_name) {
                let entity = from_by_name.get(removed_name).unwrap();
                changes.push(ChangeKind::EntityRemoved {
                    name: entity.name.clone(),
                });
            }
        }

        // Add remaining additions
        for added_name in added_entities {
            if !handled_added.contains(added_name) {
                let entity = to_by_name.get(added_name).unwrap();
                changes.push(ChangeKind::EntityAdded {
                    entity: (*entity).clone(),
                });
            }
        }

        // Check for updates to existing entities
        for name in from_names.intersection(&to_names) {
            let from_entity = from_by_name.get(name).unwrap();
            let to_entity = to_by_name.get(name).unwrap();

            if let Some(patch) = self.diff_entity(from_entity, to_entity) {
                changes.push(ChangeKind::EntityUpdated {
                    name: to_entity.name.clone(),
                    changes: patch,
                });
            }
        }

        changes
    }

    fn diff_entity(&self, from: &Entity, to: &Entity) -> Option<EntityPatch> {
        let mut patch = EntityPatch::default();

        if !self.config.track_field_changes {
            if from != to {
                return Some(patch);
            }
            return None;
        }

        // Field changes
        let from_fields: HashMap<String, _> =
            from.fields.iter().map(|f| (f.name.to_lowercase(), f)).collect();
        let to_fields: HashMap<String, _> =
            to.fields.iter().map(|f| (f.name.to_lowercase(), f)).collect();

        let from_field_names: HashSet<_> = from_fields.keys().cloned().collect();
        let to_field_names: HashSet<_> = to_fields.keys().cloned().collect();

        // Removed fields
        for name in from_field_names.difference(&to_field_names) {
            let field = from_fields.get(name).unwrap();
            patch.fields_removed.push(field.name.clone());
        }

        // Added fields
        for name in to_field_names.difference(&from_field_names) {
            let field = to_fields.get(name).unwrap();
            patch.fields_added.push((*field).clone());
        }

        // Changed fields
        for name in from_field_names.intersection(&to_field_names) {
            let from_field = from_fields.get(name).unwrap();
            let to_field = to_fields.get(name).unwrap();

            if from_field != to_field {
                patch.fields_changed.push(FieldPatch {
                    field_name: to_field.name.clone(),
                    old_value: Some((*from_field).clone()),
                    new_value: Some((*to_field).clone()),
                });
            }
        }

        // Relationship changes
        let from_rels: HashMap<String, _> =
            from.relationships.iter().map(|r| (r.name.to_lowercase(), r)).collect();
        let to_rels: HashMap<String, _> =
            to.relationships.iter().map(|r| (r.name.to_lowercase(), r)).collect();

        let from_rel_names: HashSet<_> = from_rels.keys().cloned().collect();
        let to_rel_names: HashSet<_> = to_rels.keys().cloned().collect();

        for name in from_rel_names.difference(&to_rel_names) {
            let rel = from_rels.get(name).unwrap();
            patch.relationships_removed.push(rel.name.clone());
        }

        for name in to_rel_names.difference(&from_rel_names) {
            let rel = to_rels.get(name).unwrap();
            patch.relationships_added.push((*rel).clone());
        }

        // Confidence changes
        if (from.confidence - to.confidence).abs() > 0.001 {
            patch.confidence_changed = Some((from.confidence, to.confidence));
        }

        if patch.is_empty() {
            None
        } else {
            Some(patch)
        }
    }

    fn diff_workflows(&self, from: &[Workflow], to: &[Workflow]) -> Vec<ChangeKind> {
        let mut changes = Vec::new();

        let from_by_entity: HashMap<_, _> = from.iter().map(|w| (w.entity.to_lowercase(), w)).collect();
        let to_by_entity: HashMap<_, _> = to.iter().map(|w| (w.entity.to_lowercase(), w)).collect();

        let from_entities: HashSet<_> = from_by_entity.keys().cloned().collect();
        let to_entities: HashSet<_> = to_by_entity.keys().cloned().collect();

        // Removed workflows
        for entity in from_entities.difference(&to_entities) {
            let workflow = from_by_entity.get(entity).unwrap();
            changes.push(ChangeKind::WorkflowRemoved {
                entity: workflow.entity.clone(),
            });
        }

        // Added workflows
        for entity in to_entities.difference(&from_entities) {
            let workflow = to_by_entity.get(entity).unwrap();
            changes.push(ChangeKind::WorkflowAdded {
                workflow: (*workflow).clone(),
            });
        }

        // Changed workflows
        for entity in from_entities.intersection(&to_entities) {
            let from_wf = from_by_entity.get(entity).unwrap();
            let to_wf = to_by_entity.get(entity).unwrap();

            if from_wf.states != to_wf.states {
                changes.push(ChangeKind::WorkflowChanged {
                    entity: to_wf.entity.clone(),
                    old_states: from_wf.states.clone(),
                    new_states: to_wf.states.clone(),
                });
            }
        }

        changes
    }

    fn diff_api(&self, from: &[ApiEndpoint], to: &[ApiEndpoint]) -> Vec<ChangeKind> {
        let mut changes = Vec::new();

        let key = |ep: &ApiEndpoint| format!("{}:{}", ep.method.to_lowercase(), ep.path.to_lowercase());

        let from_by_key: HashMap<_, _> = from.iter().map(|ep| (key(ep), ep)).collect();
        let to_by_key: HashMap<_, _> = to.iter().map(|ep| (key(ep), ep)).collect();

        let from_keys: HashSet<_> = from_by_key.keys().cloned().collect();
        let to_keys: HashSet<_> = to_by_key.keys().cloned().collect();

        // Removed endpoints
        for k in from_keys.difference(&to_keys) {
            let ep = from_by_key.get(k).unwrap();
            changes.push(ChangeKind::ApiSurfaceRemoved {
                method: ep.method.clone(),
                path: ep.path.clone(),
            });
        }

        // Added endpoints
        for k in to_keys.difference(&from_keys) {
            let ep = to_by_key.get(k).unwrap();
            changes.push(ChangeKind::ApiSurfaceAdded {
                endpoint: (*ep).clone(),
            });
        }

        // Changed endpoints
        for k in from_keys.intersection(&to_keys) {
            let from_ep = from_by_key.get(k).unwrap();
            let to_ep = to_by_key.get(k).unwrap();

            if from_ep != to_ep {
                changes.push(ChangeKind::ApiSurfaceChanged {
                    method: to_ep.method.clone(),
                    path: to_ep.path.clone(),
                    old: (*from_ep).clone(),
                    new: (*to_ep).clone(),
                });
            }
        }

        changes
    }

    fn diff_permissions(&self, from: &[PermissionPolicy], to: &[PermissionPolicy]) -> Vec<ChangeKind> {
        let mut changes = Vec::new();

        let from_by_entity: HashMap<_, _> =
            from.iter().map(|p| (p.entity.to_lowercase(), p)).collect();
        let to_by_entity: HashMap<_, _> = to.iter().map(|p| (p.entity.to_lowercase(), p)).collect();

        let from_entities: HashSet<_> = from_by_entity.keys().cloned().collect();
        let to_entities: HashSet<_> = to_by_entity.keys().cloned().collect();

        // Removed permissions
        for entity in from_entities.difference(&to_entities) {
            let perm = from_by_entity.get(entity).unwrap();
            changes.push(ChangeKind::PermissionRemoved {
                entity: perm.entity.clone(),
            });
        }

        // Added permissions
        for entity in to_entities.difference(&from_entities) {
            let perm = to_by_entity.get(entity).unwrap();
            changes.push(ChangeKind::PermissionAdded {
                permission: (*perm).clone(),
            });
        }

        // Changed permissions
        for entity in from_entities.intersection(&to_entities) {
            let from_perm = from_by_entity.get(entity).unwrap();
            let to_perm = to_by_entity.get(entity).unwrap();

            if from_perm != to_perm {
                changes.push(ChangeKind::PermissionChanged {
                    entity: to_perm.entity.clone(),
                    old: (*from_perm).clone(),
                    new: (*to_perm).clone(),
                });
            }
        }

        changes
    }

    fn diff_experiences(&self, from: &[Experience], to: &[Experience]) -> Vec<ChangeKind> {
        let mut changes = Vec::new();

        let from_by_name: HashMap<_, _> = from.iter().map(|e| (e.name.to_lowercase(), e)).collect();
        let to_by_name: HashMap<_, _> = to.iter().map(|e| (e.name.to_lowercase(), e)).collect();

        let from_names: HashSet<_> = from_by_name.keys().cloned().collect();
        let to_names: HashSet<_> = to_by_name.keys().cloned().collect();

        for name in from_names.difference(&to_names) {
            let exp = from_by_name.get(name).unwrap();
            changes.push(ChangeKind::ExperienceRemoved {
                name: exp.name.clone(),
            });
        }

        for name in to_names.difference(&from_names) {
            let exp = to_by_name.get(name).unwrap();
            changes.push(ChangeKind::ExperienceAdded {
                experience: (*exp).clone(),
            });
        }

        changes
    }

    fn diff_integrations(&self, from: &[Integration], to: &[Integration]) -> Vec<ChangeKind> {
        let mut changes = Vec::new();

        let from_by_name: HashMap<_, _> = from.iter().map(|i| (i.name.to_lowercase(), i)).collect();
        let to_by_name: HashMap<_, _> = to.iter().map(|i| (i.name.to_lowercase(), i)).collect();

        let from_names: HashSet<_> = from_by_name.keys().cloned().collect();
        let to_names: HashSet<_> = to_by_name.keys().cloned().collect();

        for name in from_names.difference(&to_names) {
            let integration = from_by_name.get(name).unwrap();
            changes.push(ChangeKind::IntegrationRemoved {
                name: integration.name.clone(),
            });
        }

        for name in to_names.difference(&from_names) {
            let integration = to_by_name.get(name).unwrap();
            changes.push(ChangeKind::IntegrationAdded {
                integration: (*integration).clone(),
            });
        }

        changes
    }
}

impl Default for SemanticDiff {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use treehouse_application_model::{
        ApiEndpoint, ApplicationInfo, CrudOperation, Entity, Field, GenerationMetadata,
        PermissionPolicy, Workflow,
    };

    use super::*;

    fn test_model() -> ApplicationModel {
        ApplicationModel {
            application: ApplicationInfo {
                name: "Test".to_string(),
                version: "1.0".to_string(),
            },
            entities: vec![],
            workflows: vec![],
            permissions: vec![],
            api: vec![],
            experiences: vec![],
            integrations: vec![],
            metadata: GenerationMetadata {
                generated_by: "test".to_string(),
                generated_at_unix: 0,
                source_count: 0,
            },
        }
    }

    fn make_entity(name: &str, fields: &[(&str, &str)]) -> Entity {
        Entity {
            name: name.to_string(),
            confidence: 0.9,
            fields: fields
                .iter()
                .map(|(n, t)| Field {
                    name: n.to_string(),
                    field_type: t.to_string(),
                    required: true,
                    primary: false,
                    unique: false,
                    confidence: 0.9,
                })
                .collect(),
            relationships: vec![],
            constraints: vec![],
        }
    }

    #[test]
    fn detects_entity_added() {
        let diff = SemanticDiff::new();
        let from = test_model();
        let mut to = test_model();
        to.entities.push(make_entity("User", &[("id", "uuid")]));

        let changes = diff.diff(&from, &to);

        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], ChangeKind::EntityAdded { entity } if entity.name == "User"));
    }

    #[test]
    fn detects_entity_removed() {
        let diff = SemanticDiff::new();
        let mut from = test_model();
        from.entities.push(make_entity("User", &[("id", "uuid")]));
        let to = test_model();

        let changes = diff.diff(&from, &to);

        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], ChangeKind::EntityRemoved { name } if name == "User"));
    }

    #[test]
    fn detects_entity_renamed() {
        let diff = SemanticDiff::with_config(SemanticDiffConfig {
            rename_threshold: 0.7,
            ..Default::default()
        });

        let mut from = test_model();
        from.entities.push(make_entity(
            "Customer",
            &[("id", "uuid"), ("email", "string"), ("name", "string")],
        ));

        let mut to = test_model();
        to.entities.push(make_entity(
            "Client",
            &[("id", "uuid"), ("email", "string"), ("name", "string")],
        ));

        let changes = diff.diff(&from, &to);

        assert_eq!(changes.len(), 1);
        assert!(matches!(
            &changes[0],
            ChangeKind::EntityRenamed { from, to, .. } if from == "Customer" && to == "Client"
        ));
    }

    #[test]
    fn detects_field_added() {
        let diff = SemanticDiff::new();

        let mut from = test_model();
        from.entities.push(make_entity("User", &[("id", "uuid")]));

        let mut to = test_model();
        to.entities.push(make_entity("User", &[("id", "uuid"), ("email", "string")]));

        let changes = diff.diff(&from, &to);

        assert_eq!(changes.len(), 1);
        match &changes[0] {
            ChangeKind::EntityUpdated { name, changes } => {
                assert_eq!(name, "User");
                assert_eq!(changes.fields_added.len(), 1);
                assert_eq!(changes.fields_added[0].name, "email");
            }
            _ => panic!("Expected EntityUpdated"),
        }
    }

    #[test]
    fn detects_workflow_changes() {
        let diff = SemanticDiff::new();

        let mut from = test_model();
        from.workflows.push(Workflow {
            entity: "Order".to_string(),
            states: vec!["pending".to_string(), "completed".to_string()],
            transitions: vec![],
        });

        let mut to = test_model();
        to.workflows.push(Workflow {
            entity: "Order".to_string(),
            states: vec![
                "pending".to_string(),
                "processing".to_string(),
                "completed".to_string(),
            ],
            transitions: vec![],
        });

        let changes = diff.diff(&from, &to);

        assert_eq!(changes.len(), 1);
        assert!(matches!(
            &changes[0],
            ChangeKind::WorkflowChanged { entity, .. } if entity == "Order"
        ));
    }

    #[test]
    fn detects_api_changes() {
        let diff = SemanticDiff::new();

        let mut from = test_model();
        from.api.push(ApiEndpoint {
            method: "GET".to_string(),
            path: "/users".to_string(),
            operation: CrudOperation::List,
            entity: "User".to_string(),
        });

        let mut to = test_model();
        to.api.push(ApiEndpoint {
            method: "GET".to_string(),
            path: "/customers".to_string(),
            operation: CrudOperation::List,
            entity: "Customer".to_string(),
        });

        let changes = diff.diff(&from, &to);

        assert_eq!(changes.len(), 2);
        assert!(changes
            .iter()
            .any(|c| matches!(c, ChangeKind::ApiSurfaceRemoved { path, .. } if path == "/users")));
        assert!(changes.iter().any(
            |c| matches!(c, ChangeKind::ApiSurfaceAdded { endpoint } if endpoint.path == "/customers")
        ));
    }

    #[test]
    fn detects_permission_changes() {
        let diff = SemanticDiff::new();

        let mut from = test_model();
        from.permissions.push(PermissionPolicy {
            entity: "User".to_string(),
            list: true,
            get: true,
            create: false,
            update: false,
        });

        let mut to = test_model();
        to.permissions.push(PermissionPolicy {
            entity: "User".to_string(),
            list: true,
            get: true,
            create: true,
            update: true,
        });

        let changes = diff.diff(&from, &to);

        assert_eq!(changes.len(), 1);
        assert!(matches!(
            &changes[0],
            ChangeKind::PermissionChanged { entity, .. } if entity == "User"
        ));
    }

    #[test]
    fn computes_delta() {
        let diff = SemanticDiff::new();
        let from_version = VersionId::new("ver-1");

        let from = test_model();
        let mut to = test_model();
        to.entities.push(make_entity("Order", &[("id", "uuid")]));

        let delta = diff.compute_delta(&from_version, &from, &to, vec!["ev-1".to_string()]);

        assert_eq!(delta.from, from_version);
        assert_eq!(delta.len(), 1);
        assert_eq!(delta.evidence_refs, vec!["ev-1".to_string()]);
    }
}
