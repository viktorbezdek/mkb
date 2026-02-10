# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
