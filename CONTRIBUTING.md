# Contributing to MKB

## Development Setup

### Prerequisites

- **Rust** 1.82+ (install via [rustup](https://rustup.rs/))
- **Python** 3.11+ (install via [uv](https://docs.astral.sh/uv/))
- **just** (install via `cargo install just` or `brew install just`)
- **maturin** (install via `pip install maturin` or `uv tool install maturin`)

### Quick Start

```bash
# Clone the repository
git clone https://github.com/vbezdek/mkb.git
cd mkb

# Install Python dependencies
uv sync --all-extras

# Build everything
just build

# Run all checks
just ci
```

### Development Workflow

1. **Create a feature branch** from `main`
2. **Write tests FIRST** (TDD — see below)
3. **Implement** the minimal code to pass
4. **Refactor** while tests are green
5. **Format** before committing: `just fmt`
6. **Run CI** locally: `just ci`
7. **Open a PR** against `main`

### TDD Process (Mandatory)

Every change follows the Red-Green-Refactor cycle:

1. **RED**: Write a failing test that describes the desired behavior
2. **GREEN**: Write the minimum code to make it pass
3. **REFACTOR**: Clean up while keeping tests green

### Running Tests

```bash
# All tests
just test

# Rust only
just test-rust

# Python only
just test-python

# E2E tests
just test-e2e

# Coverage report
just test-coverage
```

### Code Style

**Rust:**
- `cargo fmt` for formatting
- `cargo clippy -- -D warnings` for linting
- No `unwrap()` in library code — use `?` or `expect()` with message
- All public APIs have doc comments

**Python:**
- `ruff` for linting and formatting
- `mypy --strict` for type checking
- Type hints on ALL function signatures
- No bare `except:` — always catch specific exceptions

### Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>

Types: feat, fix, refactor, test, docs, chore
Scopes: core, parser, index, vault, query, cli, python, ci
```

### Architecture

See `docs/tech-spec.md` for the full technical specification.

Key principle: **No information enters the vault without `observed_at`.**

### Getting Help

- Read `CLAUDE.md` for project context
- Check `docs/` for specifications and ADRs
- Use `/implement <feature>` to start TDD implementation
- Use `/audit <component>` for quality checks
