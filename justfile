# justfile — MKB task runner

set shell := ["bash", "-cu"]

default: check

# === Build ===
build:
    . "$HOME/.cargo/env" && cargo build --workspace
    maturin develop --release

build-release:
    . "$HOME/.cargo/env" && cargo build --workspace --release
    maturin build --release

# === Test ===
test: test-rust test-python

test-rust:
    . "$HOME/.cargo/env" && cargo test --workspace

test-python:
    uv run pytest tests/python/ -v

test-e2e:
    uv run pytest tests/e2e/ -v --timeout=60

test-integration: test-rust test-python test-e2e

test-coverage:
    . "$HOME/.cargo/env" && cargo tarpaulin --workspace --out html
    uv run pytest tests/ --cov=mkb_ai --cov-report=html

# === Lint ===
check: lint-rust lint-python

lint: lint-rust lint-python

lint-rust:
    . "$HOME/.cargo/env" && cargo clippy --workspace -- -D warnings
    . "$HOME/.cargo/env" && cargo fmt --check

lint-python:
    uv run ruff check python/ tests/
    uv run mypy python/

# === Format ===
fmt:
    . "$HOME/.cargo/env" && cargo fmt
    uv run ruff format python/ tests/

# === Security ===
audit:
    . "$HOME/.cargo/env" && cargo audit
    uv run pip-audit

# === CI (full check) ===
ci: lint test audit
    @echo "All CI checks passed"

# === Release ===
release version:
    @echo "Releasing v{{version}}"
    . "$HOME/.cargo/env" && cargo set-version {{version}}
    git add -A
    git commit -m "chore: bump version to {{version}}"
    git tag -a v{{version}} -m "Release v{{version}}"
    git push origin main --tags
