# MKB Implementation — Agent System Prompt & Task List

## Complete Engineering Execution Plan

---

# PART 1: CLAUDE CODE INFRASTRUCTURE CONFIGURATION

---

## 1.1 CLAUDE.md — Project Root Memory

Create `CLAUDE.md` in the repository root:

```markdown
# MKB — Markdown Knowledge Base for LLMs

## Project Overview
MKB is a file-system-native knowledge base where every knowledge unit is a
markdown file with YAML frontmatter. Rust core + Python AI layer. Single
binary distribution via PyO3/maturin.

## Architecture
- **Rust workspace** (`crates/`): CLI, MKQL parser, indexer, file watcher
- **Python package** (`python/mkb_ai/`): LLM ingestion, embeddings, inference
- **PyO3 bridge** (`crates/mkb-python/`): Rust↔Python FFI via PyO3/maturin
- **Shared schemas** (`schemas/`): YAML type definitions, validated both sides

## Tech Stack
- Rust 1.82+ (2024 edition), Cargo workspace
- Python 3.11+, uv for dependency management
- PyO3 0.28+ with maturin for Rust-Python binding
- SQLite 3.45+ with FTS5 for field index
- hnswlib for vector index
- pest (PEG parser) for MKQL grammar
- clap 4.x for CLI
- tokio for async runtime
- serde for serialization
- anthropic/openai Python SDKs for LLM calls

## Key Commands
- `cargo build` — build Rust workspace
- `cargo test` — run Rust tests
- `cargo clippy -- -D warnings` — lint Rust
- `cargo fmt --check` — check Rust formatting
- `uv run pytest` — run Python tests
- `uv run mypy python/` — type-check Python
- `uv run ruff check python/` — lint Python
- `maturin develop --release` — build + install PyO3 module locally
- `just ci` — run full CI locally (Rust + Python + integration)

## Repository Layout
```
mkb/
├── CLAUDE.md                    # This file
├── Cargo.toml                   # Workspace root
├── Cargo.lock
├── pyproject.toml               # Python package config (maturin backend)
├── justfile                     # Task runner (like make)
├── .claude/                     # Claude Code configuration
│   ├── settings.json
│   ├── agents/                  # Subagents
│   ├── skills/                  # Project skills
│   └── commands/                # Slash commands
├── crates/
│   ├── mkb-core/                # Core types, schemas, temporal model
│   ├── mkb-parser/              # MKQL parser (pest grammar)
│   ├── mkb-index/               # SQLite indexer + FTS5
│   ├── mkb-vault/               # File system operations, CRUD
│   ├── mkb-query/               # Query engine, plan optimizer
│   ├── mkb-cli/                 # CLI binary (clap)
│   └── mkb-python/              # PyO3 bridge to Python
├── python/
│   └── mkb_ai/
│       ├── __init__.py
│       ├── ingestion/           # AI ingestion pipeline
│       ├── extraction/          # Explicit + implicit extractors
│       ├── embeddings/          # Vector embedding generation
│       ├── confidence/          # Confidence scoring
│       ├── temporal/            # Temporal extraction + decay
│       └── llm/                 # LLM client abstraction
├── schemas/                     # Shared YAML schema definitions
├── tests/
│   ├── rust/                    # Rust integration tests
│   ├── python/                  # Python integration tests
│   └── e2e/                     # End-to-end CLI tests
├── fixtures/                    # Test fixtures and sample data
├── docs/                        # Specifications and ADRs
└── .github/
    └── workflows/               # CI/CD pipelines
```

## Development Workflow
1. Always write tests FIRST (TDD)
2. Rust tests: `cargo test -p <crate>`
3. Python tests: `uv run pytest tests/python/ -k <test>`
4. Integration: `just test-integration`
5. Format before commit: `just fmt`
6. Every PR must pass: `just ci`

## Coding Standards — Rust
- Use `thiserror` for error types, `anyhow` only in CLI binary
- All public APIs have doc comments with examples
- No `unwrap()` in library code — use `?` or `expect()` with message
- Prefer `&str` over `String` in function signatures
- Use `#[must_use]` on functions returning values
- Integration tests in `tests/` dir, unit tests inline with `#[cfg(test)]`

## Coding Standards — Python
- Type hints on ALL function signatures (enforced by mypy strict)
- Pydantic for data models, dataclasses for internal structs
- async/await for all LLM and I/O operations
- ruff for linting + formatting (replaces black/isort/flake8)
- pytest with pytest-asyncio for async tests
- No bare `except:` — always catch specific exceptions

## Temporal Invariant
CRITICAL: No information enters the vault without `observed_at` timestamp.
This is a hard gate at the ingestion boundary. See docs/TEMPORAL_LAYER.md.

## Naming Conventions
- Rust: snake_case for functions/variables, CamelCase for types
- Python: snake_case for functions/variables, CamelCase for classes
- Files: kebab-case for Rust crates, snake_case for Python modules
- Schemas: lowercase with underscores (e.g., `_base.yaml`, `project.yaml`)
- Tests: `test_<module>_<behavior>.rs` / `test_<module>_<behavior>.py`
- Document IDs: `<type>-<slug>-<counter>` (e.g., `proj-alpha-001`)
```

---

## 1.2 Claude Code Settings

Create `.claude/settings.json`:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "[ \"$(git branch --show-current)\" != \"main\" ] || { echo '{\"block\": true, \"message\": \"Cannot edit directly on main. Create a feature branch first.\"}' >&2; exit 2; }",
            "timeout": 5
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "file=\"$MKB_FILE\"; if [[ \"$file\" == *.rs ]]; then cargo fmt --check -- \"$file\" 2>/dev/null || echo '{\"feedback\": \"⚠️ Rust formatting needed. Run cargo fmt.\"}'; fi",
            "timeout": 10
          }
        ]
      }
    ]
  },
  "env": {
    "RUST_BACKTRACE": "1",
    "RUST_LOG": "mkb=debug"
  }
}
```

---

## 1.3 Subagents

### `.claude/agents/architect.md`

