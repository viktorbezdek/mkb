# MKB Implementation — Revised Phase Specification v2

*Based on Council Debate (4 agents, 3 rounds) + Fabric analysis.*
*Original plan: 6 phases, 10 weeks. Revised: 5 phases, 12-14 weeks.*

---

## Key Changes from v1

| Aspect | v1 (Original) | v2 (Revised) |
|--------|---------------|--------------|
| Phase count | 6 phases | 5 phases |
| Timeline | 10 weeks | 12-14 weeks |
| Phase 1-2 | Separate (Types → Storage) | Merged into "Temporal Core" |
| Phase 3 | Monolithic parser+query | Split into 3a (parser) / 3b (query+CLI) |
| Phase 5 | Full AI layer + embeddings | Timeboxed to embeddings-only |
| Phase 6 | Source adapters + release | Cut adapters, release-only |
| Steel thread | Week 6 (first E2E) | Week 2 (init+add+query) |
| Source adapters | Jira, Slack, GDocs in v0.1.0 | File+CSV only, rest deferred to v0.2.0 |
| MKQL aggregations | In v0.1.0 | Deferred to v0.2.0 |
| Implicit extraction | In v0.1.0 | Deferred to v0.2.0 |
| Visualization | ADR-011 mentions Phase 5 | Deferred to v0.2.0 |

---

## Revised Phase Structure

### Phase 1: Temporal Core (Weeks 1-4)

**Merges original Phases 1 + 2. Goal: working `mkb init && mkb add && mkb query` by week 2.**

#### Week 1-2: Steel Thread
Build the thinnest vertical slice that proves the system works end-to-end.

```
TASK-100: Steel thread — types + vault + index + basic query
├── T-100.1: Document struct with temporal fields (observed_at, valid_until, precision)
│   ├── Test: document_requires_observed_at (RED first)
│   ├── Test: document_serializes_to_yaml_frontmatter
│   └── Test: document_roundtrip_preserves_all_fields
├── T-100.2: TemporalGate with hard rejection
│   ├── Test: gate_rejects_null_observed_at
│   ├── Test: gate_accepts_complete_temporal_fields
│   ├── Test: gate_computes_valid_until_from_decay_profile
│   └── Test: gate_returns_rejection_with_actionable_suggestion
├── T-100.3: YAML frontmatter parser (read + write)
│   ├── Test: parse_frontmatter_from_markdown
│   ├── Test: write_frontmatter_to_markdown
│   └── Test: rejects_yaml_without_observed_at
├── T-100.4: Vault CRUD (create, read, update, delete)
│   ├── Test: init_creates_directory_structure
│   ├── Test: create_document_writes_markdown_file
│   ├── Test: create_rejects_document_without_observed_at (gate integration)
│   ├── Test: read_document_parses_frontmatter_and_body
│   ├── Test: update_preserves_created_at_bumps_modified_at
│   └── Test: delete_soft_moves_to_archive
├── T-100.5: SQLite index — basic field + FTS5 indexing
│   ├── Test: creates_schema_on_init
│   ├── Test: index_document_stores_all_frontmatter_fields
│   ├── Test: fts_indexes_title_and_body
│   ├── Test: fts_search_returns_ranked_results
│   └── Test: full_rebuild_matches_incremental_index
├── T-100.6: Minimal CLI — init, add, query (raw JSON output)
│   ├── E2E: mkb_init_creates_vault_structure
│   ├── E2E: mkb_add_creates_document_with_temporal_gate
│   ├── E2E: mkb_add_rejects_without_observed_at
│   └── E2E: mkb_query_returns_json_results
└── GATE-1a: `mkb init && mkb add --observed-at 2025-02-10 && mkb query` works E2E
```

#### Week 3-4: Temporal Hardening
Deepen the temporal model, schema validation, and index operations.

