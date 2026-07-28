//! Model lineage tracking for append-only version history.

use serde::{Deserialize, Serialize};

use crate::delta::{DeltaId, ModelDelta};
use crate::version::{ModelId, ModelVersion, VersionId};

/// A record in the lineage representing a version and its producing delta.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LineageEntry {
    /// The version ID.
    pub version_id: VersionId,
    /// The delta that produced this version (None for root).
    pub delta_id: Option<DeltaId>,
    /// Unix timestamp when this entry was added.
    pub created_at: u64,
}

/// Append-only history of model versions and the deltas that produced them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelLineage {
    /// The model this lineage tracks.
    pub model_id: ModelId,
    /// The current head version.
    pub head: VersionId,
    /// The ordered list of entries (oldest first).
    pub entries: Vec<LineageEntry>,
    /// Unix timestamp when this lineage was created.
    pub created_at: u64,
    /// Unix timestamp when this lineage was last updated.
    pub updated_at: u64,
}

impl ModelLineage {
    /// Creates a new lineage with a root version.
    pub fn new(model_id: ModelId, root_version: &ModelVersion) -> Self {
        let now = now_unix();
        Self {
            model_id,
            head: root_version.id.clone(),
            entries: vec![LineageEntry {
                version_id: root_version.id.clone(),
                delta_id: None,
                created_at: now,
            }],
            created_at: now,
            updated_at: now,
        }
    }

    /// Appends a new version produced by a delta.
    pub fn append(&mut self, version: &ModelVersion, delta: &ModelDelta) {
        let now = now_unix();
        self.entries.push(LineageEntry {
            version_id: version.id.clone(),
            delta_id: Some(delta.id.clone()),
            created_at: now,
        });
        self.head = version.id.clone();
        self.updated_at = now;
    }

    /// Returns the number of versions in this lineage.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if this lineage is empty (should never happen).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns all version IDs in chronological order.
    pub fn version_ids(&self) -> Vec<VersionId> {
        self.entries.iter().map(|e| e.version_id.clone()).collect()
    }

    /// Returns the root version ID.
    pub fn root(&self) -> Option<&VersionId> {
        self.entries.first().map(|e| &e.version_id)
    }

    /// Returns the parent version ID for a given version.
    pub fn parent_of(&self, version_id: &VersionId) -> Option<&VersionId> {
        let idx = self.entries.iter().position(|e| &e.version_id == version_id)?;
        if idx == 0 {
            None
        } else {
            Some(&self.entries[idx - 1].version_id)
        }
    }

    /// Returns all version IDs between two versions (exclusive of start, inclusive of end).
    pub fn versions_between(&self, from: &VersionId, to: &VersionId) -> Vec<VersionId> {
        let from_idx = self.entries.iter().position(|e| &e.version_id == from);
        let to_idx = self.entries.iter().position(|e| &e.version_id == to);

        match (from_idx, to_idx) {
            (Some(from_i), Some(to_i)) if from_i < to_i => self.entries[from_i + 1..=to_i]
                .iter()
                .map(|e| e.version_id.clone())
                .collect(),
            _ => vec![],
        }
    }

    /// Validates that the lineage forms a valid chain.
    pub fn validate(&self) -> Result<(), LineageError> {
        if self.entries.is_empty() {
            return Err(LineageError::EmptyLineage);
        }

        // First entry should have no delta
        if self.entries[0].delta_id.is_some() {
            return Err(LineageError::InvalidRoot);
        }

        // All subsequent entries should have deltas
        for entry in self.entries.iter().skip(1) {
            if entry.delta_id.is_none() {
                return Err(LineageError::MissingDelta {
                    version_id: entry.version_id.clone(),
                });
            }
        }

        // Head should match last entry
        if let Some(last) = self.entries.last() {
            if last.version_id != self.head {
                return Err(LineageError::HeadMismatch {
                    expected: last.version_id.clone(),
                    actual: self.head.clone(),
                });
            }
        }

        Ok(())
    }
}

