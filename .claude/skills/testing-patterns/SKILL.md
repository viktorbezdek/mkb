---
name: testing-patterns
description: >
  Testing patterns and TDD workflows for MKB.
---
# Testing Patterns Skill

## Test Hierarchy
1. Unit tests — inline in source files (Rust) or `tests/unit/` (Python)
2. Integration tests — `tests/rust/` and `tests/python/`
3. E2E tests — `tests/e2e/`
4. Property tests — `proptest` (Rust), `hypothesis` (Python)

## Coverage Targets
- Rust: 80%+ line coverage (`cargo tarpaulin`)
- Python: 85%+ line coverage (`pytest-cov`)
- Critical paths (temporal gate, query engine): 95%+
