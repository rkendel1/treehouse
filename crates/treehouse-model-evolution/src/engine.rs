//! Evolution engine for applying deltas and managing model versions.

use anyhow::{bail, Context, Result};
use treehouse_application_model::ApplicationModel;
use treehouse_evidence::{Confidence, EvidenceSnapshot, Provenance, SourceKind};

use crate::conflict::{Conflict, ResolutionStrategy};
use crate::delta::{ChangeKind, ModelDelta};
use crate::identity::IdentityMatcher;
use crate::lineage::ModelLineage;
use crate::semantic_diff::SemanticDiff;
use crate::store::ModelLineageStore;
use crate::version::{EvidenceSnapshotId, ModelId, ModelVersion, VersionId};

/// Configuration for the evolution engine.
#[derive(Debug, Clone)]
pub struct EvolutionConfig {
    /// Strategy for resolving conflicts.
    pub resolution_strategy: ResolutionStrategy,
    /// Whether to allow evolving with unresolved conflicts.
    pub allow_unresolved_conflicts: bool,
    /// Minimum confidence threshold for changes.
    pub min_confidence_threshold: f32,
}

impl Default for EvolutionConfig {
    fn default() -> Self {
        Self {
            resolution_strategy: ResolutionStrategy::ConfidenceWeighted,
            allow_unresolved_conflicts: false,
            min_confidence_threshold: 0.5,
        }
    }
}

/// The evolution engine applies evidence-driven deltas, resolves conflicts,
/// and materializes new model versions.
pub struct EvolutionEngine<S: ModelLineageStore> {
    store: S,
    config: EvolutionConfig,
    identity_matcher: IdentityMatcher,
}

impl<S: ModelLineageStore> EvolutionEngine<S> {
    /// Creates a new EvolutionEngine with the given store.
    pub fn new(store: S) -> Self {
        Self {
            store,
            config: EvolutionConfig::default(),
            identity_matcher: IdentityMatcher::new(),
        }
    }

    /// Creates a new EvolutionEngine with custom configuration.
    pub fn with_config(store: S, config: EvolutionConfig) -> Self {
        Self {
            store,
            config,
            identity_matcher: IdentityMatcher::new(),
        }
    }

    /// Sets the identity matcher.
    pub fn with_identity_matcher(mut self, matcher: IdentityMatcher) -> Self {
        self.identity_matcher = matcher;
        self
    }

    /// Initializes a new model lineage with a root version.
    pub fn initialize(
        &self,
        model: ApplicationModel,
        evidence_snapshot: &EvidenceSnapshot,
    ) -> Result<(ModelLineage, ModelVersion)> {
        let model_id = ModelId::generate();
        let evidence_snapshot_id =
            EvidenceSnapshotId::from_unix_timestamp(evidence_snapshot.observed_through_unix);

        let root_version = ModelVersion::root(
            model_id.clone(),
            model,
            evidence_snapshot_id,
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "model-evolution", "EvolutionEngine"),
        );

        let lineage = ModelLineage::new(model_id, &root_version);

        self.store
            .save_version(&root_version)
            .context("failed to save root version")?;
        self.store
            .save_lineage(&lineage)
            .context("failed to save lineage")?;