/// Errors that can occur during lineage operations.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LineageError {
    #[error("lineage is empty")]
    EmptyLineage,

    #[error("root version has an associated delta")]
    InvalidRoot,

    #[error("version {version_id} is missing a delta")]
    MissingDelta { version_id: VersionId },

    #[error("head mismatch: expected {expected}, got {actual}")]
    HeadMismatch { expected: VersionId, actual: VersionId },
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use treehouse_application_model::{ApplicationInfo, ApplicationModel, GenerationMetadata};
    use treehouse_evidence::{Confidence, Provenance, SourceKind};

    use super::*;
    use crate::version::EvidenceSnapshotId;

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

    fn test_version(model_id: &ModelId) -> ModelVersion {
        ModelVersion::root(
            model_id.clone(),
            test_model(),
            EvidenceSnapshotId::from_unix_timestamp(0),
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "test", "test"),
        )
    }

    #[test]
    fn creates_lineage() {
        let model_id = ModelId::generate();
        let version = test_version(&model_id);
        let lineage = ModelLineage::new(model_id.clone(), &version);

        assert_eq!(lineage.model_id, model_id);
        assert_eq!(lineage.head, version.id);
        assert_eq!(lineage.len(), 1);
        assert!(lineage.validate().is_ok());
    }

    #[test]
    fn appends_version() {
        let model_id = ModelId::generate();
        let root = test_version(&model_id);
        let mut lineage = ModelLineage::new(model_id.clone(), &root);

        let v2 = ModelVersion::new(
            model_id.clone(),
            Some(root.id.clone()),
            test_model(),
            EvidenceSnapshotId::from_unix_timestamp(100),
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "test", "test"),
        );
        let delta = ModelDelta::new(root.id.clone(), vec![], vec![], Confidence::default());

        lineage.append(&v2, &delta);

        assert_eq!(lineage.len(), 2);
        assert_eq!(lineage.head, v2.id);
        assert!(lineage.validate().is_ok());
    }

    #[test]
    fn finds_parent() {
        let model_id = ModelId::generate();
        let root = test_version(&model_id);
        let mut lineage = ModelLineage::new(model_id.clone(), &root);

        let v2 = ModelVersion::new(
            model_id.clone(),
            Some(root.id.clone()),
            test_model(),
            EvidenceSnapshotId::from_unix_timestamp(100),
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "test", "test"),
        );
        let delta = ModelDelta::new(root.id.clone(), vec![], vec![], Confidence::default());
        lineage.append(&v2, &delta);

        assert_eq!(lineage.parent_of(&root.id), None);
        assert_eq!(lineage.parent_of(&v2.id), Some(&root.id));
    }

    #[test]
    fn versions_between() {
        let model_id = ModelId::generate();
        let v1 = test_version(&model_id);
        let mut lineage = ModelLineage::new(model_id.clone(), &v1);

        let v2 = ModelVersion::new(
            model_id.clone(),
            Some(v1.id.clone()),
            test_model(),
            EvidenceSnapshotId::from_unix_timestamp(100),
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "test", "test"),
        );
        let delta1 = ModelDelta::new(v1.id.clone(), vec![], vec![], Confidence::default());
        lineage.append(&v2, &delta1);

        let v3 = ModelVersion::new(
            model_id.clone(),
            Some(v2.id.clone()),
            test_model(),
            EvidenceSnapshotId::from_unix_timestamp(200),
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "test", "test"),
        );
        let delta2 = ModelDelta::new(v2.id.clone(), vec![], vec![], Confidence::default());
        lineage.append(&v3, &delta2);

        let between = lineage.versions_between(&v1.id, &v3.id);
        assert_eq!(between.len(), 2);
        assert_eq!(between[0], v2.id);
        assert_eq!(between[1], v3.id);
    }

    #[test]
    fn validates_lineage() {
        let model_id = ModelId::generate();
        let v = test_version(&model_id);
        let lineage = ModelLineage::new(model_id, &v);
        assert!(lineage.validate().is_ok());
    }

    #[test]
    fn detects_empty_lineage() {
        let lineage = ModelLineage {
            model_id: ModelId::generate(),
            head: VersionId::new("test"),
            entries: vec![],
            created_at: 0,
            updated_at: 0,
        };
        assert_eq!(lineage.validate(), Err(LineageError::EmptyLineage));
    }
}
