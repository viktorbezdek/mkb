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
- Rust 1.82+ (2021 edition), Cargo workspace
- Python 3.11+, uv for dependency management
- PyO3 0.23+ with maturin for Rust-Python binding
- SQLite 3.45+ with FTS5 for field index
- pest (PEG parser) for MKQL grammar
- clap 4.x for CLI
- tokio for async runtime
- serde for serialization
- anthropic/openai Python SDKs for LLM calls

## Key Commands
- `. "$HOME/.cargo/env" && cargo build` — build Rust workspace
- `. "$HOME/.cargo/env" && cargo test` — run Rust tests
- `. "$HOME/.cargo/env" && cargo clippy -- -D warnings` — lint Rust
- `. "$HOME/.cargo/env" && cargo fmt --check` — check Rust formatting
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
2. Rust tests: `. "$HOME/.cargo/env" && cargo test -p <crate>`
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
This is a hard gate at the ingestion boundary. See docs/tech-spec.md section 2.5.

## Naming Conventions
- Rust: snake_case for functions/variables, CamelCase for types
- Python: snake_case for functions/variables, CamelCase for classes
- Files: kebab-case for Rust crates, snake_case for Python modules
- Schemas: lowercase with underscores (e.g., `_base.yaml`, `project.yaml`)
- Tests: `test_<module>_<behavior>.rs` / `test_<module>_<behavior>.py`
- Document IDs: `<type>-<slug>-<counter>` (e.g., `proj-alpha-001`)
