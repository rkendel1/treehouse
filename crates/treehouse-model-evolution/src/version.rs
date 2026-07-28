//! Model version types and identifiers.

use std::fmt;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use treehouse_application_model::ApplicationModel;
use treehouse_evidence::{Confidence, Provenance};

/// Unique identifier for a model lineage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelId(pub String);

impl ModelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn generate() -> Self {
        Self(format!("model-{:016x}", random_id()))
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a specific version within a model lineage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VersionId(pub String);

impl VersionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn generate() -> Self {
        Self(format!("ver-{:016x}", random_id()))
    }
}

impl fmt::Display for VersionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for an evidence snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EvidenceSnapshotId(pub String);

impl EvidenceSnapshotId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn from_unix_timestamp(ts: u64) -> Self {
        Self(format!("evidence-{ts}"))
    }
}

impl fmt::Display for EvidenceSnapshotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An immutable snapshot of an ApplicationModel with metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelVersion {
    /// Unique identifier for this version.
    pub id: VersionId,
    /// Identifier for the model lineage this version belongs to.
    pub model_id: ModelId,
    /// Parent version (None for the root version).
    pub parent: Option<VersionId>,
    /// The actual application model.
    pub model: ApplicationModel,
    /// Reference to the evidence snapshot that produced this version.
    pub evidence_snapshot_id: EvidenceSnapshotId,
    /// Unix timestamp when this version was created.
    pub created_at: u64,
    /// Confidence score for this version.
    pub confidence: Confidence,
    /// Provenance information.
    pub provenance: Provenance,
}

impl ModelVersion {
    /// Creates a new ModelVersion.
    pub fn new(
        model_id: ModelId,
        parent: Option<VersionId>,
        model: ApplicationModel,
        evidence_snapshot_id: EvidenceSnapshotId,
        confidence: Confidence,
        provenance: Provenance,
    ) -> Self {
        Self {
            id: VersionId::generate(),
            model_id,
            parent,
            model,
            evidence_snapshot_id,
            created_at: now_unix(),
            confidence,
            provenance,
        }
    }

    /// Creates the root version for a new model lineage.
    pub fn root(
        model_id: ModelId,
        model: ApplicationModel,
        evidence_snapshot_id: EvidenceSnapshotId,
        confidence: Confidence,
        provenance: Provenance,
    ) -> Self {
        Self::new(
            model_id,
            None,
            model,
            evidence_snapshot_id,
            confidence,
            provenance,
        )
    }

    /// Returns true if this is the root version (has no parent).
    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn random_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
        .hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    COUNTER.fetch_add(1, Ordering::SeqCst).hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use treehouse_application_model::{ApplicationInfo, GenerationMetadata};
    use treehouse_evidence::SourceKind;

    use super::*;

    fn test_model() -> ApplicationModel {
        ApplicationModel {
            application: ApplicationInfo {
                name: "Test App".to_string(),
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

    #[test]
    fn creates_root_version() {
        let model_id = ModelId::generate();
        let version = ModelVersion::root(
            model_id.clone(),
            test_model(),
            EvidenceSnapshotId::from_unix_timestamp(123),
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "test", "test"),
        );

        assert!(version.is_root());
        assert_eq!(version.model_id, model_id);
        assert!(version.parent.is_none());
    }

    #[test]
    fn creates_child_version() {
        let model_id = ModelId::generate();
        let parent_id = VersionId::generate();
        let version = ModelVersion::new(
            model_id.clone(),
            Some(parent_id.clone()),
            test_model(),
            EvidenceSnapshotId::from_unix_timestamp(456),
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "test", "test"),
        );

        assert!(!version.is_root());
        assert_eq!(version.parent, Some(parent_id));
    }

    #[test]
    fn model_id_display() {
        let id = ModelId::new("my-model");
        assert_eq!(format!("{id}"), "my-model");
    }

    #[test]
    fn version_id_display() {
        let id = VersionId::new("ver-123");
        assert_eq!(format!("{id}"), "ver-123");
    }
}
