# MKB — Markdown Knowledge Base for LLMs

A file-system-native knowledge base where every knowledge unit is a markdown file with YAML frontmatter. Built for LLM context assembly with temporal awareness, semantic search, and a full query language.

**Rust core + Python AI layer. Single binary via PyO3/maturin.**

## Features

- **Temporal-first** — Every document has `observed_at`, confidence decay, and staleness tracking. No information enters the vault without a timestamp.
- **MKQL query language** — SQL-like queries with temporal predicates: `FRESH()`, `STALE()`, `CURRENT()`, `NEAR()`, `LINKED()`, `AS_OF()`
- **Full-text search** — FTS5-powered text search across all document fields
- **Semantic search** — sqlite-vec KNN similarity search with 1536-dimension embeddings
- **Graph visualization** — Explore document relationships in DOT, Mermaid, or JSON format
- **MCP server** — Expose your vault as tools for LLM assistants via Model Context Protocol
- **File watcher** — Auto-reindex on vault changes
- **Saved views** — Name and reuse frequently-run MKQL queries
- **AI ingestion** — Extract dates, entities, and embeddings from unstructured text (Python)
- **CSV import** — Bulk ingest CSV files with automatic date column detection

## Quick Start

```bash
# Install from source
pip install -e .

# Initialize a vault
mkb init my-vault
cd my-vault

# Add a document
mkb add --doc-type project --title "Alpha Project" \
  --observed-at 2026-02-10T12:00:00Z \
  --body "Our new ML pipeline project"

# Query with MKQL
mkb query "SELECT * FROM project WHERE CURRENT()"

# Full-text search
mkb search "machine learning"

# Semantic search
mkb search "neural networks" --semantic

# Visualize relationships
mkb graph --type project --format mermaid

# Start MCP server for LLM integration
mkb mcp
```

## Installation

### From Source (Recommended)

Requires Rust 1.82+ and Python 3.11+.

```bash
git clone https://github.com/viktorbezdek/mkb.git
cd mkb

# Install Python package with Rust extension
pip install -e .

# Or build just the Rust binary
cargo build --release
```

### Shell Completions

```bash
# Bash
mkb completions bash > ~/.bash_completion.d/mkb

# Zsh
mkb completions zsh > ~/.zfunc/_mkb

# Fish
mkb completions fish > ~/.config/fish/completions/mkb.fish
```

## MKQL — Query Language

MKQL (Markdown Knowledge Query Language) is a SQL-like language designed for temporal document queries.

```sql
-- Find all current projects
SELECT * FROM project WHERE CURRENT()

-- Recent meetings from the last 7 days
SELECT title, observed_at FROM meeting WHERE FRESH('7d') ORDER BY observed_at DESC

-- Semantic similarity with field filtering
SELECT * FROM document WHERE NEAR('machine learning', 0.8) AND status = 'active'

-- Follow document links
SELECT * FROM decision WHERE LINKED('proj-alpha-001')

-- Time-travel query
SELECT * FROM project WHERE AS_OF('2026-01-01T00:00:00Z')

-- Confidence-weighted results
SELECT * FROM signal WHERE EFF_CONFIDENCE(> 0.7)
```

### Temporal Predicates

| Predicate | Description |
|-----------|-------------|
| `CURRENT()` | Documents not yet expired (valid_until > now) |
| `FRESH('7d')` | Observed within the given duration |
| `STALE('30d')` | Not observed within the given duration |
| `EXPIRED()` | Past their valid_until date |
| `AS_OF('datetime')` | Documents as they existed at a point in time |
| `EFF_CONFIDENCE(> 0.7)` | Effective confidence after decay |
| `NEAR('text', 0.8)` | Vector similarity above threshold |
| `LINKED('doc-id')` | Documents linked to a given document |

## CLI Reference

| Command | Description |
|---------|-------------|
| `mkb init [path]` | Initialize a new vault |
| `mkb add` | Create a document with temporal gate |
| `mkb add --from-file` | Ingest from markdown with frontmatter |
| `mkb query <mkql>` | Execute an MKQL query |
| `mkb search <text>` | Full-text search (FTS5) |
| `mkb search --semantic` | Vector similarity search |
| `mkb edit <id>` | Update document fields |
| `mkb rm <id>` | Soft-delete to archive |
| `mkb link create` | Create inter-document links |
| `mkb link list <id>` | List document links |
| `mkb graph` | Visualize document relationships |
| `mkb view save/list/run/delete` | Manage saved MKQL views |
| `mkb watch` | Auto-reindex on file changes |
| `mkb mcp` | Start MCP server on stdio |
| `mkb ingest <path>` | Bulk ingest files |
| `mkb gc` | Sweep stale documents |
| `mkb stats` | Vault statistics |
| `mkb status` | Health check |
| `mkb schema list` | List document schemas |
| `mkb completions <shell>` | Generate shell completions |