```
TASK-110: Temporal hardening + schema validation
├── T-110.1: DecayModel with configurable profiles
│   ├── Test: exponential_decay_halves_at_half_life
│   ├── Test: project_status_decays_in_14_days
│   ├── Test: decision_never_decays
│   ├── Test: signal_decays_in_7_days
│   ├── Test: lower_precision_accelerates_decay
│   └── PropTest: decay_is_monotonically_decreasing
├── T-110.2: SchemaDefinition validation engine
│   ├── Test: validate_project_document_against_schema
│   ├── Test: validate_rejects_missing_required_field
│   ├── Test: validate_rejects_wrong_type
│   └── Test: all_built_in_schemas_parse_successfully
├── T-110.3: Link type and link indexing
│   ├── Test: link_creation_with_timestamp
│   ├── Test: store_and_retrieve_links
│   ├── Test: query_forward_links
│   └── Test: query_reverse_links
├── T-110.4: Temporal indexes and queries
│   ├── Test: query_by_observed_at_range
│   ├── Test: query_current_documents (not superseded, not expired)
│   ├── Test: query_with_effective_confidence
│   └── Test: staleness_sweep_marks_expired
├── T-110.5: RejectionLog with recovery + mkb status
│   ├── Test: rejected_doc_written_to_rejected_dir
│   ├── Test: rejection_includes_extraction_attempts
│   └── E2E: mkb_status_shows_rejection_count
├── T-110.6: File path resolution and naming
│   ├── Test: type_determines_subdirectory
│   ├── Test: slug_generated_from_title
│   └── Test: collision_appends_counter
└── GATE-1b: Core types + temporal gate at 95% coverage, schema validation works
```

### Phase 2: Query Engine (Weeks 5-7)

**Original Phase 3, split into parser + query execution. MKQL becomes usable.**

```
TASK-200: MKQL parser (pest grammar → AST)
├── T-200.1: SELECT statements
│   ├── Test: parse_select_star_from_type
│   ├── Test: parse_select_specific_fields
│   └── Test: parse_select_with_alias
├── T-200.2: WHERE clauses
│   ├── Test: parse_equality_predicate
│   ├── Test: parse_comparison_operators
│   ├── Test: parse_in_list
│   ├── Test: parse_like_pattern
│   ├── Test: parse_and_or_combinations
│   └── Test: parse_body_contains
├── T-200.3: Temporal functions
│   ├── Test: parse_fresh_duration
│   ├── Test: parse_stale_and_expired
│   ├── Test: parse_current_and_latest
│   ├── Test: parse_as_of_datetime
│   └── Test: parse_eff_confidence
├── T-200.4: LINKED() function
│   ├── Test: parse_linked_forward
│   ├── Test: parse_linked_reverse
│   └── Test: parse_linked_with_filter
├── T-200.5: ORDER BY, LIMIT, OFFSET
│   └── Test: parse_order_by_multiple_fields
├── T-200.6: AST types and transformation
│   ├── Test: ast_roundtrip_simple_query
│   ├── Test: ast_roundtrip_complex_query
│   └── Test: parser_error_messages_are_helpful
├── T-200.7: Property tests for grammar robustness
│   ├── PropTest: valid_queries_parse
│   └── PropTest: random_strings_dont_panic
└── GATE-2a: All MKQL syntax from spec parses correctly
```

```
TASK-210: Query compilation + execution
├── T-210.1: MKQL → SQL compiler for field predicates
│   ├── Test: compile_equality_to_sql
│   ├── Test: compile_in_list_to_sql
│   ├── Test: compile_body_contains_to_fts5
│   └── Test: compile_parameterizes_values (NO SQL injection)
├── T-210.2: Temporal function compilation
│   ├── Test: compile_fresh_to_observed_at_range
│   ├── Test: compile_current_excludes_superseded_and_expired
│   └── Test: compile_eff_confidence_with_decay
├── T-210.3: LINK clause compilation (joins/CTEs)
│   ├── Test: compile_forward_link_to_join
│   └── Test: compile_multi_hop_to_recursive_cte
├── T-210.4: ResultFormatter (JSON, Table, Markdown)
│   ├── Test: format_as_json
│   ├── Test: format_as_table
│   └── Test: format_as_markdown
├── T-210.5: ContextAssembler for LLM context windows
│   ├── Test: assembler_prioritizes_high_confidence_fresh_docs
│   ├── Test: assembler_respects_token_budget
│   └── Test: assembler_falls_back_to_summary_format
└── GATE-2b: Full query compilation + execution tested against live SQLite
```

### Phase 3: CLI Polish (Weeks 8-9)

**All CLI commands working end-to-end. System is dogfoodable.**

