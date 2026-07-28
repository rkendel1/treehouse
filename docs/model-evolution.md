# Incremental Model Evolution Engine

The Incremental Model Evolution Engine turns the inferred `ApplicationModel` from a one-shot snapshot into a living, versioned, delta-applicable artifact.

## Overview

The engine sits directly on top of the Unified Evidence Graph and enables:

- **Versioned lineage** of every `ApplicationModel`
- **Semantic (not just structural) diffs** between model versions
- **Safe application of incremental deltas**
- **Conflict detection & resolution** when evidence streams disagree
- **Model-level migrations** across versions
- **Stable identity** for entities, relationships, workflows, and capabilities over time

This is the prerequisite for the Capability Planner, Autonomous Remediation Engine, and any long-lived model-backed runtime.

## Core Concepts

### ModelVersion

An immutable snapshot of an `ApplicationModel` with metadata:

```rust
pub struct ModelVersion {
    pub id: VersionId,
    pub model_id: ModelId,
    pub parent: Option<VersionId>,
    pub model: ApplicationModel,
    pub evidence_snapshot_id: EvidenceSnapshotId,
    pub created_at: u64,
    pub confidence: Confidence,
    pub provenance: Provenance,
}
```

### ModelDelta

A typed, semantic change set between model versions:

```rust
pub struct ModelDelta {
    pub id: DeltaId,
    pub from: VersionId,
    pub changes: Vec<ChangeKind>,
    pub evidence_refs: Vec<String>,
    pub confidence: Confidence,
    pub conflicts: Vec<Conflict>,
}
```

### ChangeKind

Semantic change types include:

- `EntityAdded` / `EntityRemoved` / `EntityUpdated` / `EntityRenamed`
- `RelationshipAdded` / `RelationshipRemoved`
- `WorkflowAdded` / `WorkflowRemoved` / `WorkflowChanged`
- `ApiSurfaceAdded` / `ApiSurfaceRemoved` / `ApiSurfaceChanged`
- `PermissionAdded` / `PermissionRemoved` / `PermissionChanged`
- `ExperienceAdded` / `ExperienceRemoved`
- `IntegrationAdded` / `IntegrationRemoved`
- `ApplicationInfoChanged`

### ModelLineage

An append-only history of versions and the deltas that produced them:

```rust
pub struct ModelLineage {
    pub model_id: ModelId,
    pub head: VersionId,
    pub entries: Vec<LineageEntry>,
    pub created_at: u64,
    pub updated_at: u64,
}
```

### EvolutionEngine

The main entry point for evolving models:

```rust
pub struct EvolutionEngine<S: ModelLineageStore> {
    store: S,
    config: EvolutionConfig,
    identity_matcher: IdentityMatcher,
}
```

## Evolution Pipeline

```
New Evidence Snapshot
        │
        ▼
Semantic Diff (current ModelVersion ↔ new inference signals)
        │
        ▼
Candidate ModelDelta
        │
        ▼
Conflict Detection & Resolution
  (confidence-weighted, provenance-aware, optional human policy)
        │
        ▼
Apply Delta → new ModelVersion
        │
        ▼
Append to ModelLineage + update "head"
```

## Stable Identity

Stable identity is maintained via a combination of:

- **Structural signatures**: Hash of field names and types
- **Name + namespace heuristics**: Case-insensitive matching with aliases
- **Evidence provenance chains**: Track where evidence came from
- **Explicit rename/alias records**: Manual mapping of old names to new names

## CLI Commands

```bash
# Initialize a new model lineage
treehouse model init <project-root>

# Evolve the model from the latest evidence
treehouse model evolve <project-root> [--evidence-snapshot <snapshot-id>]

# Show lineage
treehouse model lineage <project-root>

# Semantic diff between two versions
treehouse model diff <version-from> <version-to>

# Apply a previously generated delta (or a hand-authored one)
treehouse model apply-delta <project-root> <delta-file>

# Materialise the current head model
treehouse model current <project-root> --output model.json
```

## File Structure

Artifacts live under:

```
.treehouse/model/
├── lineage.json
├── versions/
│   ├── <version_id>.json
│   └── ...
└── deltas/
    └── <delta_id>.json
```

## Conflict Handling

Conflicts are first-class citizens:

- Two high-confidence evidences that imply incompatible entity shapes
- Rename vs delete+create ambiguity
- Subsystem ownership violations
- Capability duplication across subsystem boundaries

### Resolution Strategies

Resolution strategies are configurable:

1. **Confidence-weighted automatic merge**: Choose the change with highest confidence
2. **Highest provenance wins**: Prefer changes from more authoritative sources
3. **Mark as conflict and require explicit resolution**: For human-in-the-loop scenarios
4. **Policy hooks**: For future planner or automation decisions

```rust
pub enum ResolutionStrategy {
    ConfidenceWeighted,
    HighestProvenance,
    RequireExplicit,
    Policy { policy_name: String },
}
```

## Integration Points

- **Consumes** `EvidenceSnapshots` from the Unified Evidence Graph
- **Produces** new evidence nodes of kind `ModelVersionCreated` and `ModelDeltaApplied`
- `treehouse-model-inference` becomes the "proposal" stage; this engine is the "commit" stage
- `treehouse-drift` and System Diff now operate against versioned models instead of ephemeral inferences
- Prepares the clean hand-off point for the Capability Planner and Remediation Engine

## Usage Example

```rust
use treehouse_model_evolution::{
    EvolutionEngine, FileModelLineageStore, ModelLineageStore,
};

// Create a store pointing to the .treehouse/model directory
let store = FileModelLineageStore::new(".treehouse/model");
let engine = EvolutionEngine::new(store);

// Initialize with a root model
let (lineage, root_version) = engine.initialize(initial_model, &evidence_snapshot)?;

// Later, evolve with a new inferred model
let (new_version, delta) = engine.evolve(
    &lineage.model_id,
    new_inferred_model,
    &new_evidence_snapshot,
)?;

// Check for conflicts
if delta.has_conflicts() {
    println!("Warning: {} conflicts detected", delta.conflicts.len());
}
```

## Testing

The crate includes comprehensive tests:

- **Unit tests** for delta application, identity matching, and conflict detection
- **Property-based tests** ensuring lineage remains append-only and parent pointers form a valid chain
- **Integration tests** that replay sequences of evidence snapshots and assert deterministic evolution
- **Semantic diff golden tests** (rename detection, capability moves, etc.)
- **Round-trip tests**: `ModelVersion` → `Delta` → apply → identical `ModelVersion`

Run tests with:

```bash
cargo test -p treehouse-model-evolution
```
