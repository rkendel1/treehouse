# Understanding Compiler Roadmap

Treehouse now includes a first portable twin artifact flow that treats repository code as input and emits an abstract digital twin package.

## Pipeline

Git Repository
-> Parsers
-> Semantic Graph
-> Knowledge + Runtime Projections
-> Twin Bundle (.twin.json)

## New twin commands

- treehouse twin build <repo-path> [--output file]
- treehouse twin inspect <bundle-file>
- treehouse twin compare <bundle-a> <bundle-b>
- treehouse twin run <bundle-file> --capability <name>

## What twin.v1 includes

- Architecture model (node/edge/API/symbol counts)
- Behavior model (workflow structures)
- Capability model (capability ownership, dependencies, exposed APIs)
- Runtime model (confidence, alarms, subsystem health)

## What this enables now

- Portable software twin bundles from repo state
- Cross-repository comparison by capability overlap
- Capability-level execution traces against inferred twin model

## Next roadmap slices

1. Stage 2 Behavioral Twin:
   - infer workflow sequences directly from call/event/dataflow signals
   - persist executable behavior graph
2. Stage 3 Intent Twin:
   - infer business capabilities from entities, APIs, workflows, docs
3. Stage 4 Execution Twin:
   - capability execution engine with deterministic simulation semantics
4. Stage 5 Runtime Twin:
   - telemetry overlays (latency, failures, retries, routing distributions)
5. Stage 6 Reasoning Twin:
   - why/impact/planner queries over typed graph and twin traces
6. Stage 7 Autonomous Twin:
   - predictive change simulation and policy-gated refactor plans