```
TASK-300: CLI commands
├── T-300.1: mkb init (with custom config support)
│   └── E2E: init_with_custom_config
├── T-300.2: mkb add (interactive + from-file + --observed-at flag)
│   ├── E2E: add_project_interactively
│   └── E2E: add_from_file
├── T-300.3: mkb query / mkb q (full MKQL with format flags)
│   ├── E2E: query_with_format_flag
│   ├── E2E: query_temporal_functions
│   └── E2E: query_pipe_to_stdout
├── T-300.4: mkb search / mkb s (full-text search)
│   └── E2E: search_fulltext
├── T-300.5: mkb edit + mkb rm
│   ├── E2E: edit_updates_fields
│   └── E2E: rm_soft_and_hard_delete
├── T-300.6: mkb link (create + list relationships)
│   ├── E2E: link_create_relationship
│   └── E2E: link_list_relationships
├── T-300.7: mkb schema (list + validate)
│   ├── E2E: schema_list
│   └── E2E: schema_validate
├── T-300.8: mkb gc (sweep stale + find contradictions)
│   ├── E2E: gc_sweep_stale
│   └── E2E: gc_find_contradictions
├── T-300.9: mkb stats (vault summary)
│   └── E2E: stats_shows_vault_summary
├── T-300.10: mkb status (rejection count, index health)
│   └── E2E: status_shows_rejection_count
├── T-300.11: mkb ingest file/csv (basic ingest, no AI)
│   ├── E2E: ingest_file_with_frontmatter
│   ├── E2E: ingest_csv_auto_detects_date_column
│   └── E2E: ingest_rejects_undated_by_default
└── GATE-3: All CLI commands work E2E, tested in CI
```

### Phase 4: AI Layer (Weeks 10-12)

**Timeboxed to: PyO3 bridge, embeddings, semantic search. No implicit extraction.**

```
TASK-400: PyO3 bridge
├── T-400.1: Expose vault CRUD to Python
│   ├── Test: python_can_create_document
│   ├── Test: python_can_query_mkql
│   └── Test: python_receives_temporal_validation_errors
├── T-400.2: Expose index operations to Python
│   └── Test: python_can_search_fts
├── T-400.3: Expose temporal gate to Python
│   └── Test: python_gate_rejects_missing_observed_at
└── GATE-4a: maturin develop builds, Python imports work
```

```
TASK-410: Embeddings + semantic search (sqlite-vec)
├── T-410.1: Embedding generation (OpenAI text-embedding-3-small)
│   ├── Test: generate_embedding_for_document
│   └── Test: embedding_dimensions_match_config
├── T-410.2: sqlite-vec index management
│   ├── Test: add_and_query_vectors
│   ├── Test: persist_and_reload_index
│   └── Benchmark: performance_at_50k_documents (GATE: < 100ms p99)
├── T-410.3: Integrate NEAR() with sqlite-vec
│   ├── Test: near_query_returns_similar_documents
│   └── Test: near_combined_with_field_filter
├── T-410.4: mkb search --semantic
│   └── E2E: semantic_search_finds_relevant_docs
└── GATE-4b: Semantic search works E2E, sqlite-vec validated at 50K docs
```

```
TASK-420: Explicit extraction (no LLM, rule-based only)
├── T-420.1: Date/time extraction from text
│   ├── Test: extract_dates_from_text
│   └── Test: extract_relative_dates
├── T-420.2: Entity extraction (regex-based)
│   ├── Test: extract_jira_ticket_ids
│   └── Test: extract_person_mentions
├── T-420.3: Confidence scorer
│   ├── Test: score_human_authored_document
│   └── Test: score_with_corroboration_boost
├── T-420.4: mkb ingest with AI enrichment
│   ├── E2E: ingest_file_with_ai_enrichment
│   ├── E2E: ingest_directory_batch
│   └── E2E: ingest_dry_run
└── GATE-4c: Explicit extraction pipeline works E2E
```

### Phase 5: Release (Weeks 13-14)

**Polish, cross-platform builds, v0.1.0.**

```
TASK-500: Release preparation
├── T-500.1: Full QA audit (/audit all)
├── T-500.2: sqlite-vec stress test at 50K documents
│   └── Benchmark: brute-force search latency acceptable
├── T-500.3: Cross-platform binary builds
│   ├── Test: linux-x86_64 binary runs
│   ├── Test: macos-arm64 binary runs
│   └── Test: windows-x86_64 binary runs
├── T-500.4: Python wheel builds
│   ├── Test: wheel installs in clean venv
│   └── Test: import mkb works
├── T-500.5: Version bump to 0.1.0
├── T-500.6: CHANGELOG.md generation
├── T-500.7: README.md with quickstart guide
├── T-500.8: Create v0.1.0 tag and push
└── GATE-5: v0.1.0 released, binaries downloadable, pip installable
```

