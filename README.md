# Treehouse

Treehouse is a Rust workspace for **structured data exploration**, **schema/graph inference**, **application model generation**, and **development intelligence**.

It includes:
- A desktop explorer app (`treehouse-app`)
- A mock API runtime and CLI (`treehouse-mock`, `treehouse` binary)
- Core libraries for parsing, graphing, querying, diffing, model inference, code generation, contracts, execution, and digital twin analysis
- A repository observer (`treehouse-observer`) that produces live **System Diff** reports while code evolves

---

## Repository Layout

```text
treehouse/
├── crates/
│   ├── treehouse-api-engine/
│   ├── treehouse-agent/
│   ├── treehouse-api/
│   ├── treehouse-app/
│   ├── treehouse-application-model/
│   ├── treehouse-contracts/
│   ├── treehouse-convex/
│   ├── treehouse-core/
│   ├── treehouse-diff/
│   ├── treehouse-drift/
│   ├── treehouse-execution/
│   ├── treehouse-experience/
│   ├── treehouse-graph/
│   ├── treehouse-identity/
│   ├── treehouse-migration/
│   ├── treehouse-mock/
│   ├── treehouse-model-inference/
│   ├── treehouse-observer/
│   ├── treehouse-parser/
│   ├── treehouse-postgres/
│   ├── treehouse-process/
│   ├── treehouse-query/
│   ├── treehouse-search/
│   ├── treehouse-stats/
│   ├── treehouse-subsystem-engine/
│   ├── treehouse-system-graph/
│   ├── treehouse-tree/
│   └── treehouse-twin/
├── examples/
├── docs/
└── tests/
```

---

## Workspace Crates (Current Functionality)

### UI + Runtime

- **treehouse-app**: egui/eframe desktop application for opening structured files, tree navigation, graph/intelligence views, search, JSONPath, diff, stats, bookmarks, recent files, and command palette.
- **treehouse-mock**: mock HTTP runtime generated from discovered entity shapes.

### Core Data Foundations

- **treehouse-core**: `Document` abstraction and node metadata utilities.
- **treehouse-parser**: structured file parsing for JSON, JSONL/NDJSON, YAML, TOML, XML, and CSV.
- **treehouse-tree**: tree projection/state utilities for rendering navigable structures.
- **treehouse-search**: structural value/path search.
- **treehouse-query**: JSONPath-like path value access and matching.
- **treehouse-stats**: document-level statistics (depth, object/array/value counts, nulls, key frequencies).
- **treehouse-diff**: structural diff (added/removed/changed/type-changed) between documents.

### Graph + Application Intelligence

- **treehouse-graph**: universal data graph construction, schema inference, entity relationships, and observation evidence.
- **treehouse-model-inference**: compiles graph signals into an `ApplicationModel` (entities, relationships, workflows, permissions, API, experiences, integrations).
- **treehouse-application-model**: shared IR types for application/system model artifacts.
- **treehouse-api**: API surface generation and model-first request templates.

### API Intelligence + Execution

- **treehouse-api-engine**: OpenAPI import, scenario generation, state machine discovery, and transaction flow discovery.
- **treehouse-execution**: execution environment/runner primitives and flow reporting.
- **treehouse-contracts**: subsystem contract definitions, executable contract tests, registry + compatibility tracking, declared-vs-observed drift detection, migration planning, and validation helpers.

### Platform Generation + Domain Support

- **treehouse-postgres**: compiles application model into SQL/schema/migration/seed/docs artifacts.
- **treehouse-convex**: compiles application model into Convex artifacts.
- **treehouse-process**: workflow/state-machine primitives from event/process signals.
- **treehouse-identity**: role/permission domain types.
- **treehouse-experience**: screen/route/form domain types.
- **treehouse-migration**: migration planning primitives.
- **treehouse-twin**: impact analysis + system twin simulation helpers.

### Development Intelligence

- **treehouse-observer**: repository observation engine used by `treehouse connect`.
  - Captures snapshots from git state, code symbols, migrations, API/workflow/entity signals, tests, runtime event/log markers, and DB signals.
  - Computes a persisted **System Diff** with:
    - new capabilities
    - relationship/API/workflow deltas
    - potential breakage heuristics
    - architecture drift and subsystem-scale alerts
- **treehouse-system-graph**: unified System Graph model with versioned subsystem snapshots and confidence tracking.
- **treehouse-subsystem-engine**: subsystem boundary detection from code, APIs, workflows, events, and DB/runtime signals.
- **treehouse-drift**: drift detection engine for duplicate capabilities, subsystem overlap, ownership violations, architectural drift, and model fragmentation.
- **treehouse-agent**: local real-time architecture change agent event model used by `treehouse watch`.

---

## CLI

The `treehouse` CLI is implemented in `crates/treehouse-mock/src/bin/treehouse.rs`.

### Analyze

```bash
treehouse analyze <structured files...>
```

Builds a universal graph + inferred application model and prints detected entities/relationships/confidence.

### Compile

```bash
treehouse compile --target <postgres|convex> [--output dir] <structured files...>
```

Compiles inferred model artifacts for Postgres or Convex.

### Mock Runtime

```bash
treehouse mock <model-file>
# or
 treehouse-mock <model-file>
```

Starts a local mock API server at `localhost:4000` from inferred entity/API shape.

### Development Intelligence / Architecture Watching

```bash
treehouse connect <repo-path> [--state file] [--report file] [--interval secs] [--iterations n]
```

Runs repository observation and emits System Diff output each iteration.

Defaults:
- state: `<repo>/.treehouse/development-state.json`
- report: `<repo>/.treehouse/system-diff.json`
- interval: `2s`
- iterations: `1`

Example:

```bash
treehouse connect ./my-app --iterations 5 --interval 3
```

### Real-Time Watch Agent

```bash
treehouse watch <repo-path> [--state file] [--report file] [--interval secs] [--iterations n]
```

Runs the same snapshot/diff loop as `connect`, plus architecture-change events containing drift findings and remediation recommendations.

Additional live artifacts written under `.treehouse/`:
- `subsystem-contracts.json` (generated subsystem contract map)
- `system-graph-timeline.json` (time-series architecture history)

---

## Desktop App

Run the GUI explorer:

```bash
cargo run -p treehouse-app
```

Implemented UX includes:
- Open structured files
- Tree/explorer navigation
- Graph + intelligence panel
- Search and JSONPath result panes
- Diff mode (open base file and compare)
- Stats panel
- Bookmarks and recent files
- Command palette actions

---

## Build, Test, Format

From repository root:

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
```

---

## Development Intelligence Model (Implemented)

Treehouse `connect` follows this loop:

```text
Code Change
   |
   v
Treehouse Observation Snapshot
   |
   +-- Git delta
   +-- Code symbol (AST-level) delta
   +-- Migration and DB signal delta
   +-- API / workflow / entity delta
   +-- Test and runtime-event evidence delta
   |
   v
System Diff Report
   |
   v
Updated persistent state for next comparison
```

This enables repository-level architecture feedback without waiting for deployment.

---

## Technology

- Rust (stable)
- egui / eframe
- serde / serde_json / serde_yaml / toml / quick-xml / csv
- memmap2
- tiny_http
- anyhow / thiserror
