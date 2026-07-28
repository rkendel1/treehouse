# Continuous Architecture Runtime (CAR) v1

CAR v1 turns Treehouse watch mode into a continuous architecture runtime that updates a semantic projection each cycle.

## What CAR v1 adds

- Runtime projection generated every watch cycle.
- Subsystem health scoring.
- Architectural alarms.
- Runtime timeline for architecture evolution.
- Auto-generated architecture runtime documentation.

## Runtime artifacts

For each watched repository, CAR writes:

- `.treehouse/runtime/runtime.json`
- `.treehouse/runtime/health.json`
- `.treehouse/runtime/alarms.json`
- `.treehouse/runtime/timeline.json`
- `.treehouse/docs/architecture-runtime.md`

## CLI access

- `treehouse runtime <repo-path>`
- `treehouse runtime <repo-path> --health`
- `treehouse runtime <repo-path> --alarms`
- `treehouse runtime <repo-path> --timeline`
- `treehouse runtime <repo-path> --docs`

## CAR + Knowledge Graph

CAR consumes and enriches the knowledge graph projections in:

- `.treehouse/knowledge/graph.json`
- `.treehouse/knowledge/nodes.json`
- `.treehouse/knowledge/edges.json`
- `.treehouse/knowledge/timeline.json`
- `.treehouse/knowledge/drift/report.json`

## Current limitations

CAR v1 still performs full snapshot capture per cycle. Incremental AST and selective recomputation are planned for CAR v2.

## CAR v2 priorities

1. Incremental parser and affected-subgraph recomputation.
2. Architectural fitness-rule engine with pass/fail evidence edges.
3. Blast-radius and impact traversal APIs.
4. Runtime telemetry overlay ingestion (spans, latency, retries, errors).
5. Autonomous repair proposals with guarded apply/rollback.