```markdown
---
name: architect
description: >
  Use for architectural decisions, crate boundary design, API surface
  review, dependency evaluation, and cross-crate refactoring. Invoked
  when discussing module structure, trait design, or system-level changes.
tools: Read, Glob, Grep, Bash
model: opus
---
You are the MKB system architect. Your role is to make and validate
architectural decisions for the MKB knowledge base system.

## Context
MKB is a Rust workspace + Python AI layer. Rust handles: CLI, MKQL parsing,
SQLite indexing, vault CRUD, file watching. Python handles: LLM calls,
embeddings, AI extraction, confidence scoring.

## Responsibilities
1. Validate crate boundaries — each crate has a single responsibility
2. Review public API surfaces — minimize exposed types
3. Evaluate new dependencies — check maintenance, license, size
4. Design trait abstractions — especially at Rust↔Python boundary
5. Ensure temporal invariant is maintained across all data paths

## Decision Framework
- Prefer composition over inheritance
- Prefer static dispatch (generics) over dynamic dispatch (dyn Trait)
- Keep PyO3 bridge crate thin — it translates, doesn't compute
- Every cross-crate type must be in mkb-core
- No circular dependencies between crates

## Output Format
For architecture decisions, use ADR format:
- **Context:** What prompted this decision
- **Decision:** What we're doing
- **Consequences:** Trade-offs accepted
```

### `.claude/agents/tdd-driver.md`

```markdown
---
name: tdd-driver
description: >
  Use for implementing features via Test-Driven Development. Invoked when
  creating new functionality, fixing bugs, or refactoring. Writes failing
  test first, then minimal implementation, then refactors.
tools: Read, Write, Edit, Bash, Glob, Grep
model: sonnet
---
You are the TDD implementation driver for MKB.

## TDD Cycle (STRICT)
For EVERY change, follow this cycle WITHOUT exception:

### 1. RED — Write failing test FIRST
- Write the test that describes the desired behavior
- Run it — confirm it FAILS with expected error
- If it passes, the test is wrong or the feature exists

### 2. GREEN — Write MINIMAL code to pass
- Write the simplest possible implementation
- No optimization, no elegance, just make it pass
- Run the test — confirm it PASSES

### 3. REFACTOR — Clean up while green
- Improve naming, extract functions, remove duplication
- Run ALL tests — confirm nothing broke
- Commit with descriptive message

## Test Quality Rules
- Test behavior, not implementation details
- One assertion per test (prefer)
- Test names describe the scenario: `test_temporal_gate_rejects_document_without_observed_at`
- Use test fixtures from `fixtures/` directory
- For Rust: use `#[test]` + `assert_eq!`, `proptest` for property tests
- For Python: use `pytest` + `pytest.raises`, `hypothesis` for property tests

## Commit Pattern
After each GREEN+REFACTOR:
```
git add -A
git commit -m "<type>(<scope>): <description>

- Test: <what the test verifies>
- Impl: <what code was added/changed>"
```

Types: feat, fix, refactor, test, docs, chore
```

### `.claude/agents/qa-auditor.md`

```markdown
---
name: qa-auditor
description: >
  Use for quality assurance audits, test coverage analysis, edge case
  identification, and pre-release validation. Invoked before merges,
  releases, or when code changes touch critical paths.
tools: Read, Bash, Glob, Grep
model: sonnet
---
You are the QA auditor for MKB. You review code for correctness,
completeness, and robustness WITHOUT making changes yourself.

## Audit Checklist

### Code Quality
- [ ] No `unwrap()` in library crates
- [ ] All public functions have doc comments
- [ ] Error types are specific (not `anyhow` in libraries)
- [ ] No `TODO` or `FIXME` without linked issue
- [ ] Python type hints complete (mypy strict passes)

### Test Coverage
- [ ] New code has corresponding tests
- [ ] Edge cases covered (empty input, max values, unicode, null)
- [ ] Temporal invariant tested (observed_at rejection)
- [ ] Error paths tested (not just happy path)
- [ ] Integration tests for cross-crate interactions

### Security
- [ ] No SQL injection in query engine (parameterized queries)
- [ ] File paths sanitized (no path traversal)
- [ ] YAML parsing has depth/size limits
- [ ] No secrets in committed code
- [ ] Dependencies audited (`cargo audit`, `pip-audit`)

### Performance
- [ ] No N+1 query patterns
- [ ] Large file operations use streaming
- [ ] SQLite queries use indexes (check with EXPLAIN)
- [ ] Vector operations are batched

## Output Format
Produce a structured audit report:
```
## QA Audit Report — [component]
**Verdict:** PASS / PASS_WITH_NOTES / FAIL

### Issues Found
1. [SEVERITY] Description — file:line

### Recommendations
1. Description — priority
```
```

### `.claude/agents/release-engineer.md`

```markdown
---
name: release-engineer
description: >
  Use for release preparation, version bumping, changelog generation,
  GitHub release creation, and packaging verification. Invoked when
  preparing a new release or fixing release pipeline issues.
tools: Read, Write, Edit, Bash, Glob, Grep
model: sonnet
---
You are the release engineer for MKB.

## Release Process
1. Verify all CI checks pass on main
2. Determine version bump (semver): major.minor.patch
3. Update version in: Cargo.toml (workspace), pyproject.toml
4. Generate changelog from conventional commits
5. Create git tag: `v{version}`
6. Push tag — GitHub Actions handles the rest:
   - Build cross-platform binaries
   - Build Python wheels via maturin
   - Create GitHub Release with assets
   - Publish to PyPI

## Versioning Rules
- Breaking MKQL syntax change → MAJOR
- New query function or CLI command → MINOR
- Bug fix, performance improvement → PATCH
- Pre-release: `-alpha.N`, `-beta.N`, `-rc.N`

## Changelog Format (Keep a Changelog)
```
## [version] - YYYY-MM-DD
### Added
### Changed
### Fixed
### Removed
```
```

---

## 1.4 Skills

### `.claude/skills/mkql-grammar/SKILL.md`

```markdown
---
name: mkql-grammar
description: >
  MKQL query language grammar and parser development. Use when writing
  or modifying the pest grammar, adding new query functions, extending
  the parser, or debugging query compilation. Covers SELECT, WHERE,
  LINK, NEAR, FRESH, temporal functions, and context assembly.
---
# MKQL Grammar Skill

## Grammar Location
`crates/mkb-parser/src/mkql.pest`

## Parser Implementation
`crates/mkb-parser/src/lib.rs`

## Key Design Rules
1. MKQL compiles to SQLite SQL + vector index queries
2. Every new function needs: grammar rule, AST node, compiler target
3. Temporal functions (FRESH, STALE, EXPIRED, CURRENT, AS OF) operate
   on `observed_at` field, NOT `_modified_at`
4. NEAR() generates vector similarity query against HNSW index
5. LINKED() generates recursive CTE or multi-join against links table
6. CONTEXT WINDOW triggers token-budgeted result assembly

