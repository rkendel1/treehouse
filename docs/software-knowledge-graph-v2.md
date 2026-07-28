# Software Knowledge Graph v2

Treehouse now emits a typed knowledge graph as a first-class artifact during watch cycles.

## Purpose

Move from snapshot-only observations to a living digital twin substrate that supports:

- Stable architecture identities
- Typed entities and relationships
- Timeline-aware evolution tracking
- Drift and reasoning projections

## Generated artifacts

Each watch run writes these files under the watched repository:

- .treehouse/knowledge/graph.json
- .treehouse/knowledge/nodes.json
- .treehouse/knowledge/edges.json
- .treehouse/knowledge/timeline.json
- .treehouse/knowledge/drift/report.json

## Current node model

- Repository
- Subsystem
- Capability
- Api
- Workflow
- Symbol
- Migration
- RuntimeEvent
- Finding

## Current edge model

- Owns
- Exposes
- DependsOn
- Observes
- Produces
- Documents
- Violates

## Stable node IDs

Node IDs are canonicalized as:

<type>/<slug>

Examples:

- capability/provider-routing
- subsystem/auth
- api/get-chat-completions

## Query commands

- treehouse graph <repo-path> [--contains text] [--type node-type]
- treehouse why <repo-path> <term>
- treehouse drift <repo-path>

## Projection commands

- treehouse project <application-model.json> --target <postgres|convex|gateway|all> [--output dir]
- scripts/launch-projection.sh <repo-path> --mode gateway|all|postgres|convex

## Next expansion targets

1. Runtime overlays (spans, retries, latency, allocations).
2. Architectural fitness rules with pass/fail evidence edges.
3. Ownership graph enrichment from CODEOWNERS, PR metadata, and review history.
4. Impact and blast-radius traversals over typed edges.
5. Query DSL and interactive explorer views in the desktop app.