        Ok((lineage, root_version))
    }

    /// Evolves the model from the current head using a new inferred model.
    pub fn evolve(
        &self,
        model_id: &ModelId,
        new_model: ApplicationModel,
        evidence_snapshot: &EvidenceSnapshot,
    ) -> Result<(ModelVersion, ModelDelta)> {
        // Load current state
        let lineage = self
            .store
            .load_lineage(model_id)?
            .context("lineage not found")?;
        let current_version = self
            .store
            .load_version(&lineage.head)?
            .context("head version not found")?;

        // Compute semantic diff
        let diff = SemanticDiff::new().with_identity_matcher(self.identity_matcher.clone());
        let evidence_refs: Vec<String> = evidence_snapshot
            .nodes
            .iter()
            .map(|n| n.id.clone())
            .collect();

        let mut delta =
            diff.compute_delta(&current_version.id, &current_version.model, &new_model, evidence_refs);

        // Detect and resolve conflicts
        let conflicts = self.detect_conflicts(&current_version.model, &new_model, &delta);
        for conflict in conflicts {
            delta.add_conflict(conflict);
        }

        self.resolve_conflicts(&mut delta)?;

        // Check for unresolved conflicts
        if delta.has_conflicts() && !self.config.allow_unresolved_conflicts {
            bail!("Cannot evolve: {} unresolved conflicts", delta.conflicts.len());
        }

        // Apply delta to create new version
        let new_version = self.apply_delta(&lineage, &current_version, &new_model, &delta, evidence_snapshot)?;

        Ok((new_version, delta))
    }

    /// Applies a delta to create a new version.
    pub fn apply_delta(
        &self,
        lineage: &ModelLineage,
        current_version: &ModelVersion,
        new_model: &ApplicationModel,
        delta: &ModelDelta,
        evidence_snapshot: &EvidenceSnapshot,
    ) -> Result<ModelVersion> {
        let evidence_snapshot_id =
            EvidenceSnapshotId::from_unix_timestamp(evidence_snapshot.observed_through_unix);

        let new_version = ModelVersion::new(
            lineage.model_id.clone(),
            Some(current_version.id.clone()),
            new_model.clone(),
            evidence_snapshot_id,
            delta.confidence.clone(),
            Provenance::new(SourceKind::Entity, "model-evolution", "EvolutionEngine"),
        );

        // Update lineage
        let mut updated_lineage = lineage.clone();
        updated_lineage.append(&new_version, delta);

        // Save everything
        self.store
            .save_version(&new_version)
            .context("failed to save new version")?;
        self.store
            .save_delta(delta)
            .context("failed to save delta")?;
        self.store
            .save_lineage(&updated_lineage)
            .context("failed to save lineage")?;

        Ok(new_version)
    }

    /// Applies a previously generated or hand-authored delta.
    pub fn apply_existing_delta(
        &self,
        model_id: &ModelId,
        delta: &ModelDelta,
    ) -> Result<ModelVersion> {
        let lineage = self
            .store
            .load_lineage(model_id)?
            .context("lineage not found")?;
        let current_version = self
            .store
            .load_version(&lineage.head)?
            .context("head version not found")?;

        // Verify delta is applicable
        if delta.from != current_version.id {
            bail!(
                "Delta is from version {} but current head is {}",
                delta.from,
                current_version.id
            );
        }

        // Apply changes to the model
        let new_model = self.apply_changes(&current_version.model, &delta.changes)?;

        // Create a synthetic evidence snapshot
        let evidence_snapshot = EvidenceSnapshot {
            observed_through_unix: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            nodes: vec![],
            edges: vec![],
        };

        self.apply_delta(&lineage, &current_version, &new_model, delta, &evidence_snapshot)
    }

    /// Applies a list of changes to a model.
    fn apply_changes(
        &self,
        model: &ApplicationModel,
        changes: &[ChangeKind],
    ) -> Result<ApplicationModel> {
        let mut result = model.clone();

        for change in changes {
            match change {
                ChangeKind::EntityAdded { entity } => {
                    if !result.entities.iter().any(|e| e.name == entity.name) {
                        result.entities.push(entity.clone());
                    }
                }
                ChangeKind::EntityRemoved { name } => {
                    result.entities.retain(|e| e.name != *name);
                }
                ChangeKind::EntityUpdated { name, changes: patch } => {
                    if let Some(entity) = result.entities.iter_mut().find(|e| e.name == *name) {
                        // Apply field changes
                        for field in &patch.fields_added {
                            if !entity.fields.iter().any(|f| f.name == field.name) {
                                entity.fields.push(field.clone());
                            }
                        }
                        entity.fields.retain(|f| !patch.fields_removed.contains(&f.name));
                        for field_patch in &patch.fields_changed {
                            if let Some(field) = entity.fields.iter_mut().find(|f| f.name == field_patch.field_name) {
                                if let Some(new_value) = &field_patch.new_value {
                                    *field = new_value.clone();
                                }
                            }
                        }

                        // Apply relationship changes
                        for rel in &patch.relationships_added {
                            if !entity.relationships.iter().any(|r| r.name == rel.name) {
                                entity.relationships.push(rel.clone());
                            }
                        }
                        entity.relationships.retain(|r| !patch.relationships_removed.contains(&r.name));

                        // Apply confidence change
                        if let Some((_, new_confidence)) = patch.confidence_changed {
                            entity.confidence = new_confidence;
                        }
                    }
                }
                ChangeKind::EntityRenamed { from, to, .. } => {
                    if let Some(entity) = result.entities.iter_mut().find(|e| e.name == *from) {
                        entity.name = to.clone();
                    }
                }
                ChangeKind::WorkflowAdded { workflow } => {
                    if !result.workflows.iter().any(|w| w.entity == workflow.entity) {
                        result.workflows.push(workflow.clone());
                    }
                }
                ChangeKind::WorkflowRemoved { entity } => {
                    result.workflows.retain(|w| w.entity != *entity);
                }
                ChangeKind::WorkflowChanged { entity, new_states, .. } => {
                    if let Some(workflow) = result.workflows.iter_mut().find(|w| w.entity == *entity) {
                        workflow.states = new_states.clone();
                    }
                }
                ChangeKind::ApiSurfaceAdded { endpoint } => {
                    if !result.api.iter().any(|e| e.method == endpoint.method && e.path == endpoint.path) {
                        result.api.push(endpoint.clone());
                    }
                }
                ChangeKind::ApiSurfaceRemoved { method, path } => {
                    result.api.retain(|e| !(e.method == *method && e.path == *path));
                }
                ChangeKind::ApiSurfaceChanged { method, path, new, .. } => {
                    if let Some(endpoint) = result.api.iter_mut().find(|e| e.method == *method && e.path == *path) {
                        *endpoint = new.clone();
                    }
                }
                ChangeKind::PermissionAdded { permission } => {
                    if !result.permissions.iter().any(|p| p.entity == permission.entity) {
                        result.permissions.push(permission.clone());
                    }
                }
                ChangeKind::PermissionRemoved { entity } => {
                    result.permissions.retain(|p| p.entity != *entity);
                }
                ChangeKind::PermissionChanged { entity, new, .. } => {
                    if let Some(perm) = result.permissions.iter_mut().find(|p| p.entity == *entity) {
                        *perm = new.clone();
                    }
                }
                ChangeKind::ExperienceAdded { experience } => {
                    if !result.experiences.iter().any(|e| e.name == experience.name) {
                        result.experiences.push(experience.clone());
                    }
                }
                ChangeKind::ExperienceRemoved { name } => {
                    result.experiences.retain(|e| e.name != *name);
                }
                ChangeKind::IntegrationAdded { integration } => {
                    if !result.integrations.iter().any(|i| i.name == integration.name) {
                        result.integrations.push(integration.clone());
                    }
                }
                ChangeKind::IntegrationRemoved { name } => {
                    result.integrations.retain(|i| i.name != *name);
                }
                ChangeKind::ApplicationInfoChanged { new_name, .. } => {
                    result.application.name = new_name.clone();
                }
                ChangeKind::RelationshipAdded { entity, relationship } => {
                    if let Some(e) = result.entities.iter_mut().find(|e| e.name == *entity) {
                        if !e.relationships.iter().any(|r| r.name == relationship.name) {
                            e.relationships.push(relationship.clone());
                        }
                    }
                }
                ChangeKind::RelationshipRemoved { entity, relationship_name } => {
                    if let Some(e) = result.entities.iter_mut().find(|e| e.name == *entity) {
                        e.relationships.retain(|r| r.name != *relationship_name);
                    }
                }
            }
        }

        Ok(result)
    }

    /// Detects conflicts between the current model and the new model.
    fn detect_conflicts(
        &self,
        _current: &ApplicationModel,
        _new: &ApplicationModel,
        _delta: &ModelDelta,
    ) -> Vec<Conflict> {
        // For now, return empty. In a full implementation, this would:
        // - Check for contradicting evidence
        // - Detect incompatible entity shapes
        // - Find ownership violations
        // - Identify capability duplications
        vec![]
    }

    /// Resolves conflicts using the configured strategy.
    fn resolve_conflicts(&self, delta: &mut ModelDelta) -> Result<()> {
        for conflict in &mut delta.conflicts {
            self.config.resolution_strategy.resolve(conflict);
        }
        Ok(())
    }

    /// Returns the current head version for a model.
    pub fn current(&self, model_id: &ModelId) -> Result<Option<ModelVersion>> {
        self.store.load_head(model_id)
    }

    /// Returns the lineage for a model.
    pub fn lineage(&self, model_id: &ModelId) -> Result<Option<ModelLineage>> {
        self.store.load_lineage(model_id)
    }

    /// Computes a semantic diff between two versions.
    pub fn diff_versions(
        &self,
        from_version_id: &VersionId,
        to_version_id: &VersionId,
    ) -> Result<Vec<ChangeKind>> {
        let from = self
            .store
            .load_version(from_version_id)?
            .context("from version not found")?;
        let to = self
            .store
            .load_version(to_version_id)?
            .context("to version not found")?;

        let diff = SemanticDiff::new().with_identity_matcher(self.identity_matcher.clone());
        Ok(diff.diff(&from.model, &to.model))
    }

    /// Loads a specific version.
    pub fn get_version(&self, version_id: &VersionId) -> Result<Option<ModelVersion>> {
        self.store.load_version(version_id)
    }
}