## Test Pattern
For every grammar change:
1. Add parse test in `crates/mkb-parser/tests/`
2. Add compilation test in `crates/mkb-query/tests/`
3. Add integration test with real SQLite in `tests/rust/`

## Reference
See `docs/MKB_TECHNICAL_SPECIFICATION.md` section 5 for full grammar EBNF.
See `docs/MKB_TEMPORAL_LAYER.md` section 5 for temporal extensions.
```

### `.claude/skills/temporal-enforcement/SKILL.md`

```markdown
---
name: temporal-enforcement
description: >
  Temporal layer enforcement and validation. Use when working on the
  temporal gate, decay model, timestamp extraction, staleness sweep,
  or any code path that creates/modifies vault documents. Critical
  for maintaining the observed_at invariant.
---
# Temporal Enforcement Skill

## Core Invariant
NO document enters the vault without `observed_at`. This is enforced at
the TemporalGate in `crates/mkb-vault/src/temporal_gate.rs` (Rust) and
`python/mkb_ai/temporal/gate.py` (Python ingestion).

## Required Fields (ALL documents)
- `observed_at: datetime` — MANDATORY, rejection if missing
- `valid_until: datetime` — MANDATORY, computed by decay if not provided
- `temporal_precision: enum` — MANDATORY, defaults to "inferred"

## Extraction Priority Chain
1. Source API timestamp (exact)
2. User-provided --observed-at flag (user-specified)
3. File metadata / filename pattern (approximate)
4. AI temporal inference (inferred, confidence penalty -0.15)
5. REJECTION (logged to ingestion/rejected/)

## Decay Model Location
`python/mkb_ai/temporal/decay.py`

## Key Tests
- `test_temporal_gate_rejects_no_timestamp`
- `test_temporal_gate_accepts_explicit_timestamp`
- `test_decay_model_halves_confidence_at_half_life`
- `test_staleness_sweep_archives_expired`
- `test_contradiction_resolver_newer_wins`

## Reference
See `docs/MKB_TEMPORAL_LAYER.md` for complete specification.
```

### `.claude/skills/rust-python-bridge/SKILL.md`

```markdown
---
name: rust-python-bridge
description: >
  PyO3/maturin Rust-Python bridge development. Use when working on the
  mkb-python crate, adding new Python-callable functions, handling type
  conversions across the FFI boundary, or debugging maturin builds.
---
# Rust-Python Bridge Skill

## Bridge Crate
`crates/mkb-python/` — thin translation layer, no business logic.

## PyO3 Patterns
```rust
// Expose Rust function to Python
#[pyfunction]
fn query(mkql: &str) -> PyResult<Vec<PyObject>> { ... }

// Expose Rust struct to Python
#[pyclass]
struct Document { ... }

// Module registration
#[pymodule]
fn _mkb_core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(query, m)?)?;
    m.add_class::<Document>()?;
    Ok(())
}
```

## Build
- `maturin develop --release` — build + install into active venv
- `maturin build --release` — build wheel
- PyO3 abi3-py311 for broad Python compatibility

## Type Mapping
| Rust | Python | Notes |
|------|--------|-------|
| `String` | `str` | Auto-converted |
| `Vec<T>` | `list[T]` | Auto-converted |
| `HashMap<K,V>` | `dict[K,V]` | Auto-converted |
| `Option<T>` | `T | None` | Auto-converted |
| `Result<T, E>` | raises exception | PyO3 converts errors |
| `DateTime<Utc>` | `datetime` | Manual conversion needed |
| custom struct | `@pyclass` | Must derive Clone |

## Rules
1. Keep this crate THIN — translate types, don't compute
2. All heavy lifting in mkb-core, mkb-query, etc.
3. Python calls Rust for: parsing, querying, indexing, vault CRUD
4. Rust calls Python for: LLM inference, embedding generation
5. Use `pyo3-asyncio` for async Python ↔ Rust interop
```

### `.claude/skills/testing-patterns/SKILL.md`

```markdown
---
name: testing-patterns
description: >
  Testing patterns and TDD workflows for MKB. Use when writing tests,
  setting up test fixtures, running test suites, or debugging test
  failures. Covers Rust (cargo test, proptest) and Python (pytest,
  hypothesis) testing patterns.
---
# Testing Patterns Skill

## Test Hierarchy
1. **Unit tests** — single function/method, inline in source files (Rust)
   or `tests/unit/` (Python)
2. **Integration tests** — cross-module, `tests/rust/` and `tests/python/`
3. **E2E tests** — full CLI invocation, `tests/e2e/`
4. **Property tests** — `proptest` (Rust), `hypothesis` (Python)

## Rust Test Patterns
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporal_gate_rejects_missing_observed_at() {
        let doc = DocumentBuilder::new()
            .type_name("project")
            .title("Test Project")
            // deliberately no observed_at
            .build();

        let gate = TemporalGate::default();
        let result = gate.validate(&doc);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("observed_at"));
    }
}
```

## Python Test Patterns
```python
import pytest
from mkb_ai.temporal.gate import TemporalGate

class TestTemporalGate:
    @pytest.fixture
    def gate(self):
        return TemporalGate(confidence_threshold=0.5)

    def test_rejects_missing_observed_at(self, gate):
        doc = {"type": "project", "title": "Test"}
        with pytest.raises(TemporalRejectionError, match="observed_at"):
            gate.validate(doc)
```

## Fixture Management
- Shared fixtures in `fixtures/` directory
- Rust: load via `include_str!("../../fixtures/sample_project.md")`
- Python: load via `pytest.fixture` + `pathlib.Path`
- SQLite test DBs: in-memory (`:memory:`) for speed

## Coverage Targets
- Rust: 80%+ line coverage (`cargo tarpaulin`)
- Python: 85%+ line coverage (`pytest-cov`)
- Critical paths (temporal gate, query engine): 95%+
```

---

## 1.5 Slash Commands

### `.claude/commands/implement.md`

```markdown
---
description: Implement a feature using strict TDD cycle
---
Implement the following feature using the @tdd-driver agent.

STRICT TDD PROCESS:
1. Read the relevant spec section from docs/
2. Write a FAILING test first
3. Run the test — confirm it fails
4. Write MINIMAL code to pass
5. Run the test — confirm it passes
6. Refactor while green
7. Run ALL tests — confirm nothing broke
8. Commit with conventional commit message

Feature to implement: $ARGUMENTS
```

### `.claude/commands/audit.md`

