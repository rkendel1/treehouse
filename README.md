# Treehouse

Explore structured data at any scale.

A native, cross-platform structured data explorer built in Rust.

Treehouse opens massive JSON, YAML, TOML, XML, CSV, and JSONL files instantly using lazy parsing, memory mapping, and streaming indexes. Instead of acting like another text editor, Treehouse understands the structure of your data and lets you explore it interactively.

The goal is to become TablePlus for structured documents.

---

## Vision

Most developers inspect structured data using text editors that were never designed for it.

Treehouse treats structured documents as navigable object graphs rather than text.

- Open a 15 GB JSON log.
- Expand only the branch you care about.
- Search millions of objects instantly.
- Infer schemas.
- Compare two APIs.
- Inspect data—not text.

---

## Goals

- Native Rust application
- Extremely fast startup
- Handles multi-GB files
- Memory efficient
- Cross-platform
- Offline-first
- Plugin architecture
- Beautiful UI

## Non Goals

- General purpose editor
- IDE replacement
- JSON formatter
- Database client

Treehouse is an explorer.

Editing is secondary.

---

## MVP

### File Support

- JSON
- JSONL / NDJSON
- YAML
- TOML

### Future

- XML
- CSV
- MessagePack
- BSON
- CBOR
- Avro
- Parquet
- Arrow

---

## Core Features

### Lazy Tree

Files are never fully materialized.

Every node exists as:

- Offset
- Length
- Type
- Children

Children load only when expanded.

Opening a 5 GB JSON should feel nearly instant.

### Virtualized Tree View

Instead of rendering:

```text
users
0
1
2
3
...
924383
924384
```

Treehouse renders:

```text
users (924,385)
0–999
1000–1999
2000–2999
```

Only visible rows exist in memory.

### Streaming Parser

Large files use streaming parsers.

- No recursive allocation.
- No full document deserialization.

Supported implementations:

- simd-json
- serde_json
- quick-xml
- yaml-rust

### Memory Mapping

Large files use `memmap2`.

Random access without loading the file.

### Global Search

Searches:

- Keys
- Values
- Paths
- Regex
- Types

Example queries:

- `customerId`
- `invoice`
- `2025`
- `error`
- `uuid`

Results populate while background indexes build.

### JSONPath

```text
$.orders[*]
$..price
$.customers[0]
$..metadata
```

Results highlight directly inside the tree.

### jq Console

Interactive console:

```jq
.orders[]
| select(.status=="paid")
```

Results stream into a secondary pane.

### Statistics

Automatic analysis:

- Objects
- Arrays
- Maximum depth
- Largest array
- Most common keys
- Repeated schemas
- Duplicate keys
- Null percentages
- Numeric ranges
- String lengths

Generated in parallel.

### Schema Inference

Click **Infer Schema**.

Produces:

```text
Customer
id UUID
email String
orders Array<Order>
createdAt Timestamp
```

Export:

- JSON Schema
- OpenAPI
- Rust structs
- TypeScript
- Go
- Kotlin

### Structural Search

Instead of searching text, search structure.

Examples:

- Find every UUID
- Find timestamps
- Find duplicate objects
- Find arrays over 10,000 elements
- Find nullable strings
- Find objects matching `id`, `createdAt`, and `updatedAt`

### Diff

Compare two structured files.

Instead of line diffs:

- Removed
- Added
- Changed
- Moved
- Renamed

Tree-aware comparison.

Supports:

- JSON
- YAML
- TOML

Future: cross-format (`JSON ↔ YAML`, `JSON ↔ TOML`).

---

## Future Features

### Graph View

Visualize relationships:

```text
Customer
↓
Orders
↓
Invoices
↓
Payments
```

### Timeline View

Recognize timestamps automatically:

- created
- updated
- deleted
- processed

Generate event timelines.

### API Inspector

Drop in:

- OpenAPI
- Swagger
- Postman
- Insomnia

Browse as live object graphs.

### Live Mode

Watch files. Updates stream into the tree.

Perfect for:

- logs
- telemetry
- API responses

### Plugin SDK

Plugins add:

- File formats
- Search providers
- Schema generators
- Exporters
- Diff engines

Rust traits only. No embedded scripting required.

---

## Architecture

```text
treehouse/
├── crates/
│   ├── treehouse-app/
│   ├── treehouse-ui/
│   ├── treehouse-core/
│   ├── treehouse-tree/
│   ├── treehouse-parser/
│   ├── treehouse-query/
│   ├── treehouse-search/
│   ├── treehouse-schema/
│   ├── treehouse-diff/
│   ├── treehouse-stats/
│   ├── treehouse-formats/
│   └── treehouse-plugin/
│
├── examples/
├── docs/
└── tests/
```

## Core Crates

### treehouse-core

Shared models:

- NodeId
- NodeType
- Tree
- Value
- Document
- Path

### treehouse-parser

Streaming parsers:

- JSON
- YAML
- XML
- TOML

Outputs lazy nodes.

### treehouse-tree

Virtual tree:

- Expansion
- Caching
- Selection
- Navigation

### treehouse-search

- Indexes
- Regex
- Fuzzy
- Path
- Structural search

### treehouse-query

- JSONPath
- jq

Future: JMESPath, XPath.

### treehouse-schema

- Schema inference
- OpenAPI
- JSON Schema
- Type generation

### treehouse-diff

Structural comparison engine.

### treehouse-stats

Background analytics. Runs on worker threads.

### treehouse-ui

Native desktop UI using egui/eframe.

Features:

- Dockable panes
- Virtualized tree
- Split diff view
- Search sidebar
- Inspector panel
- Command palette

---

## Technology

- Rust (stable)
- egui / eframe
- simd-json
- serde
- memmap2
- rayon
- parking_lot
- tokio (optional background tasks)
- anyhow
- tracing
- tracing-subscriber

---

## Milestone 1 — Foundation

- Workspace setup
- Native window
- File open dialog
- JSON parser
- Lazy tree
- Expand/collapse
- Virtual scrolling
- Search
- Statistics

## Milestone 2 — Explorer

- YAML
- TOML
- JSONPath
- Inspector
- Property panel
- Bookmarks
- Recent files
- Command palette

## Milestone 3 — Power Tools

- jq integration
- Structural diff
- Schema inference
- Exporters
- Plugin API
- Performance profiling
- Large-file benchmarking

---

## Long-Term Vision

Treehouse aims to become the definitive native application for exploring structured data. Just as developers instinctively reach for TablePlus when working with databases or Beyond Compare when comparing files, Treehouse should become the first tool they open when they need to understand the shape, contents, and evolution of JSON, YAML, TOML, XML, and other structured formats—whether the file is 10 KB or 100 GB.