#[cfg(test)]
mod tests {
    use treehouse_application_model::{ApplicationInfo, Entity, Field, GenerationMetadata};

    use super::*;
    use crate::store::FileModelLineageStore;

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

    fn test_snapshot() -> EvidenceSnapshot {
        EvidenceSnapshot {
            observed_through_unix: 100,
            nodes: vec![],
            edges: vec![],
        }
    }

    #[test]
    fn initializes_new_model() {
        let temp_dir = std::env::temp_dir().join("treehouse-evolution-init");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let store = FileModelLineageStore::new(&temp_dir);
        let engine = EvolutionEngine::new(store);

        let (lineage, version) = engine.initialize(test_model(), &test_snapshot()).unwrap();

        assert_eq!(lineage.len(), 1);
        assert!(version.is_root());
    }

    #[test]
    fn evolves_model() {
        let temp_dir = std::env::temp_dir().join("treehouse-evolution-evolve");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let store = FileModelLineageStore::new(&temp_dir);
        let engine = EvolutionEngine::new(store);

        let (lineage, _) = engine.initialize(test_model(), &test_snapshot()).unwrap();

        // Create a new model with an entity
        let mut new_model = test_model();
        new_model.entities.push(Entity {
            name: "User".to_string(),
            confidence: 0.9,
            fields: vec![Field {
                name: "id".to_string(),
                field_type: "uuid".to_string(),
                required: true,
                primary: true,
                unique: true,
                confidence: 0.9,
            }],
            relationships: vec![],
            constraints: vec![],
        });

        let new_snapshot = EvidenceSnapshot {
            observed_through_unix: 200,
            nodes: vec![],
            edges: vec![],
        };

        let (new_version, delta) = engine
            .evolve(&lineage.model_id, new_model, &new_snapshot)
            .unwrap();

        assert!(!new_version.is_root());
        assert_eq!(delta.len(), 1);
        assert!(matches!(
            &delta.changes[0],
            ChangeKind::EntityAdded { entity } if entity.name == "User"
        ));
    }

