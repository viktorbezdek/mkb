---
name: rust-python-bridge
description: >
  PyO3/maturin Rust-Python bridge development.
---
# Rust-Python Bridge Skill

## Bridge Crate
`crates/mkb-python/` — thin translation layer, no business logic.

## Rules
1. Keep this crate THIN — translate types, don't compute
2. All heavy lifting in mkb-core, mkb-query, etc.
3. Python calls Rust for: parsing, querying, indexing, vault CRUD
4. Rust calls Python for: LLM inference, embedding generation
5. Use `pyo3-asyncio` for async Python-Rust interop
