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
- treehouse twin simulate <bundle-file> --workflow <name> [--events event_a,event_b]
- treehouse twin what-if <bundle-file> --workflow <name> [--events event_a,event_b] [--remove-state state|--remove-transition from:to]

## What twin.v1 includes

- Architecture model (node/edge/API/symbol counts)
- Behavior model with Stage 2 inference (workflow structures + inferred transitions from call/event/dataflow signals)
- Capability model with Stage 3 intent taxonomy and confidence scoring
- Runtime model (confidence, alarms, subsystem health)
- Deterministic Stage 4 execution semantics for workflow simulation and pre-change impact runs

## What this enables now

- Portable software twin bundles from repo state
- Cross-repository comparison by capability overlap
- Capability-level execution traces against inferred twin model

## Stage Status

1. Stage 2 Behavioral Twin (implemented):
   - workflow transition inference from model + runtime event + relationship/dataflow signals
   - persisted executable workflow graph in the twin bundle
2. Stage 3 Intent Twin (implemented):
   - capability taxonomy domains inferred from capability/API/dependency/workflow evidence
   - confidence score + evidence trace per capability intent profile
3. Stage 4 Execution Twin (implemented):
   - deterministic workflow simulator (`twin simulate`)
   - pre-change what-if impact analysis (`twin what-if`)
4. Stage 5 Runtime Twin:
   - telemetry overlays (latency, failures, retries, routing distributions)
5. Stage 6 Reasoning Twin:
   - why/impact/planner queries over typed graph and twin traces
6. Stage 7 Autonomous Twin:
   - predictive change simulation and policy-gated refactor plans