```markdown
---
description: Run QA audit on a component
---
Run a thorough QA audit using the @qa-auditor agent on: $ARGUMENTS

Include:
1. Code quality check
2. Test coverage analysis (run `cargo tarpaulin` and `pytest --cov`)
3. Security review
4. Performance review
5. Temporal invariant verification
```

### `.claude/commands/release.md`

```markdown
---
description: Prepare a new release
---
Prepare a release using the @release-engineer agent.

Version: $ARGUMENTS

Steps:
1. Verify main branch CI is green
2. Run full audit: /audit all
3. Bump version numbers
4. Generate changelog
5. Create release PR
```

---

# PART 2: REPOSITORY INFRASTRUCTURE

---

## 2.1 Cargo Workspace Configuration

`Cargo.toml` (workspace root):

```toml
[workspace]
resolver = "2"
members = [
    "crates/mkb-core",
    "crates/mkb-parser",
    "crates/mkb-index",
    "crates/mkb-vault",
    "crates/mkb-query",
    "crates/mkb-cli",
    "crates/mkb-python",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.82"
license = "Apache-2.0"
repository = "https://github.com/viktorsmkb/mkb"
description = "Markdown Knowledge Base for LLMs"

[workspace.dependencies]
# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
toml = "0.8"

# Database
rusqlite = { version = "0.32", features = ["bundled", "fts5"] }

# CLI
clap = { version = "4", features = ["derive"] }

# Async
tokio = { version = "1", features = ["full"] }

# Error handling
thiserror = "2"
anyhow = "1"

# Parsing
pest = "2.7"
pest_derive = "2.7"

# Time
chrono = { version = "0.4", features = ["serde"] }

# File watching
notify = "7"

# Hashing
sha2 = "0.10"

# Testing
proptest = "1"
tempfile = "3"

# Python bridge
pyo3 = { version = "0.28", features = ["abi3-py311"] }

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"
```

## 2.2 Python Configuration

`pyproject.toml`:

```toml
[build-system]
requires = ["maturin>=1.7,<2.0"]
build-backend = "maturin"

[project]
name = "mkb"
version = "0.1.0"
description = "Markdown Knowledge Base for LLMs"
requires-python = ">=3.11"
license = "Apache-2.0"
dependencies = [
    "anthropic>=0.40",
    "openai>=1.50",
    "pydantic>=2.9",
    "pyyaml>=6.0",
    "httpx>=0.27",
    "numpy>=1.26",
    "hnswlib>=0.8",
    "tiktoken>=0.8",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.0",
    "pytest-asyncio>=0.24",
    "pytest-cov>=5.0",
    "hypothesis>=6.100",
    "mypy>=1.12",
    "ruff>=0.8",
    "pip-audit>=2.7",
]

[tool.maturin]
features = ["pyo3/extension-module"]
python-source = "python"
module-name = "mkb._mkb_core"

[tool.ruff]
target-version = "py311"
line-length = 99
[tool.ruff.lint]
select = ["E", "F", "I", "N", "UP", "ANN", "B", "A", "SIM", "TCH"]

[tool.mypy]
python_version = "3.11"
strict = true
warn_return_any = true
warn_unused_configs = true

[tool.pytest.ini_options]
testpaths = ["tests/python", "tests/e2e"]
asyncio_mode = "auto"
```

## 2.3 Justfile (Task Runner)

```makefile
# justfile — MKB task runner

default: check

# === Build ===
build:
    cargo build --workspace
    maturin develop --release

build-release:
    cargo build --workspace --release
    maturin build --release

# === Test ===
test: test-rust test-python

test-rust:
    cargo test --workspace

test-python:
    uv run pytest tests/python/ -v

test-e2e:
    uv run pytest tests/e2e/ -v --timeout=60

test-integration: test-rust test-python test-e2e

test-coverage:
    cargo tarpaulin --workspace --out html
    uv run pytest tests/ --cov=mkb_ai --cov-report=html

# === Lint ===
lint: lint-rust lint-python

lint-rust:
    cargo clippy --workspace -- -D warnings
    cargo fmt --check

lint-python:
    uv run ruff check python/ tests/
    uv run mypy python/

# === Format ===
fmt:
    cargo fmt
    uv run ruff format python/ tests/

# === Security ===
audit:
    cargo audit
    uv run pip-audit

# === CI (full check) ===
ci: lint test audit
    @echo "✅ All CI checks passed"

# === Release ===
release version:
    @echo "Releasing v{{version}}"
    cargo set-version {{version}}
    git add -A
    git commit -m "chore: bump version to {{version}}"
    git tag -a v{{version}} -m "Release v{{version}}"
    git push origin main --tags
```

---

# PART 3: GITHUB ACTIONS CI/CD

---

## 3.1 CI Pipeline (`.github/workflows/ci.yml`)

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  # === Rust Checks ===
  rust-check:
    name: Rust ${{ matrix.check }}
    runs-on: ubuntu-latest
    strategy:
      matrix:
        check: [fmt, clippy, test, doc]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2

      - name: Check formatting
        if: matrix.check == 'fmt'
        run: cargo fmt --all --check

      - name: Run clippy
        if: matrix.check == 'clippy'
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: Run tests
        if: matrix.check == 'test'
        run: cargo test --workspace --verbose

      - name: Build docs
        if: matrix.check == 'doc'
        run: cargo doc --workspace --no-deps
        env:
          RUSTDOCFLAGS: "-D warnings"

  # === Rust Coverage ===
  rust-coverage:
    name: Rust Coverage
    runs-on: ubuntu-latest
    needs: rust-check
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install tarpaulin
        run: cargo install cargo-tarpaulin
      - name: Generate coverage
        run: cargo tarpaulin --workspace --out xml --output-dir coverage/
      - name: Upload coverage
        uses: codecov/codecov-action@v4
        with:
          files: coverage/cobertura.xml
          flags: rust
          token: ${{ secrets.CODECOV_TOKEN }}

  # === Python Checks ===
  python-check:
    name: Python ${{ matrix.check }}
    runs-on: ubuntu-latest
    strategy:
      matrix:
        check: [lint, typecheck, test]
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/setup-uv@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2

      - name: Install dependencies
        run: uv sync --all-extras

      - name: Build Rust extension
        run: uv run maturin develop --release

      - name: Lint
        if: matrix.check == 'lint'
        run: uv run ruff check python/ tests/

      - name: Type check
        if: matrix.check == 'typecheck'
        run: uv run mypy python/

      - name: Test
        if: matrix.check == 'test'
        run: uv run pytest tests/python/ -v --cov=mkb_ai --cov-report=xml

      - name: Upload coverage
        if: matrix.check == 'test'
        uses: codecov/codecov-action@v4
        with:
          files: coverage.xml
          flags: python
          token: ${{ secrets.CODECOV_TOKEN }}

  # === E2E Tests ===
  e2e:
    name: E2E Tests
    runs-on: ubuntu-latest
    needs: [rust-check, python-check]
    steps:
      - uses: actions/checkout@v4
      - uses: astral-sh/setup-uv@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Build
        run: |
          cargo build --workspace --release
          uv sync --all-extras
          uv run maturin develop --release
      - name: Run E2E
        run: uv run pytest tests/e2e/ -v --timeout=120

  # === Security Audit ===
  security:
    name: Security Audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - name: Cargo audit
        run: |
          cargo install cargo-audit
          cargo audit
      - uses: astral-sh/setup-uv@v4
      - name: Pip audit
        run: |
          uv sync
          uv run pip-audit