---

## Deferred to v0.2.0

| Feature | Original Phase | Reason for Deferral |
|---------|---------------|---------------------|
| MKQL aggregations (GROUP BY, HAVING) | Phase 3 | 20% use case, 50% complexity |
| Implicit LLM extraction | Phase 5 | Research territory, not v0.1.0 |
| Contradiction resolution | Phase 1 | Confidence scoring sufficient |
| Source adapters (Jira, Slack, GDocs) | Phase 6 | OAuth 3-4 weeks each |
| `mkb serve` (HTTP API) | Phase 6 | No consumer until MCP (Phase 7) |
| `mkb graph` visualization | ADR-011 | Textual output sufficient for v0.1.0 |
| `mkb repl` (interactive shell) | Phase 4 | Nice-to-have, CLI covers use case |
| VectorBackend trait abstraction | Phase 5 | Premature until sqlite-vec limits hit |
| Saved views (YAML definitions) | Phase 6 | Advanced feature |
| File watcher auto-indexing | Phase 6 | Manual `mkb index rebuild` sufficient |

---

## TDD Priority Map

| Area | Priority | Test Type | Rationale |
|------|----------|-----------|-----------|
| Temporal gate | P0 | Unit + PropTest | Core invariant, one bug = untrustworthy system |
| Decay model math | P0 | Unit + PropTest | Financial-grade math, monotonicity required |
| MKQL → SQL compilation | P0 | Unit + PropTest | SQL injection risk, silent wrong results |
| Index rebuild consistency | P1 | Property tests | Divergence = lost user trust |
| Frontmatter roundtrip | P1 | Unit + PropTest | Data integrity across read/write |
| CLI E2E commands | P1 | E2E tests | Integration validation |
| sqlite-vec at scale | P2 | Benchmarks | Performance ceiling validation |
| PyO3 FFI error handling | P2 | Unit tests | Rust panics across FFI = UB |
| Embedding generation | P3 | Integration | External API dependency |

---

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| sqlite-vec degrades at 30K+ docs | Medium | High | Benchmark at 50K in Phase 4, escape hatch to hnswlib |
| PyO3/maturin version mismatch | Medium | Medium | Pin versions, test in CI matrix |
| MKQL grammar too complex for pest | Low | High | Start with SELECT/WHERE subset, extend incrementally |
| EAV schema too slow for temporal joins | Medium | High | Prototype in Phase 1 week 1, EXPLAIN all queries |
| Temporal rejection UX causes user abandonment | Medium | High | mkb status command, clear error messages with suggestions |
| 10-week timeline insufficient | High | Medium | Budget 12-14 weeks per Datasette precedent |

---

## Parallelization Map

| Week | Track A (Rust Core) | Track B (Python) |
|------|---------------------|------------------|
| 1-2 | Steel thread (types+vault+index+CLI) | — |
| 3-4 | Temporal hardening + schema | — |
| 5-6 | MKQL parser (pest grammar) | — |
| 7 | Query compilation + execution | — |
| 8-9 | CLI polish (all commands) | — |
| 10-11 | — | PyO3 bridge + embeddings |
| 12 | — | Explicit extraction |
| 13-14 | Release prep | Release prep |

*Note: Python track starts after Rust core stabilizes (week 10) to avoid churn in PyO3 bindings.*

---

## v0.1.0 Release Criteria

- [ ] All Phase 1-5 gates passed
- [ ] Rust coverage >= 80%
- [ ] Python coverage >= 85%
- [ ] Temporal gate: 95%+ coverage
- [ ] Zero `cargo audit` / `pip-audit` findings above WARN
- [ ] `mkb init && mkb add project --title "Test" --observed-at 2025-02-10` works
- [ ] `mkb q 'SELECT * FROM project WHERE CURRENT()'` returns results
- [ ] `mkb search --semantic "team velocity"` finds relevant docs
- [ ] `mkb ingest file notes.md` processes with explicit extraction
- [ ] `mkb status` shows vault health and rejection count
- [ ] sqlite-vec validated at 50K documents (< 100ms p99 query)
- [ ] Cross-platform binaries: Linux x86, Linux ARM, macOS ARM, Windows
- [ ] Python wheel: `pip install mkb && python -c "import mkb"` works
- [ ] CHANGELOG.md and README.md complete
