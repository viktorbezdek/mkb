# MKB — Markdown Knowledge Base for LLMs

Stop feeding your LLM stale context. MKB is a knowledge base where every fact has a timestamp, confidence decays over time, and you always know what's current.

Your knowledge lives as plain markdown files. MKB indexes them, tracks when each piece of information was observed, and lets you query across time — so your LLM always gets the freshest, most relevant context.

## Why MKB?

**The problem:** You have meeting notes, project docs, decisions, and signals scattered across files. When you ask an LLM for help, it can't tell which information is current, which is outdated, and which contradicts something newer.

**MKB solves this by:**
- Timestamping every piece of knowledge when it enters the system
- Automatically decaying confidence over time — old info gets deprioritized
- Giving you a query language to ask "what's current?" instead of grepping through files
- Exposing everything to LLMs via MCP, so your AI assistant queries your vault directly

## Getting Started

### 1. Install

Requires Rust 1.83+ and Python 3.11+.

```bash
git clone https://github.com/viktorbezdek/mkb.git
cd mkb
pip install -e .
```

### 2. Create a vault

```bash
mkb init my-vault
cd my-vault
```

This creates a `my-vault/` directory with a `.mkb/` folder for the index. Your documents will live as markdown files alongside it.

### 3. Add your first document

```bash
mkb add --doc-type project --title "Website Redesign" \
  --observed-at 2026-02-10T12:00:00Z \
  --body "Migrating to Next.js. Target launch: March 15." \
  --tags "frontend,launch"
```

Every document needs an `observed_at` timestamp. This is the core invariant — MKB won't accept knowledge without knowing *when* it was true.

### 4. Add more context over time

```bash
mkb add --doc-type meeting --title "Design Review" \
  --observed-at 2026-02-12T14:00:00Z \
  --body "Decided to delay launch to April 1. New scope includes dark mode."

mkb add --doc-type decision --title "Dark Mode Priority" \
  --observed-at 2026-02-12T14:30:00Z \
  --body "Dark mode is P1 for launch. Assign to frontend team."
```

### 5. Query what's current

```bash
# What's still relevant right now?
mkb query "SELECT * FROM project WHERE CURRENT()"

# What happened in the last 7 days?
mkb query "SELECT title, observed_at FROM meeting WHERE FRESH('7d')"

# Full-text search across everything
mkb search "dark mode"
```

### 6. Connect it to your LLM

Add MKB as an MCP server so your AI assistant can query your vault:

```json
{
  "mcpServers": {
    "mkb": {
      "command": "mkb",
      "args": ["mcp", "--vault", "/path/to/my-vault"]
    }
  }
}
```

Now your LLM can search your knowledge base, run temporal queries, and always get current context.

## What Can You Do With It?

### Track projects and decisions

```bash
mkb add --doc-type project --title "API Migration" \
  --observed-at 2026-02-10T00:00:00Z \
  --body "Moving from REST to gRPC. Q2 target."

mkb add --doc-type decision --title "Use gRPC-Web for browser clients" \
  --observed-at 2026-02-11T00:00:00Z \
  --body "Avoids proxy layer. Team agreed in arch review."

# Link the decision to the project
mkb link create --source deci-use-grpc-web-001 --target proj-api-migration-001 \
  --rel "decided_for"
```

### Find what's going stale

```bash
# Documents not updated in 30 days — probably outdated
mkb query "SELECT * FROM project WHERE STALE('30d')"

# Clean up expired documents
mkb gc
```

### Bulk import existing notes

```bash
# Ingest a directory of markdown files
mkb ingest ./notes/

# Import a CSV (date columns auto-detected)
mkb ingest ./data/meetings.csv
```

### Visualize how knowledge connects

```bash
# See relationships as a Mermaid diagram
mkb graph --type project --format mermaid

# Explore from a specific document, 2 hops deep
mkb graph --center proj-api-migration-001 --depth 2 --format dot
```

### Save queries you run often

```bash
mkb view save active-projects "SELECT * FROM project WHERE CURRENT() ORDER BY observed_at DESC"
mkb view run active-projects
```

### Semantic search (with embeddings)

```bash
# Search by meaning, not just keywords
mkb search "team velocity concerns" --semantic
```

## The Query Language (MKQL)

MKQL is SQL-like but built for temporal knowledge. The key difference: predicates that understand time.

```sql
-- What's current right now?
SELECT * FROM project WHERE CURRENT()

-- What was observed in the last 7 days?
SELECT title, observed_at FROM meeting WHERE FRESH('7d') ORDER BY observed_at DESC

-- What existed as of January 1st? (time-travel)
SELECT * FROM project WHERE AS_OF('2026-01-01T00:00:00Z')

-- What's gone stale?
SELECT * FROM signal WHERE STALE('30d')

-- High-confidence results only
SELECT * FROM decision WHERE EFF_CONFIDENCE(> 0.7)

-- Semantic similarity combined with filters
SELECT * FROM document WHERE NEAR('machine learning', 0.8) AND status = 'active'

-- Follow relationships
SELECT * FROM decision WHERE LINKED('proj-api-migration-001')
```

### Temporal Predicates

