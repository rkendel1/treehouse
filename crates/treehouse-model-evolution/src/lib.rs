//! # Incremental Model Evolution Engine
//!
//! This crate provides versioned, delta-driven evolution of [`ApplicationModel`] instances.
//!
//! ## Key Concepts
//!
//! - **ModelVersion**: An immutable snapshot of an `ApplicationModel` with metadata
//! - **ModelDelta**: A typed, semantic change set between model versions
//! - **ModelLineage**: An append-only history of versions and the deltas that produced them
//! - **EvolutionEngine**: Applies evidence-driven deltas, resolves conflicts, and materializes new versions
//! - **SemanticDiff**: Higher-level diff that understands intent (e.g., entity renamed vs. entity deleted + created)
//!
//! ## Example
//!
//! ```ignore
//! use treehouse_model_evolution::{EvolutionEngine, ModelLineageStore, FileModelLineageStore};
//!
//! let store = FileModelLineageStore::new(".treehouse/model");
//! let engine = EvolutionEngine::new(store);
//!
//! // Evolve from evidence
//! let new_version = engine.evolve(&evidence_snapshot)?;
//! ```

pub mod conflict;
pub mod delta;
pub mod engine;
pub mod identity;
pub mod lineage;
pub mod semantic_diff;
pub mod store;
pub mod version;

pub use conflict::{Conflict, ConflictKind, Resolution, ResolutionStrategy};
pub use delta::{ChangeKind, EntityPatch, FieldPatch, ModelDelta};
pub use engine::EvolutionEngine;
pub use identity::{EntityIdentity, IdentityMatcher};
pub use lineage::ModelLineage;
pub use semantic_diff::SemanticDiff;
pub use store::{FileModelLineageStore, ModelLineageStore};
pub use version::{ModelId, ModelVersion, VersionId};