## MCP Server

MKB exposes a read-only MCP server for LLM tool integration:

```json
{
  "mcpServers": {
    "mkb": {
      "command": "mkb",
      "args": ["mcp", "--vault", "/path/to/vault"]
    }
  }
}
```

### Available Tools

| Tool | Description |
|------|-------------|
| `mkb_query` | Execute MKQL queries |
| `mkb_search` | Full-text search |
| `mkb_search_semantic` | Vector similarity search |
| `mkb_get_document` | Read a document by type + ID |
| `mkb_list_types` | List available document types |
| `mkb_vault_status` | Vault health statistics |

### Resource URIs

- `mkb://vault/{type}/{id}` — Read a document
- `mkb://query/{mkql}` — Execute an MKQL query (URL-encoded)

## Document Format

Every document is a markdown file with YAML frontmatter:

```markdown
---
id: proj-alpha-001
type: project
title: Alpha Project
observed_at: "2026-02-10T12:00:00Z"
valid_until: "2026-02-24T12:00:00Z"
confidence: 0.95
precision: day
tags:
  - ml
  - infrastructure
status: active
---

Project description and notes go here.

Supports full **markdown** formatting.
```

## Architecture

```
mkb/
├── crates/
│   ├── mkb-core/     # Types, schemas, temporal model
│   ├── mkb-parser/   # MKQL PEG grammar (pest)
│   ├── mkb-index/    # SQLite + FTS5 + sqlite-vec indexer
│   ├── mkb-vault/    # File system CRUD + watcher
│   ├── mkb-query/    # Query compiler + executor + graph
│   ├── mkb-mcp/      # MCP server (rmcp SDK)
│   ├── mkb-cli/      # CLI binary (clap)
│   └── mkb-python/   # PyO3 bridge
└── python/
    └── mkb_ai/       # AI ingestion, embeddings, extraction
```

### Key Design Decisions

- **SQLite + FTS5** for field indexing and full-text search
- **sqlite-vec** for vector similarity (KNN) with 1536-dim embeddings
- **Temporal gate** — hard rejection of documents without `observed_at`
- **Exponential decay** — confidence degrades over time (configurable per type)
- **File-first** — vault is a directory of markdown files, index is derived

## Benchmarks

Processing and retrieval accuracy measured on synthetic ground-truth documents across three scales. Each document has known keywords, temporal properties, and cluster membership for verifiable precision/recall.

*Run benchmarks: `cargo run --release --bin mkb-bench`*

<!-- BENCHMARK_RESULTS_START -->
```
Platform: macOS aarch64 (Apple Silicon)

Processing Accuracy

| Metric                      |  100 docs |   1K docs |  10K docs |
|-----------------------------|-----------|-----------|-----------|
| Ingest Accuracy             |    100.0% |    100.0% |    100.0% |
| Field Preservation          |    100.0% |    100.0% |    100.0% |
| Temporal Integrity          |    100.0% |    100.0% |    100.0% |

Retrieval Accuracy

| Metric                      |  100 docs |   1K docs |  10K docs |
|-----------------------------|-----------|-----------|-----------|
| FTS Precision@10            |    100.0% |    100.0% |    100.0% |
| FTS Recall                  |    100.0% |    100.0% |    100.0% |
| MKQL Type Filter (Jaccard)  |    100.0% |    100.0% |    100.0% |
| MKQL CURRENT() Precision    |    100.0% |    100.0% |    100.0% |
| MKQL FRESH('7d') (Jaccard)  |    100.0% |    100.0% |    100.0% |
| KNN Cluster Precision@10    |     20.0% |     20.0% |     20.0% |

Performance

| Metric                      |  100 docs |   1K docs |  10K docs |
|-----------------------------|-----------|-----------|-----------|
| Ingest Throughput           |   3.3K/s  |   3.0K/s  |   2.3K/s  |
| FTS Search (p50)            |    29 us  |    36 us  |    25 us  |
| MKQL Query (p50)            |    46 us  |   337 us  |     5 ms  |
| KNN Search (p50)            |   975 us  |     2 ms  |    18 ms  |
| Index Size                  |   7.0 MB  |  14.9 MB  | 147.7 MB  |
```

> **Note:** KNN Cluster Precision uses deterministic mock embeddings (SHA256-based) which don't produce semantic clusters. The 20% baseline matches random expectation with 5 clusters. Real embedding models (OpenAI, etc.) achieve significantly higher precision.
<!-- BENCHMARK_RESULTS_END -->

## Development

```bash
# Run all Rust tests
cargo test --workspace

# Run Python tests
uv run pytest tests/python/ -v

# Lint
cargo clippy --workspace -- -D warnings
uv run ruff check python/ tests/

# Type check
uv run mypy python/

# Format
cargo fmt
uv run ruff format python/ tests/

# Full CI check
just ci
```

## License

Apache-2.0
