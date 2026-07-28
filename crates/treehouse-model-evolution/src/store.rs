//! File-backed storage for model lineage and versions.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::delta::ModelDelta;
use crate::lineage::ModelLineage;
use crate::version::{ModelId, ModelVersion, VersionId};

/// Trait for storing and retrieving model lineage data.
pub trait ModelLineageStore {
    /// Saves a model version.
    fn save_version(&self, version: &ModelVersion) -> Result<()>;

    /// Loads a model version by ID.
    fn load_version(&self, version_id: &VersionId) -> Result<Option<ModelVersion>>;

    /// Saves a model delta.
    fn save_delta(&self, delta: &ModelDelta) -> Result<()>;

    /// Loads a model delta by ID.
    fn load_delta(&self, delta_id: &crate::delta::DeltaId) -> Result<Option<ModelDelta>>;

    /// Saves the lineage.
    fn save_lineage(&self, lineage: &ModelLineage) -> Result<()>;

    /// Loads the lineage for a model.
    fn load_lineage(&self, model_id: &ModelId) -> Result<Option<ModelLineage>>;

    /// Loads the head version (current) for a model.
    fn load_head(&self, model_id: &ModelId) -> Result<Option<ModelVersion>>;

    /// Lists all model IDs in the store.
    fn list_models(&self) -> Result<Vec<ModelId>>;
}

/// File-backed implementation of ModelLineageStore.
///
/// Directory structure:
/// ```text
/// .treehouse/model/
/// ├── lineage.json
/// ├── versions/
/// │   ├── <version_id>.json
/// │   └── ...
/// └── deltas/
///     └── <delta_id>.json
/// ```
#[derive(Debug, Clone)]
pub struct FileModelLineageStore {
    base_path: PathBuf,
}

impl FileModelLineageStore {
    /// Creates a new FileModelLineageStore at the given path.
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// Returns the path to the versions directory.
    fn versions_path(&self) -> PathBuf {
        self.base_path.join("versions")
    }

    /// Returns the path to the deltas directory.
    fn deltas_path(&self) -> PathBuf {
        self.base_path.join("deltas")
    }

    /// Returns the path to the lineage file.
    fn lineage_path(&self) -> PathBuf {
        self.base_path.join("lineage.json")
    }

    /// Returns the path to a specific version file.
    fn version_file_path(&self, version_id: &VersionId) -> PathBuf {
        self.versions_path().join(format!("{}.json", version_id.0))
    }

    /// Returns the path to a specific delta file.
    fn delta_file_path(&self, delta_id: &crate::delta::DeltaId) -> PathBuf {
        self.deltas_path().join(format!("{}.json", delta_id.0))
    }

    /// Ensures all necessary directories exist.
    fn ensure_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.base_path)
            .with_context(|| format!("failed creating {}", self.base_path.display()))?;
        fs::create_dir_all(self.versions_path())
            .with_context(|| format!("failed creating {}", self.versions_path().display()))?;
        fs::create_dir_all(self.deltas_path())
            .with_context(|| format!("failed creating {}", self.deltas_path().display()))?;
        Ok(())
    }
}

impl ModelLineageStore for FileModelLineageStore {
    fn save_version(&self, version: &ModelVersion) -> Result<()> {
        self.ensure_dirs()?;
        let path = self.version_file_path(&version.id);
        let json = serde_json::to_string_pretty(version)?;
        let mut file = fs::File::create(&path)
            .with_context(|| format!("failed creating {}", path.display()))?;
        file.write_all(json.as_bytes())
            .with_context(|| format!("failed writing {}", path.display()))?;
        Ok(())
    }

    fn load_version(&self, version_id: &VersionId) -> Result<Option<ModelVersion>> {
        let path = self.version_file_path(version_id);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed reading {}", path.display()))?;
        let version: ModelVersion = serde_json::from_str(&content)
            .with_context(|| format!("failed parsing {}", path.display()))?;
        Ok(Some(version))
    }

    fn save_delta(&self, delta: &ModelDelta) -> Result<()> {
        self.ensure_dirs()?;
        let path = self.delta_file_path(&delta.id);
        let json = serde_json::to_string_pretty(delta)?;
        let mut file = fs::File::create(&path)
            .with_context(|| format!("failed creating {}", path.display()))?;
        file.write_all(json.as_bytes())
            .with_context(|| format!("failed writing {}", path.display()))?;
        Ok(())
    }

    fn load_delta(&self, delta_id: &crate::delta::DeltaId) -> Result<Option<ModelDelta>> {
        let path = self.delta_file_path(delta_id);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed reading {}", path.display()))?;
        let delta: ModelDelta = serde_json::from_str(&content)
            .with_context(|| format!("failed parsing {}", path.display()))?;
        Ok(Some(delta))
    }

