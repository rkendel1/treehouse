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
│   ├── treehouse-evidence/
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
- **treehouse-evidence**: typed append-only evidence graph store with snapshot/query/conflict helpers.
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
treehouse connect <repo-path> [--state file] [--report file] [--interval secs] [--iterations n] [--continuous] [--hot-reload]
```

Runs repository observation and emits System Diff output each iteration.

Defaults:
- state: `<repo>/.treehouse/development-state.json`
- report: `<repo>/.treehouse/system-diff.json`
- interval: `2s`
- iterations: `1`

Use `--continuous` to keep running until you stop it (Ctrl+C).
Use `--hot-reload` to auto-regenerate twin and projection artifacts when drift or relevant changes are detected.

Example:

```bash
treehouse connect ./my-app --iterations 5 --interval 3
treehouse connect . --interval 2 --continuous
```

### Real-Time Watch Agent

```bash
treehouse watch <repo-path> [--state file] [--report file] [--interval secs] [--iterations n] [--continuous] [--hot-reload]
```

Runs the same snapshot/diff loop as `connect`, plus architecture-change events containing drift findings and remediation recommendations.

Additional live artifacts written under `.treehouse/`:
- `subsystem-contracts.json` (generated subsystem contract map)
- `system-graph-timeline.json` (time-series architecture history)
- `evidence/` (append-only unified evidence graph)
- `knowledge/graph.json` (typed software knowledge graph)
- `knowledge/nodes.json` (node projection)
- `knowledge/edges.json` (edge projection)
- `knowledge/timeline.json` (graph evolution timeline)
- `knowledge/drift/report.json` (drift projection)
- `runtime/runtime.json` (continuous architecture runtime projection)
- `runtime/health.json` (subsystem health scores)
- `runtime/alarms.json` (architectural alarms)
- `runtime/timeline.json` (runtime history per cycle)
- `docs/architecture-runtime.md` (auto-generated architecture runtime documentation)

Desktop repo continuous loop:

```bash
cargo run -p treehouse-mock --bin treehouse -- watch . --interval 2 --continuous
```

Desktop app + observer in separate terminals:

```bash
# terminal 1
cargo run -p treehouse-app

# terminal 2
cargo run -p treehouse-mock --bin treehouse -- watch . --interval 2 --continuous
```

Monitor multiple desktop repos at once:

```bash
scripts/monitor-repos.sh --interval 3 ~/Desktop/repo-a ~/Desktop/repo-b
```

For each watched repo, monitor output is written inside that repo at `.treehouse/monitors/<repo-name>.log`.

Optional one-shot mode (run N snapshots per repo and exit):

```bash
scripts/monitor-repos.sh --interval 2 --iterations 1 ~/Desktop/repo-a ~/Desktop/repo-b
```

Cold start watched locations (initialize state/artifacts before continuous monitoring):

```bash
scripts/cold-start-repos.sh ~/Desktop/repo-a ~/Desktop/repo-b
```

Cold start + auto-discover Desktop repos + handoff into continuous monitoring:

```bash
scripts/cold-start-repos.sh --desktop-all --desktop-max 5 --start-monitor --monitor-interval 3
```

Optional baseline scan during cold start:

```bash
scripts/cold-start-repos.sh --baseline-scan ~/Desktop/repo-a
```

Cold-start outputs per watched repo:
- `.treehouse/development-state.json`
- `.treehouse/system-diff.json`
- `.treehouse/system-graph-timeline.json`
- `.treehouse/subsystem-contracts.json`
- `.treehouse/monitors/<repo-name>-cold-start.log`

Run project projections and API gateway from analyzed repo state:

```bash
# generate postgres + convex projections from cold-start/baseline artifacts
scripts/launch-projection.sh ~/Desktop/repo-a --mode all

# only postgres projection
scripts/launch-projection.sh ~/Desktop/repo-a --mode postgres

# run API gateway projection (mock runtime) from inferred model
scripts/launch-projection.sh ~/Desktop/repo-a --mode gateway
```

Direct CLI projection command from an artifact model:

```bash
treehouse project <application-model.json> --target <postgres|convex|gateway|all> [--output dir]
```

Query the software digital twin:

```bash
treehouse graph <repo-path> [--contains text] [--type node-type]
treehouse why <repo-path> <term>
treehouse drift <repo-path>
treehouse runtime <repo-path> [--health|--alarms|--timeline|--docs]
treehouse twin build <repo-path> [--output file]
treehouse twin inspect <bundle-file>
treehouse twin compare <bundle-a> <bundle-b>
treehouse twin run <bundle-file> --capability <name>
treehouse twin simulate <bundle-file> --workflow <name> [--events event_a,event_b]
treehouse twin what-if <bundle-file> --workflow <name> [--events event_a,event_b] [--remove-state state|--remove-transition from:to]
```

See `docs/software-knowledge-graph-v2.md` for the V2 model and roadmap.
See `docs/continuous-architecture-runtime-v1.md` for CAR v1 details.
See `docs/understanding-compiler-roadmap.md` for the twin compiler direction.

Start CAR in one command:

```bash
scripts/run-car.sh ~/Desktop/repo-a 2
```

### GitHub Repo Automation

This repository includes GitHub Actions workflows under `.github/workflows/` that:

- run formatting/check/test gates on each push and pull request
- run a Treehouse watch snapshot and upload `.treehouse` artifacts for each run

Once you push to GitHub, open the **Actions** tab to see continuous run output and download the generated report artifacts.

### Target-Driven Scan

```bash
treehouse scan ./my-service \
  --target ./targets/event-driven-microservice.md \
  --output .treehouse/scan/

# named target from ./targets
treehouse scan ./my-service --target event-driven --local-llm heuristic
```

Flags:
- `--target <path|name>`
- `--local-llm [heuristic|ollama:<model>]`
- `--output <dir>`
- `--baseline-only`
- `--goals-only`
- `--format json|markdown`

Generated artifacts:

```text
.treehouse/scan/<target>/
├── baseline/
│   ├── evidence-snapshot.json
│   ├── application-model.json
│   └── system-graph.json
├── target/
│   ├── inferred-architecture.json
│   ├── goals.json
│   └── plan.md
├── gap/
│   ├── analysis.md
│   ├── files-to-add/
│   ├── contracts-to-add/
│   ├── migrations-to-add/
│   └── api-surfaces-to-add/
└── summary.json
```

### Evidence Graph CLI

```bash
treehouse evidence query --repo . --kind entity --since 2026-07-01
treehouse evidence snapshot --repo . --output evidence-snapshot.json
```

---

## Desktop App

Run the GUI explorer:

```bash
cargo run -p treehouse-app
```

Implemented UX includes:
- Open structured files
- Progressive disclosure with Overview-first inferred model summary
- Multi-pane layout with left navigation tree, center content views, right inspector, and bottom utility panel
- Graph + intelligence panel with confidence filtering
- Search, JSONPath, stats, and live System Diff result panes
- Live System Diff target selection: add repository paths, auto-discover Desktop repos, and connect to a selected target feed
- In-app cold start for selected monitor target: one-click generation of initial `.treehouse` artifacts (optional baseline scan)
- In-app Twin Runtime Controls: build twin bundle, generate projections, start/stop API gateway, start/stop CAR hot reload mode, run inferred capability execution, and one-click end-to-end twin software execution for the selected monitor target
- Diff mode (open base file and compare)
- Bookmarks and recent files
- Command palette actions with categories and descriptions
- Focus mode and keyboard navigation shortcuts (Cmd/Ctrl+K palette, Cmd/Ctrl+D diff toggle, arrows/Enter/Esc tree navigation)

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
