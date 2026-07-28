# Unified Evidence Graph

Treehouse now writes observed development signals into a unified append-only evidence store under:

- `.treehouse/evidence/nodes.jsonl`
- `.treehouse/evidence/edges.jsonl`

Each evidence record captures:

- a typed evidence kind (git delta, symbol, migration, API, workflow, entity, test, runtime event, DB signal, or system diff finding)
- confidence
- provenance
- observation timestamp
- optional subsystem attribution

## CLI

Query evidence:

```bash
treehouse evidence query --repo . --kind entity --since 2026-07-01
```

Export a point-in-time snapshot:

```bash
treehouse evidence snapshot --repo . --output evidence-snapshot.json
```

## Notes

The evidence store is append-only and additive with existing System Diff outputs.