    #[test]
    fn applies_changes_correctly() {
        let temp_dir = std::env::temp_dir().join("treehouse-evolution-apply");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let store = FileModelLineageStore::new(&temp_dir);
        let engine = EvolutionEngine::new(store);

        let mut model = test_model();
        model.entities.push(Entity {
            name: "Order".to_string(),
            confidence: 0.9,
            fields: vec![],
            relationships: vec![],
            constraints: vec![],
        });

        let changes = vec![
            ChangeKind::EntityAdded {
                entity: Entity {
                    name: "User".to_string(),
                    confidence: 0.9,
                    fields: vec![],
                    relationships: vec![],
                    constraints: vec![],
                },
            },
            ChangeKind::EntityRemoved {
                name: "Order".to_string(),
            },
        ];

        let result = engine.apply_changes(&model, &changes).unwrap();

        assert!(result.entities.iter().any(|e| e.name == "User"));
        assert!(!result.entities.iter().any(|e| e.name == "Order"));
    }

    #[test]
    fn diffs_versions() {
        let temp_dir = std::env::temp_dir().join("treehouse-evolution-diff");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let store = FileModelLineageStore::new(&temp_dir);
        let engine = EvolutionEngine::new(store);

        let (lineage, v1) = engine.initialize(test_model(), &test_snapshot()).unwrap();

        let mut new_model = test_model();
        new_model.entities.push(Entity {
            name: "Product".to_string(),
            confidence: 0.9,
            fields: vec![],
            relationships: vec![],
            constraints: vec![],
        });

        let new_snapshot = EvidenceSnapshot {
            observed_through_unix: 200,
            nodes: vec![],
            edges: vec![],
        };

        let (v2, _) = engine
            .evolve(&lineage.model_id, new_model, &new_snapshot)
            .unwrap();

        let changes = engine.diff_versions(&v1.id, &v2.id).unwrap();
        assert_eq!(changes.len(), 1);
    }

    #[test]
    fn retrieves_current_head() {
        let temp_dir = std::env::temp_dir().join("treehouse-evolution-current");
        let _ = std::fs::remove_dir_all(&temp_dir);

        let store = FileModelLineageStore::new(&temp_dir);
        let engine = EvolutionEngine::new(store);

        let (lineage, version) = engine.initialize(test_model(), &test_snapshot()).unwrap();

        let current = engine.current(&lineage.model_id).unwrap();
        assert!(current.is_some());
        assert_eq!(current.unwrap().id, version.id);
    }
}