```

## 3.2 Release Pipeline (`.github/workflows/release.yml`)

```yaml
name: Release

on:
  push:
    tags:
      - 'v[0-9]+.[0-9]+.[0-9]+'
      - 'v[0-9]+.[0-9]+.[0-9]+-*'

permissions:
  contents: write

jobs:
  # === Create GitHub Release ===
  create-release:
    name: Create Release
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: taiki-e/create-gh-release-action@v1
        with:
          changelog: CHANGELOG.md
          token: ${{ secrets.GITHUB_TOKEN }}

  # === Build Rust Binaries (cross-platform) ===
  build-binaries:
    name: Build ${{ matrix.target }}
    needs: create-release
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - target: x86_64-unknown-linux-gnu
            os: ubuntu-latest
          - target: x86_64-unknown-linux-musl
            os: ubuntu-latest
          - target: aarch64-unknown-linux-gnu
            os: ubuntu-latest
          - target: x86_64-apple-darwin
            os: macos-latest
          - target: aarch64-apple-darwin
            os: macos-latest
          - target: x86_64-pc-windows-msvc
            os: windows-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@v2
        with:
          key: release-${{ matrix.target }}

      - name: Install cross (Linux ARM)
        if: matrix.target == 'aarch64-unknown-linux-gnu'
        run: cargo install cross --git https://github.com/cross-rs/cross

      - name: Build
        run: |
          if [ "${{ matrix.target }}" = "aarch64-unknown-linux-gnu" ]; then
            cross build --release --target ${{ matrix.target }} -p mkb-cli
          else
            cargo build --release --target ${{ matrix.target }} -p mkb-cli
          fi
        shell: bash

      - uses: taiki-e/upload-rust-binary-action@v1
        with:
          bin: mkb
          target: ${{ matrix.target }}
          tar: all
          zip: windows
          token: ${{ secrets.GITHUB_TOKEN }}

  # === Build Python Wheels (maturin) ===
  build-wheels:
    name: Wheels ${{ matrix.target }}
    needs: create-release
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64
          - os: ubuntu-latest
            target: aarch64
          - os: macos-latest
            target: x86_64
          - os: macos-latest
            target: aarch64
          - os: windows-latest
            target: x86_64
    steps:
      - uses: actions/checkout@v4

      - name: Setup QEMU (Linux ARM)
        if: matrix.target == 'aarch64' && runner.os == 'Linux'
        uses: docker/setup-qemu-action@v3
        with:
          platforms: arm64

      - name: Build wheels
        uses: PyO3/maturin-action@v1
        with:
          target: ${{ matrix.target }}
          args: --release --out dist
          manylinux: auto

      - name: Upload wheels
        uses: actions/upload-artifact@v4
        with:
          name: wheels-${{ matrix.os }}-${{ matrix.target }}
          path: dist/

  # === Publish to PyPI ===
  publish-pypi:
    name: Publish to PyPI
    needs: build-wheels
    runs-on: ubuntu-latest
    if: "!contains(github.ref, '-')"  # Skip pre-releases
    steps:
      - name: Download wheels
        uses: actions/download-artifact@v4
        with:
          pattern: wheels-*
          merge-multiple: true
          path: dist/
      - name: Publish
        uses: pypa/gh-action-pypi-publish@release/v1
        with:
          password: ${{ secrets.PYPI_TOKEN }}