| Predicate | What it does |
|-----------|-------------|
| `CURRENT()` | Not yet expired |
| `FRESH('7d')` | Observed within the duration |
| `STALE('30d')` | Not observed within the duration |
| `EXPIRED()` | Past its expiration date |
| `AS_OF('datetime')` | Time-travel to a point in the past |
| `EFF_CONFIDENCE(> 0.7)` | Confidence after time-decay |
| `NEAR('text', 0.8)` | Vector similarity above threshold |
| `LINKED('doc-id')` | Connected to a document |

## Document Format

Every document is a markdown file with YAML frontmatter. You can create them with the CLI or write them by hand:

```markdown
---
id: proj-website-redesign-001
type: project
title: Website Redesign
observed_at: "2026-02-10T12:00:00Z"
valid_until: "2026-02-24T12:00:00Z"
confidence: 0.95
precision: day
tags:
  - frontend
  - launch
status: active
---

Migrating to Next.js. Target launch: March 15.

## Key Decisions
- Use App Router (not Pages)
- Tailwind for styling
- Vercel for deployment
```

Edit files directly — MKB watches for changes and re-indexes automatically:

```bash
mkb watch
```

## MCP Server

MKB exposes a read-only MCP server so LLM assistants can query your vault directly.

```json
{
  "mcpServers": {
    "mkb": {
      "command": "mkb",
      "args": ["mcp", "--vault", "/path/to/vault"]
    }
  }
}
```

**Available tools:**

| Tool | What it does |
|------|-------------|
| `mkb_query` | Run any MKQL query |
| `mkb_search` | Full-text search |
| `mkb_search_semantic` | Find similar documents by meaning |
| `mkb_get_document` | Read a specific document |
| `mkb_list_types` | See what document types exist |
| `mkb_vault_status` | Check vault health |

**Resource URIs:** `mkb://vault/{type}/{id}` and `mkb://query/{mkql}`

## CLI Reference

| Command | What it does |
|---------|-------------|
| `mkb init [path]` | Create a new vault |
| `mkb add` | Add a document |
| `mkb add --from-file` | Import a markdown file |
| `mkb query <mkql>` | Run an MKQL query |
| `mkb search <text>` | Full-text search |
| `mkb search --semantic` | Semantic similarity search |
| `mkb edit <id>` | Update a document |
| `mkb rm <id>` | Archive a document |
| `mkb link create` | Link two documents |
| `mkb link list <id>` | See a document's links |
| `mkb graph` | Visualize relationships |
| `mkb view save/list/run/delete` | Manage saved queries |
| `mkb watch` | Auto-reindex on changes |
| `mkb mcp` | Start MCP server |
| `mkb ingest <path>` | Bulk import files or CSV |
| `mkb gc` | Clean up stale documents |
| `mkb stats` | Vault statistics |
| `mkb status` | Health check |
| `mkb completions <shell>` | Shell completions (bash/zsh/fish) |

## Accuracy and Performance

Measured on synthetic ground-truth documents with known properties for verifiable precision and recall.

<!-- BENCHMARK_RESULTS_START -->
```
Processing Accuracy

| Metric                      |  100 docs |   1K docs |  10K docs |
|-----------------------------|-----------|-----------|-----------|
| Ingest Accuracy             |    100.0% |    100.0% |    100.0% |
| Field Preservation          |    100.0% |    100.0% |    100.0% |
| Temporal Integrity          |    100.0% |    100.0% |    100.0% |

Retrieval Accuracy

| Metric                      |  100 docs |   1K docs |  10K docs |
|-----------------------------|-----------|-----------|-----------|
| FTS Precision@10            |    100.0% |    100.0% |    100.0% |
| FTS Recall                  |    100.0% |    100.0% |    100.0% |
| MKQL Type Filter (Jaccard)  |    100.0% |    100.0% |    100.0% |
| MKQL CURRENT() Precision    |    100.0% |    100.0% |    100.0% |
| MKQL FRESH('7d') (Jaccard)  |    100.0% |    100.0% |    100.0% |

Performance (Apple Silicon)

| Metric                      |  100 docs |   1K docs |  10K docs |
|-----------------------------|-----------|-----------|-----------|
| Ingest Throughput           |   3.3K/s  |   3.0K/s  |   2.3K/s  |
| FTS Search (p50)            |    29 us  |    36 us  |    25 us  |
| MKQL Query (p50)            |    46 us  |   337 us  |     5 ms  |
| KNN Search (p50)            |   975 us  |     2 ms  |    18 ms  |
```
<!-- BENCHMARK_RESULTS_END -->

Run benchmarks yourself: `cargo run --release --bin mkb-bench`

## How It Works

MKB is a Rust core with a Python AI layer, connected via PyO3.

```
Your markdown files (the vault)
        |
        v
   SQLite + FTS5          <-- field index + full-text search
   sqlite-vec             <-- vector embeddings (1536-dim)
        |
        v
   MKQL query engine      <-- temporal-aware queries
        |
        v
   CLI / MCP server / Python API
```

**Key design decisions:**
- **Files are the source of truth** — the index is always derived, never authoritative
- **Temporal gate** — documents without `observed_at` are rejected at the boundary
- **Confidence decay** — older information is automatically deprioritized
- **SQLite everything** — FTS5 for text, sqlite-vec for vectors, no external services

## Development

```bash
cargo test --workspace          # Rust tests (180)
uv run pytest tests/python/ -v  # Python tests (74)
cargo clippy --workspace -- -D warnings
just ci                         # Full CI check
```

## License

Apache-2.0