    fn save_lineage(&self, lineage: &ModelLineage) -> Result<()> {
        self.ensure_dirs()?;
        let path = self.lineage_path();
        let json = serde_json::to_string_pretty(lineage)?;
        let mut file = fs::File::create(&path)
            .with_context(|| format!("failed creating {}", path.display()))?;
        file.write_all(json.as_bytes())
            .with_context(|| format!("failed writing {}", path.display()))?;
        Ok(())
    }

    fn load_lineage(&self, _model_id: &ModelId) -> Result<Option<ModelLineage>> {
        let path = self.lineage_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed reading {}", path.display()))?;
        let lineage: ModelLineage = serde_json::from_str(&content)
            .with_context(|| format!("failed parsing {}", path.display()))?;
        Ok(Some(lineage))
    }

    fn load_head(&self, model_id: &ModelId) -> Result<Option<ModelVersion>> {
        let lineage = self.load_lineage(model_id)?;
        match lineage {
            Some(l) => self.load_version(&l.head),
            None => Ok(None),
        }
    }

    fn list_models(&self) -> Result<Vec<ModelId>> {
        let path = self.lineage_path();
        if !path.exists() {
            return Ok(vec![]);
        }
        let lineage = self.load_lineage(&ModelId::new(""))?;
        Ok(lineage.map(|l| vec![l.model_id]).unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use treehouse_application_model::{ApplicationInfo, ApplicationModel, GenerationMetadata};
    use treehouse_evidence::{Confidence, Provenance, SourceKind};

    use super::*;
    use crate::delta::ChangeKind;
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

    #[test]
    fn saves_and_loads_version() {
        let temp_dir = std::env::temp_dir().join("treehouse-model-store-test-version");
        let _ = fs::remove_dir_all(&temp_dir);

        let store = FileModelLineageStore::new(&temp_dir);

        let model_id = ModelId::generate();
        let version = ModelVersion::root(
            model_id,
            test_model(),
            EvidenceSnapshotId::from_unix_timestamp(123),
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "test", "test"),
        );

        store.save_version(&version).unwrap();
        let loaded = store.load_version(&version.id).unwrap();

        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, version.id);
    }

    #[test]
    fn saves_and_loads_delta() {
        let temp_dir = std::env::temp_dir().join("treehouse-model-store-test-delta");
        let _ = fs::remove_dir_all(&temp_dir);

        let store = FileModelLineageStore::new(&temp_dir);

        let delta = ModelDelta::new(
            VersionId::new("ver-1"),
            vec![ChangeKind::EntityRemoved {
                name: "Test".to_string(),
            }],
            vec!["ev-1".to_string()],
            Confidence::default(),
        );

        store.save_delta(&delta).unwrap();
        let loaded = store.load_delta(&delta.id).unwrap();

        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, delta.id);
    }

    #[test]
    fn saves_and_loads_lineage() {
        let temp_dir = std::env::temp_dir().join("treehouse-model-store-test-lineage");
        let _ = fs::remove_dir_all(&temp_dir);

        let store = FileModelLineageStore::new(&temp_dir);

        let model_id = ModelId::generate();
        let version = ModelVersion::root(
            model_id.clone(),
            test_model(),
            EvidenceSnapshotId::from_unix_timestamp(123),
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "test", "test"),
        );

        let lineage = ModelLineage::new(model_id.clone(), &version);

        store.save_lineage(&lineage).unwrap();
        let loaded = store.load_lineage(&model_id).unwrap();

        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().model_id, model_id);
    }

    #[test]
    fn loads_head_version() {
        let temp_dir = std::env::temp_dir().join("treehouse-model-store-test-head");
        let _ = fs::remove_dir_all(&temp_dir);

        let store = FileModelLineageStore::new(&temp_dir);

        let model_id = ModelId::generate();
        let version = ModelVersion::root(
            model_id.clone(),
            test_model(),
            EvidenceSnapshotId::from_unix_timestamp(123),
            Confidence::default(),
            Provenance::new(SourceKind::Entity, "test", "test"),
        );

        let lineage = ModelLineage::new(model_id.clone(), &version);

        store.save_version(&version).unwrap();
        store.save_lineage(&lineage).unwrap();

        let head = store.load_head(&model_id).unwrap();
        assert!(head.is_some());
        assert_eq!(head.unwrap().id, version.id);
    }

    #[test]
    fn returns_none_for_missing() {
        let temp_dir = std::env::temp_dir().join("treehouse-model-store-test-missing");
        let _ = fs::remove_dir_all(&temp_dir);

        let store = FileModelLineageStore::new(&temp_dir);

        let version = store.load_version(&VersionId::new("nonexistent")).unwrap();
        assert!(version.is_none());

        let lineage = store.load_lineage(&ModelId::new("nonexistent")).unwrap();
        assert!(lineage.is_none());
    }
}