```

---

# PART 4: IMPLEMENTATION TASK LIST

---

## Phase 0: Repository Bootstrap (Week 0)

```
TASK-000: Repository initialization
├── T-000.1: Create GitHub repository with Apache-2.0 license
├── T-000.2: Initialize Cargo workspace with all crate stubs
├── T-000.3: Initialize pyproject.toml with maturin backend
├── T-000.4: Create justfile with all task definitions
├── T-000.5: Create .claude/ directory with all agents, skills, commands
├── T-000.6: Create CLAUDE.md with full project context
├── T-000.7: Set up .github/workflows/ci.yml
├── T-000.8: Set up .github/workflows/release.yml
├── T-000.9: Create .gitignore (Rust + Python + IDE)
├── T-000.10: Create CHANGELOG.md (Keep a Changelog format)
├── T-000.11: Create docs/ directory with all spec files
├── T-000.12: Create fixtures/ directory with sample markdown files
├── T-000.13: Set up Codecov integration
├── T-000.14: Configure Dependabot for Rust + Python
├── T-000.15: Create CONTRIBUTING.md with dev setup instructions
├── T-000.16: Verify `just ci` passes on empty workspace
└── GATE: CI pipeline green, all stubs compile, `just ci` passes
```

## Phase 1: Core Types & Schema System (Weeks 1–2)

```
TASK-100: mkb-core crate — foundational types
├── T-100.1: Define Document struct with all system fields
│   ├── Test: document_builder_creates_valid_document
│   ├── Test: document_requires_observed_at
│   └── Test: document_serializes_to_yaml_frontmatter
├── T-100.2: Define temporal types (ObservedAt, ValidUntil, TemporalPrecision)
│   ├── Test: temporal_precision_ordering
│   ├── Test: observed_at_rejects_future_dates
│   └── Test: valid_until_must_be_after_observed_at
├── T-100.3: Define Link type and relationship model
│   ├── Test: link_creation_with_timestamp
│   └── Test: link_serialization_roundtrip
├── T-100.4: Define SchemaDefinition types (FieldDef, FieldType, Validation)
│   ├── Test: schema_loads_from_yaml
│   ├── Test: schema_validates_required_fields
│   └── Test: schema_validates_field_types
├── T-100.5: Define error types hierarchy (MkbError, TemporalError, SchemaError)
│   └── Test: errors_display_human_readable_messages
├── T-100.6: Implement YAML frontmatter parser (read + write)
│   ├── Test: parse_frontmatter_from_markdown
│   ├── Test: write_frontmatter_to_markdown
│   ├── Test: roundtrip_preserves_all_fields
│   └── Test: rejects_yaml_without_observed_at
├── T-100.7: Implement schema validation engine
│   ├── Test: validate_project_document_against_schema
│   ├── Test: validate_rejects_missing_required_field
│   ├── Test: validate_rejects_wrong_type
│   └── Test: validate_enum_field_against_allowed_values
├── T-100.8: Create base schemas (_base.yaml, project, person, decision,
│            meeting, signal, concept) in schemas/ directory
│   └── Test: all_built_in_schemas_parse_successfully
└── GATE: `cargo test -p mkb-core` — all green, 95%+ coverage
```

```
TASK-110: Temporal gate and decay model
├── T-110.1: Implement TemporalGate with hard rejection
│   ├── Test: gate_rejects_null_observed_at
│   ├── Test: gate_rejects_null_temporal_precision
│   ├── Test: gate_accepts_complete_temporal_fields
│   ├── Test: gate_computes_valid_until_from_decay_profile
│   └── Test: gate_returns_rejection_with_suggestion
├── T-110.2: Implement DecayModel with configurable profiles
│   ├── Test: exponential_decay_halves_at_half_life
│   ├── Test: project_status_decays_in_14_days
│   ├── Test: decision_never_decays
│   ├── Test: signal_decays_in_7_days
│   ├── Test: lower_precision_accelerates_decay
│   └── Test: effective_confidence_floors_at_zero
├── T-110.3: Implement ContradictionResolver
│   ├── Test: newer_observation_supersedes_older
│   ├── Test: same_timestamp_higher_confidence_wins
│   └── Test: resolver_creates_supersession_chain
├── T-110.4: Implement RejectionLog with recovery support
│   ├── Test: rejected_doc_written_to_rejected_dir
│   ├── Test: rejection_includes_extraction_attempts
│   └── Test: recovery_re_ingests_with_provided_timestamp
└── GATE: Temporal invariant enforced and tested at 95%+ coverage
```

## Phase 2: Storage & Indexing (Weeks 2–3)

```
TASK-200: mkb-vault crate — file system operations
├── T-200.1: Implement Vault struct with init/open
│   ├── Test: init_creates_directory_structure
│   ├── Test: open_existing_vault
│   └── Test: open_nonexistent_vault_returns_error
├── T-200.2: Implement document CRUD (create, read, update, delete)
│   ├── Test: create_document_writes_markdown_file
│   ├── Test: create_assigns_unique_id
│   ├── Test: read_document_parses_frontmatter_and_body
│   ├── Test: update_document_preserves_created_at
│   ├── Test: update_bumps_modified_at
│   ├── Test: delete_soft_moves_to_archive
│   ├── Test: delete_hard_removes_file
│   └── Test: create_rejects_document_without_observed_at (gate integration)
├── T-200.3: Implement file path resolution and naming
│   ├── Test: type_determines_subdirectory
│   ├── Test: slug_generated_from_title
│   └── Test: collision_appends_counter
└── GATE: Full CRUD with temporal gate, all tests green
```

```
TASK-210: mkb-index crate — SQLite indexing
├── T-210.1: Implement IndexManager with SQLite setup
│   ├── Test: creates_schema_on_init
│   └── Test: handles_concurrent_access
├── T-210.2: Implement document indexing (EAV pattern)
│   ├── Test: index_document_stores_all_frontmatter_fields
│   ├── Test: index_handles_array_fields (tags, refs)
│   ├── Test: index_stores_temporal_fields
│   ├── Test: reindex_updates_changed_fields
│   └── Test: delete_from_index_removes_all_entries
├── T-210.3: Implement FTS5 full-text indexing
│   ├── Test: fts_indexes_title_and_body
│   ├── Test: fts_search_returns_ranked_results
│   ├── Test: fts_handles_unicode_content
│   └── Test: fts_porter_stemming_works
├── T-210.4: Implement link indexing
│   ├── Test: store_and_retrieve_links
│   ├── Test: query_forward_links (source → targets)
│   └── Test: query_reverse_links (target ← sources)
├── T-210.5: Implement temporal indexes and queries
│   ├── Test: query_by_observed_at_range
│   ├── Test: query_current_documents (not superseded, not expired)
│   ├── Test: query_with_effective_confidence
│   └── Test: staleness_sweep_marks_expired
├── T-210.6: Implement full rebuild from vault files
│   ├── Test: rebuild_matches_incremental_index
│   └── Test: rebuild_handles_corrupt_files_gracefully
└── GATE: All index operations tested, EXPLAIN shows index usage
```

## Phase 3: MKQL Parser & Query Engine (Weeks 3–5)

```
TASK-300: mkb-parser crate — MKQL grammar
├── T-300.1: Write pest grammar for SELECT statements
│   ├── Test: parse_select_star_from_type
│   ├── Test: parse_select_specific_fields
│   ├── Test: parse_select_with_alias
│   └── Test: parse_select_with_aggregates
├── T-300.2: Write pest grammar for WHERE clauses
│   ├── Test: parse_equality_predicate
│   ├── Test: parse_comparison_operators
│   ├── Test: parse_in_list
│   ├── Test: parse_like_pattern
│   ├── Test: parse_and_or_combinations
│   ├── Test: parse_nested_parentheses
│   └── Test: parse_body_contains
├── T-300.3: Write pest grammar for temporal functions
│   ├── Test: parse_fresh_duration
│   ├── Test: parse_stale_and_expired
│   ├── Test: parse_current_and_latest
│   ├── Test: parse_during_range
│   ├── Test: parse_as_of_datetime
│   ├── Test: parse_history
│   ├── Test: parse_age_function
│   └── Test: parse_eff_confidence
├── T-300.4: Write pest grammar for NEAR() and LINKED()
│   ├── Test: parse_near_with_threshold
│   ├── Test: parse_linked_forward
│   ├── Test: parse_linked_reverse
│   └── Test: parse_linked_with_filter
├── T-300.5: Write pest grammar for ORDER BY, LIMIT, OFFSET, GROUP BY
│   ├── Test: parse_order_by_multiple_fields
│   └── Test: parse_group_by_with_having
├── T-300.6: Write pest grammar for CONTEXT clause
│   ├── Test: parse_context_window
│   └── Test: parse_context_format
├── T-300.7: Build AST types and parser → AST transformation
│   ├── Test: ast_roundtrip_simple_query
│   ├── Test: ast_roundtrip_complex_query
│   └── Test: parser_error_messages_are_helpful
├── T-300.8: Property tests for grammar robustness
│   ├── Test: proptest_valid_queries_parse
│   └── Test: proptest_random_strings_dont_panic
└── GATE: All MKQL syntax from spec parses correctly
```

```
TASK-310: mkb-query crate — query compilation & execution
├── T-310.1: Implement MKQL → SQL compiler for field predicates
│   ├── Test: compile_equality_to_sql
│   ├── Test: compile_in_list_to_sql
│   ├── Test: compile_body_contains_to_fts5
│   └── Test: compile_parameterizes_values (no SQL injection)
├── T-310.2: Implement temporal function compilation
│   ├── Test: compile_fresh_to_observed_at_range
│   ├── Test: compile_current_excludes_superseded_and_expired
│   ├── Test: compile_eff_confidence_with_decay
│   └── Test: compile_as_of_to_version_lookup
├── T-310.3: Implement LINK clause compilation (joins/CTEs)
│   ├── Test: compile_forward_link_to_join
│   ├── Test: compile_reverse_link_to_join
│   └── Test: compile_multi_hop_to_recursive_cte
├── T-310.4: Implement aggregation compilation
│   ├── Test: compile_count_group_by
│   └── Test: compile_avg_with_filter
├── T-310.5: Implement QueryPlanner with cost estimation
│   ├── Test: planner_chooses_index_scan_over_full_scan
│   └── Test: planner_combines_fts_and_field_predicates
├── T-310.6: Implement ResultFormatter (JSON, Table, Markdown, Context)
│   ├── Test: format_as_json
│   ├── Test: format_as_table
│   ├── Test: format_as_context_with_token_budget
│   └── Test: format_context_respects_window_limit
├── T-310.7: Implement ContextAssembler for LLM context windows
│   ├── Test: assembler_prioritizes_high_confidence_fresh_docs
│   ├── Test: assembler_respects_token_budget
│   └── Test: assembler_falls_back_to_summary_format
└── GATE: Full query compilation + execution, tested against live SQLite
```

## Phase 4: CLI (Weeks 5–6)

```
TASK-400: mkb-cli crate
├── T-400.1: Implement `mkb init` command
│   ├── E2E: init_creates_vault_structure
│   └── E2E: init_with_custom_config
├── T-400.2: Implement `mkb add` command
│   ├── E2E: add_project_interactively
│   ├── E2E: add_from_file
│   ├── E2E: add_rejects_without_timestamp
│   └── E2E: add_with_observed_at_flag
├── T-400.3: Implement `mkb query` / `mkb q` command
│   ├── E2E: query_basic_select
│   ├── E2E: query_with_format_flag
│   ├── E2E: query_temporal_functions
│   └── E2E: query_pipe_to_stdout
├── T-400.4: Implement `mkb search` / `mkb s` command
│   ├── E2E: search_fulltext
│   └── E2E: search_semantic (requires embeddings)
├── T-400.5: Implement `mkb edit` and `mkb rm` commands
│   ├── E2E: edit_updates_fields
│   └── E2E: rm_soft_delete_and_hard_delete
├── T-400.6: Implement `mkb link` command
│   ├── E2E: link_create_relationship
│   └── E2E: link_list_relationships
├── T-400.7: Implement `mkb schema` command
│   ├── E2E: schema_list
│   └── E2E: schema_validate
├── T-400.8: Implement `mkb gc` command
│   ├── E2E: gc_sweep_stale
│   ├── E2E: gc_stale_report
│   └── E2E: gc_find_contradictions
├── T-400.9: Implement `mkb repl` (interactive query shell)
│   ├── E2E: repl_executes_query
│   └── E2E: repl_dot_commands
├── T-400.10: Implement `mkb stats` command
│   └── E2E: stats_shows_vault_summary
└── GATE: All CLI commands work end-to-end, tested in CI
```

## Phase 5: Python AI Layer (Weeks 6–8)

```
TASK-500: PyO3 bridge — mkb-python crate
├── T-500.1: Expose vault CRUD to Python
│   ├── Test: python_can_create_document
│   ├── Test: python_can_query_mkql
│   └── Test: python_receives_temporal_validation_errors
├── T-500.2: Expose index operations to Python
│   └── Test: python_can_search_fts
├── T-500.3: Expose temporal gate to Python
│   └── Test: python_gate_rejects_missing_observed_at
└── GATE: maturin develop builds successfully, Python imports work
```

```
TASK-510: AI ingestion pipeline (Python)
├── T-510.1: Implement source adapter framework
│   ├── Test: file_adapter_reads_markdown
│   ├── Test: csv_adapter_extracts_rows
│   └── Test: adapter_normalizes_to_common_format
├── T-510.2: Implement explicit extractor (NER, regex, dates)
│   ├── Test: extract_jira_ticket_ids
│   ├── Test: extract_dates_from_text
│   ├── Test: extract_person_mentions
│   └── Test: extract_metrics_with_context
├── T-510.3: Implement temporal extractor (Priority 4: AI inference)
│   ├── Test: infer_timestamp_from_relative_date
│   ├── Test: infer_timestamp_from_context
│   ├── Test: reject_when_no_timestamp_inferable
│   └── Test: temporal_precision_set_to_inferred
├── T-510.4: Implement implicit extractor (LLM-powered)
│   ├── Test: extract_sentiment_signal
│   ├── Test: extract_risk_signal
│   ├── Test: extract_capacity_signal
│   └── Test: confidence_reduced_for_implicit
├── T-510.5: Implement confidence scorer
│   ├── Test: score_human_authored_document
│   ├── Test: score_ai_inferred_signal
│   └── Test: score_with_corroboration_boost
├── T-510.6: Implement dedup and merge logic
│   ├── Test: skip_exact_hash_match
│   ├── Test: update_same_source_different_hash
│   └── Test: merge_different_source_same_entity
├── T-510.7: Implement `mkb ingest` CLI integration
│   ├── E2E: ingest_file_with_ai_enrichment
│   ├── E2E: ingest_directory_batch
│   ├── E2E: ingest_rejects_undated_by_default
│   └── E2E: ingest_dry_run
└── GATE: Full ingestion pipeline works end-to-end with LLM calls
```

```
TASK-520: Embedding and vector search
├── T-520.1: Implement embedding generation (OpenAI/local)
│   ├── Test: generate_embedding_for_document
│   └── Test: embedding_dimensions_match_config
├── T-520.2: Implement HNSW index management
│   ├── Test: add_and_query_vectors
│   └── Test: persist_and_reload_index
├── T-520.3: Integrate NEAR() with vector index
│   ├── Test: near_query_returns_similar_documents
│   └── Test: near_combined_with_field_filter
├── T-520.4: Implement `mkb search --semantic`
│   └── E2E: semantic_search_finds_relevant_docs
└── GATE: Semantic search works end-to-end
```

## Phase 6: Integration, Polish & Release (Weeks 9–10)

```
TASK-600: Source adapters (Jira, Slack, Google Docs)
├── T-600.1: Jira REST API adapter
│   ├── Test: jira_adapter_extracts_issue_fields
│   ├── Test: jira_timestamp_maps_to_observed_at
│   └── Test: jira_transform_creates_project_doc
├── T-600.2: Google Docs adapter
│   ├── Test: gdocs_adapter_reads_document
│   └── Test: gdocs_modified_time_maps_to_observed_at
├── T-600.3: CSV/XLSX adapter
│   ├── Test: csv_auto_detects_date_column
│   └── Test: csv_rejects_when_no_date_found
└── GATE: At least 3 adapters working, temporally grounded
```

```
TASK-610: Views, hooks, and automation
├── T-610.1: Implement saved views (YAML definitions)
│   ├── Test: load_view_definition
│   └── Test: execute_view_queries
├── T-610.2: Implement file watcher for auto-indexing
│   ├── Test: file_created_triggers_index
│   ├── Test: file_modified_triggers_reindex
│   └── Test: file_deleted_triggers_deindex
├── T-610.3: Implement `mkb serve` HTTP API
│   ├── Test: api_query_endpoint
│   └── Test: api_ingest_endpoint
└── GATE: Views and automation working
```

```
TASK-620: Release preparation
├── T-620.1: Full QA audit (/audit all)
├── T-620.2: Version bump to 0.1.0
├── T-620.3: Generate CHANGELOG.md
├── T-620.4: Verify cross-platform binaries build
│   ├── Test: linux-x86_64 binary runs
│   ├── Test: macos-arm64 binary runs
│   └── Test: windows-x86_64 binary runs
├── T-620.5: Verify Python wheels build
│   ├── Test: wheel installs in clean venv
│   └── Test: `import mkb` works
├── T-620.6: Create v0.1.0 tag and push
├── T-620.7: Verify GitHub Release created with all assets
├── T-620.8: Verify PyPI publication (if enabled)
└── GATE: v0.1.0 released, binaries downloadable, pip installable
```

---

# PART 5: QUALITY GATES & DEFINITION OF DONE

---

## Per-Task Definition of Done

Every task (T-xxx.x) is complete when:

1. ✅ **Tests written FIRST** — failing test exists before implementation
2. ✅ **Tests pass** — `cargo test` / `pytest` green
3. ✅ **No warnings** — `clippy -D warnings` / `ruff` / `mypy` clean
4. ✅ **Formatted** — `cargo fmt` / `ruff format` applied
5. ✅ **Documented** — public API has doc comments
6. ✅ **Committed** — conventional commit message
7. ✅ **Temporal invariant** — any data path creating docs enforces `observed_at`

## Per-Phase Gates

| Phase | Gate Criteria |
|-------|--------------|
| 0 | CI pipeline green, all stubs compile |
| 1 | Core types + schemas + temporal gate at 95% coverage |
| 2 | Full CRUD + indexing, SQLite EXPLAIN shows index usage |
| 3 | All MKQL syntax from spec parses + executes correctly |
| 4 | All CLI commands work E2E, tested in CI |
| 5 | AI ingestion pipeline works E2E with LLM calls |
| 6 | Cross-platform binaries + wheels build, v0.1.0 released |

## Release Criteria for v0.1.0

- [ ] All Phase 0–6 gates passed
- [ ] Rust coverage ≥ 80%
- [ ] Python coverage ≥ 85%
- [ ] Zero `cargo audit` / `pip-audit` findings above WARN
- [ ] CLI `mkb --help` shows all commands
- [ ] `mkb init && mkb add project --title "Test" --observed-at 2025-02-10` works
- [ ] `mkb q 'SELECT * FROM project WHERE CURRENT()'` returns results
- [ ] `mkb ingest file notes.md --infer` processes with AI enrichment
- [ ] Cross-platform binaries: Linux x86, Linux ARM, macOS x86, macOS ARM, Windows
- [ ] Python wheel: `pip install mkb && python -c "import mkb"` works
- [ ] CHANGELOG.md complete
- [ ] README.md with quickstart guide

---

# PART 6: AGENT EXECUTION PROTOCOL

---

## How to Use This Document

This is the **system prompt and task backlog** for AI-driven implementation of MKB.
The intended execution environment is **Claude Code** with the subagents, skills,
and commands defined in Part 1.

### Execution Order

```
1. /implement T-000.1 through T-000.16  (repo bootstrap)
2. /implement T-100.1 through T-100.8   (core types)
3. /implement T-110.1 through T-110.4   (temporal)
4. Continue sequentially through T-200 → T-620
5. At each GATE: /audit <phase>
6. At final gate: /release 0.1.0
```

### Parallelization Opportunities

These task groups can execute in parallel:

| Track A (Rust core) | Track B (Python AI) | Track C (Infra) |
|---------------------|---------------------|------------------|
| T-100 Core types | — | T-000 Bootstrap |
| T-110 Temporal | — | CI/CD setup |
| T-200 Vault | — | — |
| T-210 Indexing | — | — |
| T-300 Parser | — | — |
| T-310 Query engine | T-500 PyO3 bridge | — |
| T-400 CLI | T-510 AI pipeline | T-600 Adapters |
| — | T-520 Embeddings | T-610 Automation |
| — | — | T-620 Release |

### Subagent Delegation Map

| Task Range | Primary Agent | Supporting Agent |
|-----------|--------------|-----------------|
| T-000 | Main Claude | — |
| T-100, T-110 | @tdd-driver | @architect |
| T-200, T-210 | @tdd-driver | @architect |
| T-300, T-310 | @tdd-driver | @architect |
| T-400 | @tdd-driver | @qa-auditor |
| T-500–T-520 | @tdd-driver | @architect |
| T-600, T-610 | @tdd-driver | — |
| T-620 | @release-engineer | @qa-auditor |
| Gates | @qa-auditor | — |
