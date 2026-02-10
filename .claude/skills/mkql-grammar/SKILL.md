---
name: mkql-grammar
description: >
  MKQL query language grammar and parser development.
---
# MKQL Grammar Skill

## Grammar Location
`crates/mkb-parser/src/mkql.pest`

## Key Design Rules
1. MKQL compiles to SQLite SQL + vector index queries
2. Every new function needs: grammar rule, AST node, compiler target
3. Temporal functions operate on `observed_at` field, NOT `_modified_at`
4. NEAR() generates vector similarity query against HNSW index
5. LINKED() generates recursive CTE or multi-join against links table
6. CONTEXT WINDOW triggers token-budgeted result assembly

## Test Pattern
For every grammar change:
1. Add parse test in `crates/mkb-parser/tests/`
2. Add compilation test in `crates/mkb-query/tests/`
3. Add integration test with real SQLite in `tests/rust/`

## Reference
See `docs/tech-spec.md` section 5 for full grammar EBNF.
