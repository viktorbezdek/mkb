# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] — 2026-02-10

### Added

#### Saved Views
- Named MKQL queries stored as YAML files in `.mkb/views/`
- `mkb view save <name> <mkql>` — save a query as a named view
- `mkb view list` — list all saved views
- `mkb view run <name>` — execute a saved view
- `mkb view delete <name>` — remove a saved view
- `mkb query --save <name>` — save while querying
- `mkb query --view <name>` — run a saved view inline

#### NEAR() Vector Similarity in MKQL
- `NEAR('query text', 0.8)` predicate for vector similarity filtering
- Combines with standard WHERE predicates: `WHERE NEAR('ml', 0.7) AND status = 'active'`
- Two-phase execution: KNN candidates filtered by threshold, then intersected with SQL results

#### Semantic Search CLI
- `mkb search --semantic "query"` — vector similarity search via CLI
- `mkb search --embedding '[0.1, ...]'` — search with pre-computed embedding vectors
- Mock embedding backend ported to Rust (SHA256-based deterministic hashing)

#### Graph Visualization
- `mkb graph --center <id> --depth N` — BFS traversal from a document
- `mkb graph --type <type>` — visualize all documents of a type
- Output formats: DOT (Graphviz), Mermaid, JSON
- Supports relationship-type edge labels

#### MCP Server (Model Context Protocol)
- `mkb mcp` — start MCP server on stdio for LLM tool integration
- 6 read-only tools: `mkb_query`, `mkb_search`, `mkb_search_semantic`, `mkb_get_document`, `mkb_list_types`, `mkb_vault_status`
- Resource templates: `mkb://vault/{type}/{id}`, `mkb://query/{mkql}`
- Built on `rmcp` SDK with full JSON-RPC over stdio

#### File Watcher
- `mkb watch` — auto-reindex vault on file changes
- Cross-platform support via `notify` crate (FSEvents on macOS, inotify on Linux)
- Handles create, modify, and delete events for `.md` files
- Debounced event processing to avoid redundant reindexing

#### CSV Ingestion Adapter
- `CsvAdapter` for bulk CSV-to-document ingestion (Python)
- Auto-detection of date columns by column name hints and cell value sampling
- Date normalization: ISO, slash dates (M/D/YYYY), written dates (March 5, 2025)
- Custom column mapping via `CsvColumnMapping` for title, date, and body columns

#### Shell Completions
- `mkb completions <shell>` — generate shell completions for bash, zsh, fish, powershell

#### Benchmarks
- `mkb-bench` binary for reproducible performance measurements
- Measures bulk ingest, FTS search, MKQL query, and KNN search at 100/1K/10K document scales
- Reports throughput (docs/sec) and p50/p95/p99 latencies

### Stats
- **254 tests** (180 Rust + 74 Python), all passing
- **8 Rust crates** in workspace (added `mkb-mcp`)
- **7 Python modules** with full type coverage (mypy strict + ruff)

---

## [0.1.0] — 2026-02-10

### Added

#### Phase 0 — Repository Bootstrap
- Cargo workspace with 7 crates (core, parser, index, vault, query, cli, python)
- Python package structure (`mkb_ai/`) with extraction, embeddings, confidence, ingestion modules
- CI/CD pipelines for Rust and Python (GitHub Actions + Dependabot)
- YAML schema definitions for all built-in types (project, meeting, decision, signal, document)
- Architecture Decision Records (ADR-001 through ADR-012)
- Technical specification v1.0 and phase spec v2

#### Phase 1 — Steel Thread & Temporal Hardening
- Core types: Document, TemporalFields, Link, SchemaDefinition, MkbError
- Temporal gate: hard rejection of documents without `observed_at`
- Decay models: exponential decay with per-type half-lives (signal=7d, project=14d, decision=never)
- Schema validation engine with field type checking and enum enforcement
- Link indexing with forward/reverse traversal
- Rejection log for temporal gate failures with actionable suggestions
- Document ID collision handling with counter suffixes
- MKQL pest PEG grammar with full query language support

#### Phase 2 — Query Engine
- MKQL parser: SELECT, FROM, WHERE, ORDER BY, LIMIT, OFFSET, LINKED, FRESH, STALE, CURRENT
- Query compiler: MKQL AST → parameterized SQL with FTS5 integration
- Query executor: end-to-end MKQL execution against SQLite index
- Context assembler: token-budget-aware document assembly for LLM context windows
- Output formatters: JSON, Markdown, and table output modes

#### Phase 3 — CLI & Vault Operations
- `mkb init` — create vault directory structure with schema files
- `mkb add` — create documents with temporal gate enforcement
- `mkb add --file` — ingest from markdown files
- `mkb edit` — update document fields (title, body, tags, confidence)
- `mkb rm` — soft-delete to `.archive/` directory
- `mkb query` — execute MKQL queries with JSON/table/markdown output
- `mkb search` — full-text search via FTS5
- `mkb link` — create and list inter-document links
- `mkb gc` — sweep stale/expired documents
- `mkb stats` — vault summary (counts, staleness, type distribution)
- `mkb status` — health check (document counts, index freshness)
- `mkb schema` — list available schema types
- `mkb ingest` — file ingestion pipeline via CLI
- 16 end-to-end CLI integration tests

#### Phase 4 — AI Layer & Semantic Search
- PyO3/maturin bridge: 16 Python-callable functions for vault, index, and temporal operations
- sqlite-vec integration for vector similarity search (1536-dimension embeddings)
- Embedding generation pipeline with OpenAI (text-embedding-3-small) and mock backends
- Rule-based date extraction: ISO datetime, ISO date, written dates, slash dates, relative references
- Rule-based entity extraction: Jira tickets, @mentions, person names with titles, URLs, emails
- Confidence scoring: weighted source/precision/completeness/corroboration model
- Enriched ingestion pipeline: auto-extract dates, entities, tags; optional embedding; dry run mode

### Stats
- **216 tests** (150 Rust + 66 Python), all passing
- **7 Rust crates** in workspace
- **6 Python modules** with full type coverage (mypy strict + ruff)
