# MKB — Markdown Knowledge Base for LLMs

## Technical Specification v1.0

---

## 1. Executive Summary

MKB is a file-system-native knowledge base where **every knowledge unit is a markdown file with structured YAML frontmatter**. It combines the human-readability of markdown with the queryability of a database through a SQL-like DSL called **MKQL** (Markdown Knowledge Query Language), a full CLI interface, and an AI-powered ingestion pipeline that extracts both explicit facts and implicit signals from structured and unstructured sources.

**Design principles:**

- Files are the truth — no opaque binary stores, no vendor lock-in
- Git-native — every mutation is diffable, branchable, mergeable
- LLM-first — schemas, chunking, and retrieval are optimized for context windows
- Queryable — frontmatter fields are indexed and queryable via SQL-like syntax
- Inference-aware — AI ingestion extracts not just what's said, but what's implied
- **Time-grounded** — every knowledge unit has mandatory temporal anchoring; no information enters the vault without a timestamp

---

## 2. Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                      CLI (mkb)                          │
│  mkb query | mkb ingest | mkb add | mkb sync | mkb gc  │
└──────────────┬──────────────────────────┬───────────────┘
               │                          │
       ┌───────▼───────┐         ┌────────▼────────┐
       │  Query Engine  │         │ Ingestion Engine │
       │    (MKQL)      │         │   (AI Pipeline)  │
       └───────┬───────┘         └────────┬────────┘
               │                          │
       ┌───────▼──────────────────────────▼───────┐
       │              Index Layer                  │
       │  ┌────────────┐  ┌─────────────────────┐ │
       │  │ Field Index │  │ Vector Index (HNSW) │ │
       │  │ (SQLite FTS)│  │  (embeddings)       │ │
       │  └────────────┘  └─────────────────────┘ │
       └───────────────────┬──────────────────────┘
                           │
       ┌───────────────────▼──────────────────────┐
       │           Storage Layer                   │
       │  ┌──────────────────────────────────────┐ │
       │  │  .mkb/                               │ │
       │  │  ├── vault/          (markdown files) │ │
       │  │  ├── schemas/        (type defs)      │ │
       │  │  ├── indexes/        (SQLite + HNSW)  │ │
       │  │  ├── ingestion/      (source configs) │ │
       │  │  └── .mkb.toml       (config)         │ │
       │  └──────────────────────────────────────┘ │
       └──────────────────────────────────────────┘
```

---

## 2.5 Temporal Grounding — Core Invariant

> **No information enters the vault without a timestamp. Ever.**
>
> This is not a soft default. It is a hard gate. If temporal grounding
> cannot be determined — from the source data, from metadata, or from
> explicit user input — the ingestion pipeline REJECTS the input and
> logs the rejection with the reason.

**Rationale:** A fact without a timestamp is a liability. It cannot be:
- Ordered against contradicting facts
- Decayed for staleness
- Correlated with events
- Trusted by an LLM assembling context
- Used for trend detection

An undated "Project Alpha is on track" is worse than no information at all,
because it may reflect a state from 6 months ago while the project is now on fire.

---

## 3. Storage Layer — The Vault

### 3.1 Directory Structure

```
.mkb/
├── .mkb.toml                    # Global configuration
├── vault/                       # All knowledge files
│   ├── people/
│   │   ├── john-doe.md
│   │   └── jane-smith.md
│   ├── projects/
│   │   ├── project-alpha.md
│   │   └── project-beta.md
│   ├── decisions/
│   │   └── 2025-01-15-api-migration.md
│   ├── meetings/
│   │   └── 2025-02-10-standup.md
│   ├── concepts/
│   │   └── dag-operational-awareness.md
│   └── signals/                 # AI-inferred implicit knowledge
│       └── sig-2025-02-10-team-morale-drop.md
├── schemas/
│   ├── _base.yaml               # Common fields all docs inherit
│   ├── person.yaml
│   ├── project.yaml
│   ├── decision.yaml
│   ├── meeting.yaml
│   ├── concept.yaml
│   └── signal.yaml
├── indexes/
│   ├── fields.db                # SQLite with FTS5
│   └── vectors.bin              # HNSW vector index
├── ingestion/
│   ├── sources.yaml             # Registered data sources
│   ├── rejected/                # Documents rejected for missing temporal grounding
│   │   ├── 2025-02-10T14:30:00_unknown-001.md
│   │   └── _rejection_log.jsonl
│   └── transforms/              # Custom transform scripts
│       ├── jira-to-project.py
│       └── slack-to-signal.py
└── hooks/
    ├── pre-commit.sh            # Validate schemas on commit
    └── post-ingest.sh           # Re-index after ingestion
```

### 3.2 Document Anatomy

Every knowledge unit is a markdown file with three sections:

```markdown
---
# === SYSTEM FIELDS (auto-managed) ===
id: "proj-mbnxt-checkout-001"
type: project

# System temporal (file lifecycle)
_created_at: 2025-01-15T10:30:00Z
_modified_at: 2025-02-10T14:22:00Z

# Content temporal (MANDATORY - knowledge lifecycle)
observed_at: 2025-02-10T09:15:00Z        # When this information was true/observed
valid_until: 2025-02-24T09:15:00Z        # When this information expires
temporal_precision: exact                  # exact | day | week | month | quarter | approximate | inferred
occurred_at: null                          # When the event happened (if different from observed_at)

# Provenance
source: jira:GPROD-4521
source_hash: sha256:a1b2c3d4e5f6...
confidence: 0.95
provenance: ai-ingest:jira-adapter:v2.1

# Supersession chain
supersedes: "proj-mbnxt-checkout-001@2025-02-03"  # previous snapshot
superseded_by: null                                 # this is current

# === SCHEMA FIELDS (type-specific) ===
title: "Project Alpha — Mobile Checkout Redesign"
status: in_progress
owner: jane-smith
team: [mbnxt-app, platform]
priority: P1
start_date: 2025-01-15
target_date: 2025-03-31
tags: [mobile, checkout, ux, revenue]
stakeholders: [vp-product, cto]
depends_on: [proj-beta-002]
blocks: []
kpis:
  conversion_lift: "+12% target"
  error_rate: "<0.5%"

# === RELATION FIELDS ===
links:
  - rel: owner
    target: people/jane-smith
    observed_at: 2025-02-10T09:15:00Z    # links are timestamped too
  - rel: blocked_by
    target: projects/api-gateway-migration
    observed_at: 2025-02-08T11:00:00Z
  - rel: has_signal
    target: signals/sig-2025-02-08-velocity-decline
    observed_at: 2025-02-08T11:00:00Z
---

## Summary

Mobile checkout redesign targeting 12% conversion lift through simplified
3-step flow replacing current 7-step process.

## Context

Initiated after Q4 2024 data showed 67% cart abandonment on mobile...

## Current State (as of 2025-02-10)

Sprint 4 of 8 complete. Core flow implemented. Payment integration in progress.
Blocked on API Gateway migration.

Sprint velocity: 45 → 42 → 42 → 31 (measured 2025-02-10).

## Open Questions

- Payment provider: Stripe vs Adyen decision pending (see decision doc)
- A/B test strategy for gradual rollout

## Implicit Signals (AI-inferred, temporally anchored)

- [2025-02-08] Velocity decline: 42→31pts over Sprint 3→4 (conf: 0.87)
- [2025-02-07, 2025-02-10] Jane "stretched thin" — 2 standup mentions (conf: 0.82)
- [2025-02-09] Gateway team lost 1 engineer to incident rotation (conf: 0.79)
```

### 3.3 System Fields (All Documents)

| Field | Type | Auto | Description |
|-------|------|------|-------------|
| `id` | string | ✓ | Unique identifier (type-prefix + slug) |
| `type` | enum | ✓ | Schema type reference |
| `_created_at` | datetime | ✓ | When this vault file was first created (immutable) |
| `_modified_at` | datetime | ✓ | When this vault file was last modified |
| `observed_at` | datetime | ✓ | **REQUIRED** — When this information was true/observed (knowledge lifecycle) |
| `valid_until` | datetime | ✓ | **REQUIRED** — When this information expires (computed if not provided) |
| `temporal_precision` | enum | ✓ | **REQUIRED** — exact \| day \| week \| month \| quarter \| approximate \| inferred |
| `occurred_at` | datetime | — | When the described event happened (if different from observed_at) |
| `temporal_range` | json | — | For information spanning a period: {start, end, granularity} |
| `source` | string | ✓ | Origin reference (e.g., `jira:GPROD-4521`) |
| `source_hash` | string | ✓ | Content hash for dedup/change detection |
| `confidence` | float | ✓ | 0.0–1.0 confidence score (1.0 for human-authored) |
| `provenance` | string | ✓ | How this doc was created |
| `supersedes` | ref | — | Points to the document this replaces |
| `superseded_by` | ref | — | Points to replacement document |
| `superseded_at` | datetime | — | When this document was superseded |
| `tags` | string[] | — | Free-form classification tags |
| `links` | Link[] | — | Typed relationships to other documents (carry their own observed_at) |
| `embeddings_dirty` | bool | ✓ | Whether embeddings need refresh |

---

## 4. Schema System

### 4.1 Schema Definition Format

Schemas are YAML files that define the frontmatter contract for each document type.

```yaml
# schemas/project.yaml
name: project
version: 2
extends: _base
description: "A tracked initiative, epic, or workstream"

fields:
  title:
    type: string
    required: true
    indexed: true
    searchable: true       # included in FTS index

  status:
    type: enum
    values: [proposed, approved, in_progress, blocked, completed, cancelled]
    required: true
    indexed: true
    default: proposed

  owner:
    type: ref
    ref_type: person
    required: true
    indexed: true

  team:
    type: ref[]
    ref_type: team
    indexed: true

  priority:
    type: enum
    values: [P0, P1, P2, P3]
    indexed: true

  start_date:
    type: date
    indexed: true

  target_date:
    type: date
    indexed: true

  stakeholders:
    type: ref[]
    ref_type: person

  depends_on:
    type: ref[]
    ref_type: project

  blocks:
    type: ref[]
    ref_type: project

  kpis:
    type: map
    key_type: string
    value_type: string

  health:
    type: enum
    values: [green, yellow, red]
    indexed: true
    computed: true          # can be set by AI inference rules

computed_fields:
  days_remaining:
    expr: "date_diff(target_date, now())"
    type: integer
  is_overdue:
    expr: "target_date < now() AND status NOT IN ('completed', 'cancelled')"
    type: boolean
  dependency_risk:
    expr: |
      ANY(depends_on, d => 
        SELECT health FROM project WHERE id = d AND health = 'red'
      )
    type: boolean

validation:
  - rule: "target_date >= start_date"
    message: "Target date must be after start date"
  - rule: "owner IS NOT NULL"
    message: "Every project needs an owner"

display:
  icon: "🚀"
  summary_template: "{{title}} ({{status}}) — Owner: {{owner}}, Due: {{target_date}}"
  card_fields: [title, status, owner, priority, health]
```

### 4.2 Supported Field Types

| Type | Description | Indexable | Example |
|------|-------------|-----------|---------|
| `string` | Free text | FTS | `"Mobile Checkout"` |
| `integer` | Whole number | B-tree | `42` |
| `float` | Decimal number | B-tree | `0.92` |
| `boolean` | True/false | Bitmap | `true` |
| `date` | Date only | B-tree | `2025-03-31` |
| `datetime` | Full timestamp | B-tree | `2025-01-15T10:30:00Z` |
| `duration` | Time span | — | `90d`, `6m`, `2w` |
| `enum` | Constrained values | Hash | `in_progress` |
| `ref` | Link to another doc | Hash | `people/jane-smith` |
| `ref[]` | Multiple links | Multi-hash | `[proj-a, proj-b]` |
| `string[]` | Tag list | Multi-FTS | `[mobile, ux]` |
| `map` | Key-value pairs | — | `{conversion: "+12%"}` |
| `json` | Arbitrary JSON | — | Complex nested data |

### 4.3 Base Schema

```yaml
# schemas/_base.yaml v2 — temporal-mandatory
name: _base
version: 2
description: "Base fields inherited by all document types. Temporally grounded."

fields:
  # --- Identity ---
  id:
    type: string
    required: true
    unique: true
    auto: true

  type:
    type: enum
    required: true
    auto: true

  # --- System Temporal (file lifecycle, auto-managed) ---
  _created_at:
    type: datetime
    required: true
    auto: true
    indexed: true
    immutable: true           # never changes after creation

  _modified_at:
    type: datetime
    required: true
    auto: true
    indexed: true

  # --- Content Temporal (knowledge lifecycle, MANDATORY) ---
  observed_at:
    type: datetime
    required: true            # HARD REQUIREMENT — rejection if missing
    indexed: true
    description: >
      When was this information true or observed? This is the authoritative
      timestamp that anchors this knowledge in time. For API data, use the
      API timestamp. For meeting notes, use the meeting date. For AI
      inference, use the timestamp of the source data that was analyzed.

  valid_until:
    type: datetime
    required: true            # Always present — computed if not provided
    indexed: true
    description: >
      When does this information expire or need re-verification?
      Computed by decay model if not explicitly set.

  temporal_precision:
    type: enum
    values: [exact, day, week, month, quarter, approximate, inferred]
    required: true
    indexed: true
    default: inferred         # most conservative default

  occurred_at:
    type: datetime
    indexed: true
    description: "When the described event actually happened (if different from observed_at)"

  temporal_range:
    type: json
    description: "For information spanning a period: {start, end, granularity}"

  # --- Provenance ---
  source:
    type: string
    indexed: true

  source_hash:
    type: string

  confidence:
    type: float
    default: 1.0
    indexed: true

  provenance:
    type: string
    indexed: true

  # --- Lifecycle ---
  supersedes:
    type: ref
    description: "Document this replaces"

  superseded_at:
    type: datetime
    indexed: true

  superseded_by:
    type: ref
    description: "Document that replaced this one"

  tags:
    type: string[]
    indexed: true
    searchable: true

  links:
    type: json

  embeddings_dirty:
    type: boolean
    auto: true

validation:
  - rule: "observed_at IS NOT NULL"
    message: "REJECTED: No temporal grounding. Every document must have observed_at."
    severity: fatal

  - rule: "valid_until >= observed_at"
    message: "valid_until cannot be before observed_at"
    severity: fatal

  - rule: "occurred_at IS NULL OR occurred_at <= observed_at"
    message: "occurred_at should not be after observed_at (can't observe before it happens)"
    severity: warning

  - rule: "temporal_precision IS NOT NULL"
    message: "temporal_precision must be set"
    severity: fatal
```

---

## 5. MKQL — Query Language

### 5.1 Design Philosophy

MKQL is a SQL-like DSL designed for querying frontmatter fields and full-text content across the vault. It compiles down to SQLite queries against the field index and optionally combines with vector similarity search.

**Key differences from SQL:**

- `FROM` targets document types, not tables
- `BODY` keyword accesses markdown content
- `NEAR()` function for vector similarity
- `LINKED()` for graph traversal
- `IMPLICIT()` for AI-inferred fields
- Path expressions for nested fields (e.g., `kpis.conversion_lift`)
- Temporal functions: `FRESH()`, `STALE()`, `EXPIRED()`, `CURRENT()`, `LATEST()`, `EFF_CONFIDENCE()`, `AGE()`, `AS OF`, `HISTORY`

### 5.2 Grammar (EBNF)

```ebnf
query       = select_stmt | insert_stmt | update_stmt | delete_stmt ;

select_stmt = "SELECT" field_list
              "FROM" type_list
              ["WHERE" condition]
              ["LINK" link_clause]
              ["ORDER BY" order_list]
              ["LIMIT" integer]
              ["OFFSET" integer]
              ["CONTEXT" context_opts] ;

field_list  = "*" | field ("," field)* ;
field       = identifier | path_expr | agg_func | computed ;
path_expr   = identifier "." identifier ;
agg_func    = ("COUNT" | "AVG" | "SUM" | "MIN" | "MAX") "(" field ")" ;
computed    = "CONFIDENCE" | "FRESHNESS" | "RELEVANCE" | "EFF_CONFIDENCE" | "AGE" ;

type_list   = type_name ("," type_name)* | "*" ;
type_name   = identifier ;

condition   = predicate (("AND" | "OR") predicate)* ;
predicate   = field operator value
            | field "IN" "(" value_list ")"
            | field "IS" ("NULL" | "NOT NULL")
            | field "CONTAINS" string
            | field "MATCHES" regex
            | "BODY" "CONTAINS" string
            | "BODY" "MATCHES" regex
            | "NEAR" "(" string "," float ")"
            | "LINKED" "(" link_pattern ")"
            | "IMPLICIT" "(" signal_type ")"
            | "FRESH" "(" duration ")"
            | "STALE" "(" ")"
            | "EXPIRED" "(" ")"
            | "CURRENT" "(" ")"
            | "LATEST" "(" ")"
            | "DURING" "(" datetime "," datetime ")"
            | "OVERLAPS" "(" datetime "," datetime ")"
            | "AS" "OF" datetime
            | "HISTORY"
            | "NOT" predicate
            | "(" condition ")" ;

operator    = "=" | "!=" | ">" | "<" | ">=" | "<=" | "LIKE" ;

link_clause = link_pattern ("," link_pattern)* ;
link_pattern= [rel_type] "->" type_name ["AS" alias]
            | [rel_type] "<-" type_name ["AS" alias] ;

order_list  = order_item ("," order_item)* ;
order_item  = field ("ASC" | "DESC") ;

context_opts= "WINDOW" integer          /* max tokens for LLM context */
            | "FORMAT" ("full" | "summary" | "frontmatter" | "snippet")
            | "EMBED" ("true" | "false") ;

insert_stmt = "INSERT" type_name "SET" assignments ;
update_stmt = "UPDATE" type_name "SET" assignments "WHERE" condition ;
delete_stmt = "DELETE FROM" type_name "WHERE" condition ;
assignments = assignment ("," assignment)* ;
assignment  = field "=" value ;
```

### 5.3 Query Examples

#### Basic Queries

```sql
-- All active projects owned by Jane
SELECT title, status, priority, target_date
FROM project
WHERE owner = "jane-smith" AND status = "in_progress"
ORDER BY priority ASC, target_date ASC

-- Overdue tasks with high confidence
SELECT *
FROM project
WHERE is_overdue = true AND confidence >= 0.8
ORDER BY target_date ASC

-- Full-text search across all types
SELECT id, type, title, RELEVANCE
FROM *
WHERE BODY CONTAINS "cart abandonment"
ORDER BY RELEVANCE DESC
LIMIT 10

-- Documents observed in last 7 days (uses observed_at, not _modified_at)
SELECT title, type, observed_at
FROM *
WHERE FRESH(7d)
ORDER BY observed_at DESC

-- Only current, non-stale project information
SELECT title, status, health, EFF_CONFIDENCE() as trust
FROM project
WHERE CURRENT() AND EFF_CONFIDENCE() >= 0.5
ORDER BY trust DESC
```

#### Semantic / Vector Queries

```sql
-- Find docs semantically similar to a concept
SELECT title, type, RELEVANCE
FROM *
WHERE NEAR("team morale and burnout indicators", 0.75)
ORDER BY RELEVANCE DESC
LIMIT 5

-- Hybrid: semantic + field filter
SELECT title, status, health
FROM project
WHERE NEAR("performance degradation", 0.7)
  AND status = "in_progress"
  AND health IN ("yellow", "red")
```

#### Graph Traversal (LINK)

```sql
-- Projects blocked by Project Beta, with their owners
SELECT p.title, p.status, owner.name AS owner_name
FROM project AS p
LINK blocked_by -> project WHERE id = "proj-beta-002"
LINK owner -> person AS owner

-- All decisions that affect a specific project
SELECT d.title, d.created, d.outcome
FROM decision AS d
LINK affects -> project WHERE id = "proj-alpha-001"
ORDER BY d.created DESC

-- People connected to red-health projects
SELECT DISTINCT person.name, person.role
FROM person
LINK owns <- project WHERE health = "red"
```

#### Aggregations

```sql
-- Project health distribution
SELECT health, COUNT(*) as count
FROM project
WHERE status = "in_progress"
GROUP BY health

-- Average confidence by provenance
SELECT provenance, AVG(confidence) as avg_conf, COUNT(*) as total
FROM *
GROUP BY provenance
ORDER BY avg_conf ASC

-- Tags frequency analysis
SELECT UNNEST(tags) AS tag, COUNT(*) AS usage
FROM *
GROUP BY tag
ORDER BY usage DESC
LIMIT 20
```

#### AI-Inferred Signals

```sql
-- All implicit signals about team health
SELECT title, confidence, created, source
FROM signal
WHERE IMPLICIT("team_health")
  AND confidence >= 0.7
ORDER BY created DESC

-- Projects with AI-detected risk signals
SELECT p.title, s.title AS signal, s.confidence
FROM project AS p
LINK has_signal -> signal AS s
WHERE s.confidence >= 0.8
  AND s.signal_type = "risk"
```

#### Temporal Queries

```sql
-- What did we know about Project Alpha on Jan 15?
SELECT *
FROM project
WHERE id = "proj-alpha-001"
AS OF "2025-01-15"

-- Timeline of a project's health changes
SELECT observed_at, health, confidence, source
FROM project
WHERE id = "proj-alpha-001"
HISTORY
ORDER BY observed_at ASC

-- Signals from last 7 days, excluding inferred timestamps
SELECT title, signal_type, severity, EFF_CONFIDENCE() as trust
FROM signal
WHERE FRESH(7d)
  AND temporal_precision IN ("exact", "day")
  AND NOT STALE()
ORDER BY observed_at DESC

-- Information aging: what's getting stale?
SELECT title, type, observed_at, AGE(observed_at) as age,
       EFF_CONFIDENCE() as trust, valid_until
FROM *
WHERE EFF_CONFIDENCE() BETWEEN 0.3 AND 0.6
ORDER BY trust ASC
LIMIT 20

-- Latest known state of all projects (handles supersession)
SELECT title, status, health, observed_at, EFF_CONFIDENCE() as trust
FROM project
WHERE LATEST() AND status != "cancelled"
ORDER BY trust DESC, observed_at DESC

-- What happened during Sprint 8? (temporal range query)
SELECT type, title, observed_at, signal_type
FROM *
WHERE DURING("2025-02-03", "2025-02-14")
ORDER BY observed_at ASC
```

#### Context-Aware Retrieval (for LLM consumption)

```sql
-- Retrieve full context for LLM, limited to token budget
SELECT *
FROM project, decision, signal
WHERE tags CONTAINS "mobile"
  AND FRESH(30d)
  AND CURRENT()                    # exclude stale/expired
ORDER BY RELEVANCE DESC
CONTEXT WINDOW 4000 FORMAT full

-- Summary cards for dashboard
SELECT title, status, health, owner, days_remaining, EFF_CONFIDENCE() as trust
FROM project
WHERE status IN ("in_progress", "blocked")
  AND CURRENT()
CONTEXT FORMAT summary
```

### 5.4 Query Compilation Pipeline

```
MKQL String
    │
    ▼
┌──────────┐
│  Parser  │ → AST (Abstract Syntax Tree)
└────┬─────┘
     │
     ▼
┌──────────────┐
│  Optimizer   │ → Rewrite rules, predicate pushdown
└────┬─────────┘
     │
     ▼
┌──────────────────┐
│  Plan Generator  │ → Execution plan with cost estimates
└────┬─────────────┘
     │
     ├──▶ SQLite FTS5 (field predicates + full-text)
     ├──▶ HNSW Index (NEAR() vector queries)
     └──▶ File System (LINK graph traversal)
           │
           ▼
     ┌────────────┐
     │  Merger    │ → Intersect/union result sets
     └────┬───────┘
          │
          ▼
     ┌────────────┐
     │ Formatter  │ → JSON | Table | Markdown | LLM Context
     └────────────┘
```

---

## 6. Indexing Layer

### 6.1 Field Index (SQLite + FTS5)

All frontmatter fields are extracted and stored in a normalized SQLite database.

```sql
-- Core documents table
CREATE TABLE documents (
    id                TEXT PRIMARY KEY,
    type              TEXT NOT NULL,
    path              TEXT NOT NULL UNIQUE,
    _created_at       TEXT NOT NULL,
    _modified_at      TEXT NOT NULL,
    observed_at       TEXT NOT NULL,        -- MANDATORY temporal grounding
    valid_until        TEXT NOT NULL,        -- MANDATORY expiry
    temporal_precision TEXT NOT NULL,       -- MANDATORY precision level
    occurred_at       TEXT,
    confidence        REAL DEFAULT 1.0,
    provenance        TEXT,
    content_hash      TEXT,
    supersedes        TEXT,
    superseded_by     TEXT,
    superseded_at     TEXT
);

CREATE INDEX idx_docs_type ON documents(type);
CREATE INDEX idx_docs_modified ON documents(_modified_at);
CREATE INDEX idx_docs_confidence ON documents(confidence);
CREATE INDEX idx_docs_observed ON documents(observed_at);
CREATE INDEX idx_docs_valid_until ON documents(valid_until);
CREATE INDEX idx_docs_precision ON documents(temporal_precision);
CREATE INDEX idx_docs_superseded ON documents(superseded_by);

-- Composite indexes for common temporal query patterns
CREATE INDEX idx_docs_current ON documents(type, superseded_by, valid_until)
  WHERE superseded_by IS NULL;
CREATE INDEX idx_docs_timeline ON documents(type, observed_at DESC);

-- Temporal version chain (for AS OF queries)
CREATE TABLE document_versions (
    doc_id          TEXT NOT NULL,
    version_id      TEXT NOT NULL,     -- doc_id@observed_at
    observed_at     TEXT NOT NULL,
    snapshot_hash   TEXT NOT NULL,      -- hash of frontmatter at this point
    PRIMARY KEY (doc_id, observed_at)
);

CREATE INDEX idx_versions_lookup ON document_versions(doc_id, observed_at DESC);

-- Contradiction tracking
CREATE TABLE contradictions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_id       TEXT NOT NULL,
    field_name      TEXT NOT NULL,
    doc_a_id        TEXT NOT NULL,
    doc_a_value     TEXT,
    doc_a_observed  TEXT NOT NULL,
    doc_b_id        TEXT NOT NULL,
    doc_b_value     TEXT,
    doc_b_observed  TEXT NOT NULL,
    resolved        INTEGER DEFAULT 0,
    resolution      TEXT,              -- "temporal_ordering" | "confidence" | "manual"
    detected_at     TEXT NOT NULL
);

-- Field values (EAV pattern for flexible schemas)
CREATE TABLE field_values (
    doc_id      TEXT NOT NULL REFERENCES documents(id),
    field_name  TEXT NOT NULL,
    field_type  TEXT NOT NULL,
    str_value   TEXT,
    int_value   INTEGER,
    float_value REAL,
    date_value  TEXT,
    bool_value  INTEGER,
    PRIMARY KEY (doc_id, field_name)
);

CREATE INDEX idx_fv_field ON field_values(field_name, str_value);
CREATE INDEX idx_fv_date ON field_values(field_name, date_value);

-- Multi-value fields (tags, refs)
CREATE TABLE field_arrays (
    doc_id      TEXT NOT NULL REFERENCES documents(id),
    field_name  TEXT NOT NULL,
    value       TEXT NOT NULL
);

CREATE INDEX idx_fa_lookup ON field_arrays(field_name, value);

-- Links / relationships
CREATE TABLE links (
    source_id   TEXT NOT NULL REFERENCES documents(id),
    rel_type    TEXT NOT NULL,
    target_id   TEXT NOT NULL,
    metadata    TEXT  -- JSON for extra link attributes
);

CREATE INDEX idx_links_source ON links(source_id, rel_type);
CREATE INDEX idx_links_target ON links(target_id, rel_type);

-- Full-text search
CREATE VIRTUAL TABLE content_fts USING fts5(
    doc_id,
    title,
    body,
    tags,
    tokenize='porter unicode61'
);
```

### 6.2 Vector Index

Embeddings are stored in an HNSW (Hierarchical Navigable Small World) index for fast approximate nearest neighbor search.

```python
# Vector index configuration
VECTOR_CONFIG = {
    "model": "text-embedding-3-small",  # or local: nomic-embed-text
    "dimensions": 1536,
    "chunk_strategy": "semantic",        # semantic | fixed | sliding
    "chunk_max_tokens": 512,
    "overlap_tokens": 64,
    "index_type": "hnsw",
    "hnsw_params": {
        "M": 16,
        "ef_construction": 200,
        "ef_search": 100
    }
}
```

Each document produces 1+ embedding vectors:

| Vector Type | Source | Use Case |
|------------|--------|----------|
| `doc_summary` | Frontmatter + first 200 tokens | Quick semantic match |
| `doc_chunk_N` | Semantic chunks of body | Deep content retrieval |
| `field_composite` | Concatenated key fields | Structured similarity |

### 6.3 Index Maintenance

```
File Watcher (inotify/fsevents)
    │
    ├── File created/modified → hash check → re-index if changed
    ├── File deleted → remove from all indexes
    └── Schema modified → validate + re-index affected type

Periodic jobs:
    ├── Temporal staleness sweep (every 6h) → recompute effective confidence, archive expired
    ├── Embedding refresh (on dirty flag) → re-embed modified docs
    ├── Integrity check (daily) → vault ↔ index consistency
    ├── Contradiction detection (daily) → find and resolve temporal conflicts
    └── Orphan cleanup (weekly) → remove dangling links
```

---

## 7. CLI Interface

### 7.1 Command Structure

```
mkb <command> [subcommand] [flags] [arguments]

COMMANDS:
  init          Initialize a new MKB vault
  add           Create a new knowledge document
  edit          Modify an existing document
  rm            Remove a document (soft delete with --hard for permanent)
  query / q     Execute an MKQL query
  search / s    Quick full-text or semantic search
  ingest        Run AI ingestion pipeline
  link          Manage relationships between documents
  schema        Manage document type schemas
  index         Manage indexes
  sync          Sync with external data sources
  export        Export query results
  gc            Garbage collection (staleness, contradictions, orphans)
  serve         Start HTTP API server
  repl          Interactive query shell
  stats         Vault statistics
  report        Temporal analytics (freshness, timeline, decay)
```

### 7.2 Detailed Command Reference

#### `mkb init`

```bash
# Initialize new vault in current directory
mkb init

# Initialize with specific config
mkb init --name "engineering-kb" --llm openai --embedding-model text-embedding-3-small

# Initialize from template
mkb init --template engineering-leadership
```

Generated `.mkb.toml`:

```toml
[vault]
name = "engineering-kb"
version = "1.0"
root = ".mkb/vault"

[llm]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key_env = "ANTHROPIC_API_KEY"

[embedding]
provider = "openai"
model = "text-embedding-3-small"
dimensions = 1536
api_key_env = "OPENAI_API_KEY"

[index]
backend = "sqlite"
fts_tokenizer = "porter unicode61"
vector_backend = "hnswlib"

[ingestion]
confidence_threshold = 0.6
implicit_extraction = true
dedup_strategy = "content_hash"

[gc]
ttl_sweep_interval = "1h"
orphan_cleanup_interval = "7d"
soft_delete_retention = "30d"
```

#### `mkb add`

```bash
# Interactive (prompts for fields based on schema)
mkb add project

# Inline
mkb add project \
  --title "Payment Gateway Migration" \
  --owner jane-smith \
  --priority P1 \
  --tags payment,infrastructure \
  --target-date 2025-06-30

# From file
mkb add --file draft.md --type project

# From stdin (pipe from other tools)
echo "Meeting notes from today..." | mkb add meeting --title "Standup Feb 10" --infer

# With AI enrichment (--infer flag)
mkb add meeting --file notes.md --infer
# AI will: extract action items, identify participants, detect sentiment,
# create links to mentioned projects/people, generate tags
```

#### `mkb query` / `mkb q`

```bash
# Direct MKQL
mkb q 'SELECT title, status FROM project WHERE health = "red"'

# With output format
mkb q 'SELECT * FROM project WHERE owner = "jane-smith"' --format table
mkb q 'SELECT * FROM project WHERE owner = "jane-smith"' --format json
mkb q 'SELECT * FROM project WHERE owner = "jane-smith"' --format markdown
mkb q 'SELECT * FROM project WHERE owner = "jane-smith"' --format csv

# Temporal awareness
mkb q 'SELECT * FROM project' --as-of 2025-01-15           # point-in-time snapshot
mkb q 'SELECT * FROM project' --current-only               # CURRENT() shortcut
mkb q 'SELECT * FROM project' --min-confidence 0.5        # with decay applied
mkb q 'SELECT * FROM project' --show-decay                 # display eff_confidence

# Context mode (for piping to LLM)
mkb q 'SELECT * FROM project WHERE FRESH(7d) AND CURRENT()' --format context --max-tokens 4000

# Save query as named view
mkb q --save "my-red-projects" 'SELECT * FROM project WHERE health = "red" AND owner = "me"'

# Run saved view
mkb q --view my-red-projects

# Explain query plan
mkb q --explain 'SELECT * FROM project WHERE NEAR("performance issue", 0.7)'
```

#### `mkb search` / `mkb s`

```bash
# Quick full-text search
mkb s "cart abandonment mobile"

# Semantic search
mkb s --semantic "team seems burned out and velocity is dropping"

# Hybrid (FTS + semantic, merged by RRF)
mkb s --hybrid "payment integration issues" --limit 10

# Scoped search
mkb s "blocked" --type project --where 'status = "in_progress"'

# Natural language query (AI translates to MKQL)
mkb s --natural "what projects is Jane working on that are at risk?"
```

#### `mkb ingest`

```bash
# Ingest single file
mkb ingest file meeting-notes.txt --type meeting --infer

# Ingest with explicit temporal grounding (REQUIRED if source lacks timestamp)
mkb ingest file notes.md --observed-at 2025-02-08
mkb ingest file notes.md --observed-at 2025-02-08 --precision day
mkb ingest file notes.md --valid-until 2025-03-01

# Ingest directory (rejects undated by default)
mkb ingest dir ./exports/jira/ --source jira --transform jira-to-project
mkb ingest dir ./jira/ --reject-undated                     # (default: on)
mkb ingest dir ./jira/ --no-reject-undated                  # allow undated (DANGER)

# Ingest from configured source
mkb ingest source jira --since 2025-02-01
mkb ingest source slack --channel engineering --since 7d
mkb ingest source google-docs --folder "1:1 Notes"

# Bulk ingest with progress
mkb ingest dir ./data/ --recursive --parallel 4 --progress

# Dry run (show what would be created/updated)
mkb ingest file notes.md --type meeting --infer --dry-run

# Re-ingest (force update even if source hash matches)
mkb ingest source jira --since 2025-01-01 --force

# Handle rejections (documents without temporal grounding)
mkb ingest --rejected                                    # list rejections
mkb ingest --recover 2025-02-10T14:30:00_unknown-001.md --observed-at 2025-02-05
mkb ingest --recover-all --observed-at 2025-02-08 --batch "jira-import-feb"
```

#### `mkb link`

```bash
# Create relationship
mkb link proj-alpha-001 --rel depends_on --target proj-beta-002

# List relationships
mkb link --list proj-alpha-001
mkb link --list proj-alpha-001 --rel depends_on

# Remove relationship
mkb link --rm proj-alpha-001 --rel depends_on --target proj-beta-002

# Auto-detect links (AI scans content for references)
mkb link --auto proj-alpha-001

# Visualize graph
mkb link --graph --type project --output deps.svg
mkb link --graph --center proj-alpha-001 --depth 2
```

#### `mkb schema`

```bash
# List schemas
mkb schema list

# Show schema details
mkb schema show project

# Create new schema
mkb schema create custom-type --from template

# Validate all documents against schemas
mkb schema validate

# Migrate (when schema version changes)
mkb schema migrate project --from 1 --to 2 --dry-run
mkb schema migrate project --from 1 --to 2
```

#### `mkb repl`

```bash
$ mkb repl

mkb> SELECT title, health FROM project WHERE status = "in_progress"
┌──────────────────────────────────┬────────┐
│ title                            │ health │
├──────────────────────────────────┼────────┤
│ Mobile Checkout Redesign         │ yellow │
│ API Gateway Migration            │ green  │
│ Search Relevance v3              │ red    │
└──────────────────────────────────┴────────┘
3 results (12ms)

mkb> .mode json
mkb> SELECT * FROM signal WHERE FRESH(7d) AND confidence > 0.8
[{ "id": "sig-001", ... }]

mkb> .explain
mkb> SELECT * FROM project WHERE NEAR("scaling issues", 0.7)
Plan: vector_scan(threshold=0.7) → filter(type=project) → fetch_docs
Est. cost: 45ms, scans: ~150 vectors

mkb> .natural what decisions were made about the mobile app this quarter?
→ Translated to: SELECT * FROM decision WHERE tags CONTAINS "mobile"
  AND created >= "2025-01-01" ORDER BY created DESC
```

### 7.3 Output Formats

| Format | Flag | Use Case |
|--------|------|----------|
| `table` | `--format table` | Human reading in terminal |
| `json` | `--format json` | Piping to other tools |
| `jsonl` | `--format jsonl` | Streaming processing |
| `csv` | `--format csv` | Spreadsheet export |
| `markdown` | `--format markdown` | Documentation |
| `context` | `--format context` | LLM context window injection |
| `frontmatter` | `--format frontmatter` | Just the YAML metadata |
| `ids` | `--format ids` | Just document IDs (for scripting) |

### 7.4 Piping & Composition

```bash
# Pipe query results to LLM
mkb q 'SELECT * FROM project WHERE health = "red"' --format context | \
  llm "Summarize these at-risk projects and suggest interventions"

# Chain queries
mkb q 'SELECT owner FROM project WHERE health = "red"' --format ids | \
  xargs -I{} mkb q "SELECT * FROM person WHERE id = '{}'"

# Export to file
mkb q 'SELECT * FROM decision WHERE FRESH(90d)' --format markdown > decisions-q1.md

# Watch for changes
mkb q 'SELECT title, health FROM project WHERE health != "green"' --watch 60

# Batch operations
mkb q 'SELECT id FROM project WHERE status = "completed"' --format ids | \
  xargs -I{} mkb edit {} --set ttl=90d

# Temporal maintenance
mkb gc --sweep-stale                  # recompute effective confidence, archive expired
mkb gc --stale-report                 # show what's decaying
mkb gc --find-contradictions          # temporal conflicts
mkb gc --archive-expired              # move expired to archive/

# Temporal analytics
mkb report freshness                  # vault freshness heatmap
mkb report timeline --type project    # temporal distribution
mkb report decay --type signal        # decay curve visualization
```

---

## 8. AI Ingestion Pipeline

### 8.1 Pipeline Architecture

```
Source Data (structured or unstructured)
    │
    ▼
┌─────────────┐
│ Source Adapter│ → Normalize to common intermediate format
└──────┬──────┘
       │
       ▼
┌──────────────┐
│  Preprocessor │ → Clean, chunk, dedup (content hash)
└──────┬───────┘
       │
       ▼
┌──────────────────────┐
│  Temporal Extractor   │ → Extract observed_at, occurred_at, precision
│  (Priority chain:     │   REJECTS if no timestamp found
│   explicit → user →    │
│   metadata → AI)       │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Explicit Extractor  │ → Named entities, dates, facts, structured data
│  (deterministic +    │
│   NER + regex)       │
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Implicit Extractor  │ → Sentiment, relationships, risk signals,
│  (LLM inference)     │   unstated assumptions, behavioral patterns
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Schema Mapper       │ → Map extracted data to document schemas
│                      │   Assign types, fill fields, create links
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Temporal Gate        │ → VALIDATE: observed_at present, valid_until computed
│                      │   REJECT if temporal grounding missing
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Confidence Scorer   │ → Assign confidence per field and document
│                      │   Based on source reliability + extraction method
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Decay Model         │ → Compute valid_until based on type + precision
│                      │   Apply decay profiles
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Dedup & Merge       │ → Match against existing docs
│                      │   Update vs create decision
│                      │   Resolve contradictions via temporal ordering
└──────┬───────────────┘
       │
       ▼
┌──────────────────────┐
│  Writer              │ → Create/update markdown files
│                      │   Update indexes, trigger hooks
└──────────────────────┘
```

### 8.2 Source Adapters

Each source adapter must declare how it extracts `observed_at`. If the mapping produces `null`, the temporal gate rejects.

```yaml
# ingestion/sources.yaml — temporal mappings
sources:
  jira:
    type: rest_api
    base_url: "https://groupon.atlassian.net"
    auth: oauth2
    credentials_env: JIRA_TOKEN
    endpoints:
      issues: "/rest/api/3/search"
      comments: "/rest/api/3/issue/{key}/comment"
    mapping:
      project_key: [GPROD, JPROD]
    schedule: "*/30 * * * *"    # every 30 min
    transform: jira-to-project
    temporal_mapping:
      observed_at: "fields.updated"               # Jira's last-updated timestamp
      occurred_at: "fields.created"                # when issue was created
      temporal_precision: "exact"                  # API timestamps are exact
      valid_until_rule: "observed_at + 14d"        # Jira status is volatile
    fallback_chain:
      - "fields.updated"
      - "fields.created"
      - "fields.resolutiondate"
      # No fallback to "now" — if all null, reject

  slack:
    type: webhook
    channels: [engineering, incidents, standups]
    auth: bot_token
    credentials_env: SLACK_BOT_TOKEN
    schedule: realtime
    transform: slack-to-signal
    temporal_mapping:
      observed_at: "ts"                            # Slack message timestamp (epoch)
      temporal_precision: "exact"
      valid_until_rule: "observed_at + 7d"         # Slack is ephemeral context
    # Slack always has ts — no fallback needed

  google_docs:
    type: google_drive
    folders: ["1:1 Notes", "Meeting Notes", "RFCs"]
    auth: service_account
    credentials_file: ~/.mkb/gcp-sa.json
    schedule: "0 */4 * * *"     # every 4 hours
    transform: doc-to-meeting
    temporal_mapping:
      observed_at: "modifiedTime"                  # Drive API last modified
      occurred_at: null                             # needs AI inference from content
      temporal_precision: "day"                    # doc-level, not paragraph-level
      valid_until_rule: "observed_at + 30d"
    fallback_chain:
      - "modifiedTime"
      - "createdTime"

  email:
    type: imap
    server: imap.gmail.com
    labels: [INBOX, "Engineering Updates"]
    auth: app_password
    credentials_env: EMAIL_APP_PASSWORD
    schedule: "0 * * * *"       # hourly
    transform: email-to-signal
    temporal_mapping:
      observed_at: "headers.Date"                  # RFC2822 Date header
      temporal_precision: "exact"
      valid_until_rule: "observed_at + 14d"

  meeting_transcript:
    type: file
    patterns: ["*meeting*.md", "*standup*.txt"]
    transform: doc-to-meeting
    temporal_mapping:
      observed_at: "metadata.meeting_start"        # calendar event start time
      occurred_at: "metadata.meeting_start"
      temporal_precision: "exact"
      valid_until_rule: "observed_at + 14d"        # action items decay fast
    fallback_chain:
      - "metadata.meeting_start"
      - "metadata.calendar_event.start"
      - "filename_date_pattern"                    # "2025-02-10-standup.md"
      - "ai_inference"

  csv_import:
    type: file
    watch_dir: ./imports/
    patterns: ["*.csv", "*.xlsx"]
    transform: tabular-to-typed
    temporal_mapping:
      observed_at: "column:date OR column:timestamp OR column:created_at"
      temporal_precision: "day"                    # CSV dates rarely have times
      valid_until_rule: "profile_default"
    fallback_chain:
      - "date_column_auto_detect"                  # scan headers for date-like names
      - "file_mtime"
      # If CSV has no date column and file mtime is unreliable → reject

  clipboard:
    type: manual
    description: "Paste unstructured text for AI processing"
    temporal_mapping:
      observed_at: "MUST BE PROVIDED"              # --observed-at flag required
      temporal_precision: "user_specified"
      valid_until_rule: "profile_default"
    # Manual paste with no --observed-at → immediate rejection with clear error
```

### 8.3 Temporal Extraction Pipeline

When ingesting data, MKB attempts to determine `observed_at` through a strict priority chain. If ALL methods fail → **REJECT**.

```
Priority 1: EXPLICIT TIMESTAMP IN SOURCE DATA
  │  Jira: issue.fields.updated / created
  │  Slack: message.ts
  │  Email: Date header
  │  Calendar: event.start
  │  Git: commit.author_date
  │  API response: timestamp field
  │  CSV/XLSX: date column
  │  ✓ temporal_precision = "exact"
  │
  ├── Found? → Use it
  │
  ▼
Priority 2: USER-PROVIDED TIMESTAMP
  │  CLI: mkb ingest --observed-at 2025-02-10
  │  Frontmatter: user wrote observed_at in the file
  │  ✓ temporal_precision = user-specified or "day"
  │
  ├── Found? → Use it
  │
  ▼
Priority 3: FILE/DOCUMENT METADATA
  │  File modification time (mtime)
  │  Document metadata (PDF creation date, EXIF data)
  │  Filename date pattern: "2025-02-10-standup.md"
  │  ✓ temporal_precision = "day" or "approximate"
  │
  ├── Found? → Use it (with lower confidence)
  │
  ▼
Priority 4: AI TEMPORAL INFERENCE
  │  LLM analyzes content for temporal markers:
  │    "yesterday we decided..."  → observed_at = content_date - 1d
  │    "last sprint..."           → observed_at = sprint_start_date
  │    "Q4 results show..."       → observed_at = Q4_end_date
  │    "the recent outage..."     → cross-reference incident logs
  │  ✓ temporal_precision = "inferred"
  │  ✓ confidence reduced by 0.15
  │
  ├── Found with confidence >= 0.5? → Use it
  │
  ▼
Priority 5: REJECTION
  │  No temporal grounding could be determined.
  │  ✗ Document is NOT created.
  │  ✗ Logged to ingestion/rejected/ with reason.
  │  ✗ User notified with suggested fix.
  │
  └── REJECTED: "Cannot determine when this information was true.
       Provide --observed-at flag or add date context to content."
```

Rejected documents are preserved in `.mkb/ingestion/rejected/` for recovery. Each rejection includes:
- Original content
- Rejection reason
- Extraction attempts made
- Suggested fix

### 8.4 Explicit Extraction

Deterministic extraction of structured information:

```python
class ExplicitExtractor:
    """Extract clearly stated facts from source data."""

    def extract(self, content: str, source_type: str) -> ExplicitFacts:
        return ExplicitFacts(
            entities=self._extract_entities(content),      # NER
            dates=self._extract_dates(content),            # dateutil parsing
            references=self._extract_references(content),  # @mentions, ticket IDs
            urls=self._extract_urls(content),
            code_refs=self._extract_code_refs(content),    # file paths, functions
            metrics=self._extract_metrics(content),        # numbers with context
            action_items=self._extract_actions(content),   # "TODO", "ACTION:"
            decisions=self._extract_decisions(content),    # "DECIDED:", "AGREED:"
            statuses=self._extract_statuses(content),      # "DONE", "BLOCKED"
        )

    def _extract_entities(self, content: str) -> List[Entity]:
        """NER + custom patterns for domain-specific entities."""
        patterns = {
            "jira_ticket": r"[A-Z]+-\d+",
            "person_mention": r"@[\w.-]+",
            "team_name": r"(?:team|squad)\s+[\w-]+",
            "metric": r"\d+(?:\.\d+)?%|\$[\d,.]+|\d+(?:ms|s|min|hrs?)",
            "date_ref": r"(?:by|before|after|due)\s+\w+\s+\d{1,2}",
        }
        # ... pattern matching + spaCy NER
```

### 8.5 Implicit Extraction

LLM-powered inference of unstated information:

```python
class ImplicitExtractor:
    """Infer information not explicitly stated in source data."""

    EXTRACTION_PROMPT = """
    Analyze the following content and extract IMPLICIT information —
    things not directly stated but inferable from context, tone,
    patterns, or domain knowledge.

    For each inference, provide:
    1. signal_type: category of the inference
    2. description: what you inferred
    3. evidence: specific text/patterns that led to this inference
    4. confidence: 0.0-1.0 how confident you are
    5. affected_entities: people, projects, or concepts affected

    Signal types to look for:
    - SENTIMENT: emotional tone, morale indicators
    - RISK: schedule, technical, or organizational risks
    - RELATIONSHIP: interpersonal dynamics, power shifts
    - BLOCKER: unstated dependencies or impediments
    - OPPORTUNITY: unmentioned potential gains
    - PATTERN: recurring behaviors or trends
    - ASSUMPTION: unstated beliefs driving decisions
    - TENSION: conflicts between goals, teams, or priorities
    - CAPACITY: workload, bandwidth, burnout signals
    - ALIGNMENT: agreement/disagreement with strategy

    Content to analyze:
    ---
    {content}
    ---

    Context (existing knowledge):
    ---
    {context}
    ---

    Return as JSON array of signal objects.
    """

    SIGNAL_TYPES = [
        "sentiment", "risk", "relationship", "blocker",
        "opportunity", "pattern", "assumption", "tension",
        "capacity", "alignment"
    ]

    async def extract(
        self,
        content: str,
        context: List[Document],
        source_metadata: dict
    ) -> List[ImplicitSignal]:
        # Retrieve relevant context from existing vault
        context_docs = await self.vault.query(
            f'SELECT * FROM * WHERE NEAR("{content[:200]}", 0.6) LIMIT 5'
        )

        prompt = self.EXTRACTION_PROMPT.format(
            content=content,
            context=self._format_context(context_docs)
        )

        response = await self.llm.complete(prompt, response_format="json")
        signals = self._parse_signals(response)

        # Cross-reference with existing knowledge
        for signal in signals:
            signal.corroboration = await self._find_corroboration(signal)
            signal.confidence = self._adjust_confidence(
                signal.confidence,
                signal.corroboration,
                source_metadata.get("reliability", 0.8)
            )

        return [s for s in signals if s.confidence >= self.threshold]
```

### 8.6 Confidence Scoring

```python
class ConfidenceScorer:
    """Multi-factor confidence scoring for extracted knowledge."""

    WEIGHTS = {
        "source_reliability": 0.3,    # How trustworthy is the source?
        "extraction_method": 0.25,    # Deterministic vs AI inference
        "corroboration": 0.2,         # Confirmed by other sources?
        "freshness": 0.15,            # How recent is the source?
        "specificity": 0.1,           # Vague vs specific claims
    }

    SOURCE_RELIABILITY = {
        "human_authored": 1.0,
        "jira_api": 0.95,
        "google_docs": 0.9,
        "meeting_transcript": 0.85,
        "slack_message": 0.8,
        "email": 0.8,
        "ai_inference": 0.6,          # base, adjusted by corroboration
        "third_party_api": 0.7,
    }

    EXTRACTION_METHOD_SCORES = {
        "direct_field_mapping": 1.0,  # Jira status → project status
        "regex_extraction": 0.95,     # Pattern-matched fact
        "ner_extraction": 0.85,       # Named entity recognition
        "llm_explicit": 0.8,          # LLM extracted stated fact
        "llm_implicit": 0.6,          # LLM inferred unstated fact
    }

    def score(self, extraction: Extraction) -> float:
        factors = {
            "source_reliability": self.SOURCE_RELIABILITY.get(
                extraction.source_type, 0.5
            ),
            "extraction_method": self.EXTRACTION_METHOD_SCORES.get(
                extraction.method, 0.5
            ),
            "corroboration": self._corroboration_score(extraction),
            "freshness": self._freshness_score(extraction.source_date),
            "specificity": self._specificity_score(extraction.content),
        }

        return sum(
            factors[k] * self.WEIGHTS[k]
            for k in self.WEIGHTS
        )
```

### 8.7 Dedup & Merge Strategy

```
New extraction arrives
    │
    ├── Compute content_hash
    │
    ├── Exact match (same source + hash)?
    │   └── Skip (no changes)
    │
    ├── Same source, different hash?
    │   └── Update existing doc, bump modified timestamp
    │
    ├── Different source, same entity?
    │   ├── Merge fields (higher confidence wins per field)
    │   ├── Append new information to body
    │   └── Link sources in provenance chain
    │
    └── New entity?
        └── Create new document

### 8.8 Contradiction Resolution via Temporal Ordering

When multiple documents describe the same entity with conflicting values, MKB resolves using temporal ordering:

1. **Sort by `observed_at`** (newest first)
2. **Newer observation supersedes older** — automatically marks older docs as `superseded_by`
3. **Same timestamp?** Higher confidence wins
4. **Contradictions tracked** in `contradictions` table for review

```python
class ContradictionResolver:
    """Resolve conflicting information using temporal ordering."""

    def resolve(self, docs: List[Document], field: str) -> Resolution:
        sorted_docs = sorted(docs, key=lambda d: d.observed_at, reverse=True)
        newest = sorted_docs[0]
        older = sorted_docs[1:]

        if newest.observed_at > older[0].observed_at:
            # Auto-supersede: newer observation wins
            for old_doc in older:
                if self._same_entity_same_field(newest, old_doc, field):
                    old_doc.superseded_by = newest.id
                    old_doc.superseded_at = newest.observed_at
                    self.vault.update(old_doc)

            return Resolution(
                winner=newest,
                superseded=older,
                strategy="temporal_ordering"
            )
```

Query contradictions:
```sql
-- Find all active contradictions (unresolved)
SELECT a.id, a.title, a.observed_at AS when_a,
       b.id, b.title, b.observed_at AS when_b,
       a.health AS state_a, b.health AS state_b
FROM project a
JOIN project b ON a.id = b.id
WHERE a.observed_at < b.observed_at
  AND a.superseded_by IS NULL
  AND a.health != b.health
```

---

## 9. Information Decay Model

### 9.1 Decay Philosophy

Information doesn't become false at a specific moment — it becomes *less trustworthy* over time. The rate of decay depends on the type of information and how volatile the domain is.

```
Effective Confidence = base_confidence × decay_factor(age, type)
```

### 9.2 Decay Curves by Information Type

```yaml
# .mkb.toml [decay] section
[decay]
enabled = true
sweep_interval = "6h"        # how often to recompute valid_until

[decay.profiles]
  # Project status changes fast — stale in days
  [decay.profiles.project_status]
    applies_to = { type = "project", fields = ["status", "health"] }
    half_life = "14d"          # confidence halves every 14 days
    hard_expiry = "60d"        # force re-verification after 60 days
    volatility = "high"

  # Project metadata changes slowly
  [decay.profiles.project_metadata]
    applies_to = { type = "project", fields = ["title", "owner", "team"] }
    half_life = "90d"
    hard_expiry = "180d"
    volatility = "low"

  # People info is relatively stable
  [decay.profiles.person]
    applies_to = { type = "person" }
    half_life = "180d"
    hard_expiry = "365d"
    volatility = "low"

  # Decisions are permanent (but may be superseded)
  [decay.profiles.decision]
    applies_to = { type = "decision" }
    half_life = "never"        # decisions don't decay
    hard_expiry = "never"
    volatility = "none"
    note = "Decisions are superseded, not decayed"

  # Signals decay fast — they're observations, not facts
  [decay.profiles.signal]
    applies_to = { type = "signal" }
    half_life = "7d"
    hard_expiry = "30d"
    volatility = "very_high"

  # Meeting notes are historical record
  [decay.profiles.meeting]
    applies_to = { type = "meeting" }
    half_life = "never"
    hard_expiry = "never"
    volatility = "none"
    note = "Meetings happened; action items decay separately"

  # Meeting action items decay
  [decay.profiles.action_item]
    applies_to = { type = "meeting", fields = ["action_items"] }
    half_life = "14d"
    hard_expiry = "30d"
    volatility = "high"

  # Concepts are stable knowledge
  [decay.profiles.concept]
    applies_to = { type = "concept" }
    half_life = "365d"
    hard_expiry = "never"
    volatility = "very_low"
```

### 9.3 Decay Computation

```python
import math
from datetime import datetime, timedelta

class DecayModel:
    """Compute time-adjusted confidence for knowledge units."""

    def effective_confidence(
        self,
        base_confidence: float,
        observed_at: datetime,
        profile: DecayProfile,
        now: datetime = None
    ) -> float:
        now = now or datetime.utcnow()
        age = (now - observed_at).total_seconds()

        if profile.half_life == "never":
            return base_confidence

        half_life_seconds = self._parse_duration(profile.half_life).total_seconds()

        # Exponential decay: C(t) = C₀ × 0.5^(t / half_life)
        decay_factor = math.pow(0.5, age / half_life_seconds)

        effective = base_confidence * decay_factor

        # Hard floor: below 0.1 is effectively expired
        if effective < 0.1:
            return 0.0

        return round(effective, 3)

    def compute_expiry(
        self,
        doc_type: str,
        observed_at: datetime,
        temporal_precision: str,
        base_confidence: float
    ) -> datetime:
        """Compute valid_until based on decay profile."""
        profile = self.get_profile(doc_type)

        if profile.hard_expiry == "never":
            return datetime(2099, 12, 31)  # sentinel for "never expires"

        hard_expiry = observed_at + self._parse_duration(profile.hard_expiry)

        # Lower precision = faster expiry
        precision_penalty = {
            "exact": 1.0,
            "day": 0.95,
            "week": 0.8,
            "month": 0.6,
            "quarter": 0.4,
            "approximate": 0.3,
            "inferred": 0.2,
        }

        penalty = precision_penalty.get(temporal_precision, 0.2)
        adjusted_duration = (hard_expiry - observed_at) * penalty

        return observed_at + adjusted_duration
```

### 9.4 Staleness Sweep

Periodic job (runs every 6 hours):
- Recomputes `effective_confidence` for all documents
- Documents below 0.3 effective confidence → marked stale
- Documents past `valid_until` → marked expired
- Expired documents → moved to archive (not deleted)
- Generates staleness report

```bash
mkb gc --sweep-stale                  # recompute, archive expired
mkb gc --stale-report                 # show what's decaying
mkb gc --stale-report --type project  # just projects
mkb gc --stale-report --threshold 0.5 # custom threshold
```

---

## 10. Built-in Document Schemas

### 10.1 Person

```yaml
# schemas/person.yaml
name: person
fields:
  name: { type: string, required: true, indexed: true, searchable: true }
  role: { type: string, indexed: true }
  team: { type: ref[], ref_type: team }
  email: { type: string, indexed: true, unique: true }
  reports_to: { type: ref, ref_type: person }
  influence: { type: enum, values: [low, medium, high, critical], indexed: true }
  trust_level: { type: enum, values: [unknown, low, medium, high] }
  communication_style: { type: string }
  agenda_notes: { type: string }
  last_interaction: { type: datetime, indexed: true }
```

### 10.2 Project

```yaml
# (defined in detail above in section 4.1)
```

### 10.3 Decision

```yaml
name: decision
fields:
  title: { type: string, required: true, searchable: true }
  decision_date: { type: date, required: true, indexed: true }
  status: { type: enum, values: [proposed, decided, reversed, superseded], indexed: true }
  outcome: { type: string }
  rationale: { type: string }
  participants: { type: ref[], ref_type: person }
  affects: { type: ref[], ref_type: project }
  alternatives_considered: { type: string[] }
  reversibility: { type: enum, values: [easily, with_effort, irreversible] }
```

### 10.4 Meeting

```yaml
name: meeting
fields:
  title: { type: string, required: true, searchable: true }
  meeting_date: { type: datetime, required: true, indexed: true }
  participants: { type: ref[], ref_type: person }
  type: { type: enum, values: [standup, 1on1, planning, review, adhoc, all_hands] }
  action_items: { type: json }   # [{owner, task, due_date, status}]
  decisions_made: { type: ref[], ref_type: decision }
  topics: { type: string[], searchable: true }
  sentiment: { type: enum, values: [positive, neutral, tense, negative] }
```

### 10.5 Signal (AI-Inferred)

```yaml
name: signal
fields:
  title: { type: string, required: true, searchable: true }
  signal_type:
    type: enum
    values: [sentiment, risk, relationship, blocker, opportunity,
             pattern, assumption, tension, capacity, alignment]
    required: true
    indexed: true
  severity: { type: enum, values: [low, medium, high, critical], indexed: true }
  affected_entities: { type: ref[] }
  evidence: { type: string }
  corroborated_by: { type: ref[], ref_type: signal }
  recommended_action: { type: string }
  acknowledged: { type: boolean, default: false }
  resolved: { type: boolean, default: false }
  first_detected: { type: datetime, indexed: true }
  last_seen: { type: datetime, indexed: true }
  occurrence_count: { type: integer, default: 1 }
```

### 10.6 Concept

```yaml
name: concept
fields:
  title: { type: string, required: true, searchable: true }
  domain: { type: string, indexed: true }
  definition: { type: string }
  related_to: { type: ref[], ref_type: concept }
  used_in: { type: ref[] }
  aliases: { type: string[], searchable: true }
```

---

## 11. Advanced Features

### 11.1 Views (Saved Queries)

```yaml
# .mkb/views/my-dashboard.yaml
name: my-dashboard
description: "VP Engineering operational dashboard"
refresh: 5m
sections:
  - name: "🔴 Red Projects"
    query: |
      SELECT title, owner, days_remaining, health
      FROM project
      WHERE health = "red" AND status = "in_progress"
      ORDER BY days_remaining ASC
  - name: "⚠️ Recent Signals"
    query: |
      SELECT title, signal_type, severity, confidence
      FROM signal
      WHERE FRESH(7d) AND severity IN ("high", "critical") AND resolved = false
      ORDER BY severity DESC, confidence DESC
  - name: "📅 Upcoming Decisions"
    query: |
      SELECT title, decision_date, participants
      FROM decision
      WHERE status = "proposed"
      ORDER BY decision_date ASC
  - name: "👥 Team Capacity"
    query: |
      SELECT p.name, COUNT(proj.id) AS active_projects,
             SUM(CASE WHEN proj.health = 'red' THEN 1 ELSE 0 END) AS red_count
      FROM person AS p
      LINK owns <- project AS proj WHERE status = "in_progress"
      GROUP BY p.name
      ORDER BY red_count DESC
```

### 11.2 Hooks & Automation

```bash
# hooks/post-ingest.sh
#!/bin/bash

# After any ingestion, check for new critical signals
NEW_SIGNALS=$(mkb q 'SELECT COUNT(*) FROM signal WHERE FRESH(1h) AND severity = "critical"' --format raw)
if [ "$NEW_SIGNALS" -gt 0 ]; then
    mkb q 'SELECT title, signal_type, evidence FROM signal WHERE FRESH(1h) AND severity = "critical"' \
        --format json | \
        notify-send "MKB: $NEW_SIGNALS critical signals detected"
fi

# Auto-link new documents
mkb link --auto --since 1h
```

### 11.3 LLM Context Assembly

```python
class ContextAssembler:
    """Build optimal LLM context from query results."""

    def assemble(
        self,
        results: List[Document],
        max_tokens: int = 4000,
        format: str = "full"
    ) -> str:
        # Priority: high confidence + high relevance + fresh
        ranked = self._rank_for_context(results)

        context_parts = []
        remaining_tokens = max_tokens

        for doc in ranked:
            rendered = self._render(doc, format, remaining_tokens)
            token_count = self._count_tokens(rendered)

            if token_count <= remaining_tokens:
                context_parts.append(rendered)
                remaining_tokens -= token_count
            else:
                # Try summary format as fallback
                summary = self._render(doc, "summary", remaining_tokens)
                if self._count_tokens(summary) <= remaining_tokens:
                    context_parts.append(summary)
                    remaining_tokens -= self._count_tokens(summary)
                break

        return "\n---\n".join(context_parts)
```

### 11.4 Temporal Query Operators

| Operator | Meaning | Example |
|----------|---------|---------|
| `FRESH(d)` | `observed_at` within last duration | `FRESH(7d)` |
| `STALE()` | effective confidence < 0.3 | `WHERE NOT STALE()` |
| `EXPIRED()` | past `valid_until` | `WHERE NOT EXPIRED()` |
| `CURRENT()` | `LATEST() AND NOT EXPIRED()` | `WHERE CURRENT()` |
| `LATEST()` | not superseded | `WHERE LATEST()` |
| `DURING(s,e)` | `observed_at` in range | `DURING("2025-01-01","2025-03-31")` |
| `OVERLAPS(s,e)` | `temporal_range` overlaps | `OVERLAPS("2025-02","2025-03")` |
| `AS OF dt` | snapshot at point in time | `AS OF "2025-01-15"` |
| `HISTORY` | include superseded versions | `WHERE id="x" HISTORY` |
| `AGE(f) > d` | older than duration | `AGE(observed_at) > 30d` |
| `EFF_CONFIDENCE() > n` | decay-adjusted confidence | `EFF_CONFIDENCE() >= 0.5` |

---

## 12. Temporal Invariants & Constraints

| # | Invariant | Enforced At | Consequence |
|---|-----------|-------------|-------------|
| T1 | `observed_at` is NEVER null | Temporal Gate (ingestion) | Document rejected |
| T2 | `valid_until` is NEVER null | Decay model (computed if not provided) | Auto-computed |
| T3 | `temporal_precision` is NEVER null | Schema validation | Defaults to `inferred` |
| T4 | `valid_until >= observed_at` | Schema validation | Error on save |
| T5 | `occurred_at <= observed_at` | Schema validation | Warning |
| T6 | Newer `observed_at` supersedes older for same entity+field | Contradiction resolver | Auto-supersession |
| T7 | `CURRENT()` excludes expired and superseded docs | Query engine | Default in most queries |
| T8 | Effective confidence decays with age | Decay model | Continuous recomputation |
| T9 | Links carry their own `observed_at` | Link schema | Temporal graph traversal |
| T10 | Rejection log preserves all rejected docs for recovery | Rejection handler | Nothing is silently lost |

---

## 13. Implementation Roadmap

### Phase 1: Core (Weeks 1–3)

- Vault directory structure + `.mkb.toml` config
- Schema definition + validation (with temporal fields)
- Markdown file CRUD (add, edit, rm)
- SQLite field index + FTS5 (with temporal indexes)
- MKQL parser + basic SELECT/WHERE/ORDER
- Temporal extraction pipeline (rejection gate)
- CLI: `init`, `add`, `edit`, `rm`, `query`, `search`

### Phase 2: Intelligence (Weeks 4–6)

- Vector embeddings + HNSW index
- NEAR() semantic search
- LINK graph traversal
- AI ingestion pipeline (explicit extraction)
- Temporal MKQL functions (FRESH, STALE, CURRENT, etc.)
- Decay model + staleness sweep
- Source adapters: file, clipboard, CSV (with temporal mapping)
- CLI: `ingest`, `link`, `schema`, `repl`, `gc --sweep-stale`

### Phase 3: Inference (Weeks 7–9)

- Implicit extraction (LLM-powered)
- Confidence scoring system
- Signal schema + detection
- Dedup & merge strategy
- Contradiction resolution via temporal ordering
- Corroboration engine
- Temporal query operators (AS OF, HISTORY, EFF_CONFIDENCE)
- CLI: `sync`, `gc --find-contradictions`, views

### Phase 4: Integration (Weeks 10–12)

- REST API server (`mkb serve`)
- Source adapters: Jira, Slack, Google Docs, Email (with temporal mappings)
- Hooks & automation
- Temporal analytics (freshness, timeline, decay reports)
- Dashboard views
- Context assembler for LLM pipelines (temporal-aware)
- Export & reporting

---

## 14. Technology Stack

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| Language | Rust (core) + Python (AI/LLM) | Performance + ML ecosystem |
| CLI Framework | `clap` (Rust) | Fast, typed argument parsing |
| Field Index | SQLite + FTS5 | Embedded, battle-tested, zero config |
| Vector Index | `hnswlib` / `usearch` | Fast ANN, memory-mapped |
| MKQL Parser | `pest` (Rust PEG) | Clean grammar, good errors |
| LLM Client | `anthropic` / `openai` Python SDK | Flexible provider support |
| Embeddings | `text-embedding-3-small` or `nomic-embed-text` | Quality/cost balance |
| File Watching | `notify` (Rust) | Cross-platform inotify/fsevents |
| Config | TOML | Human-readable, typed |
| Serialization | `serde` (Rust) | Fast YAML/JSON parsing |
| Testing | `cargo test` + `pytest` | Native test frameworks |

---

## 15. Example Session

```bash
$ mkb init --name "eng-leadership-kb" --template engineering
✓ Vault initialized at .mkb/
✓ 6 schemas created (person, project, decision, meeting, signal, concept)
✓ Indexes created

$ mkb ingest dir ~/exports/jira-q1/ --source jira --transform jira-to-project
▸ Processing 47 files...
✓ Created: 12 projects, 8 people (new), 3 decisions
✓ Updated: 5 projects (existing)
✓ Signals detected: 4 (2 risk, 1 capacity, 1 tension)
✓ Links created: 31

$ mkb ingest file ~/notes/standup-feb-10.md --type meeting --infer
✓ Created: meetings/2025-02-10-standup.md
✓ AI extracted: 3 action items, 5 participants linked
✓ Signal: capacity/high — "Sprint velocity declining across 2 teams"
✓ Signal: blocker/medium — "API dependency unresolved for 2 weeks"

$ mkb q 'SELECT title, health, owner FROM project WHERE status = "in_progress" ORDER BY health'
┌─────────────────────────────┬────────┬─────────────┐
│ title                       │ health │ owner       │
├─────────────────────────────┼────────┼─────────────┤
│ Search Relevance v3         │ red    │ adam-k      │
│ Mobile Checkout Redesign    │ yellow │ jane-smith  │
│ SEO Performance Audit       │ yellow │ tomas-rous  │
│ API Gateway Migration       │ green  │ josef-d     │
│ Coupon Engine Refactor      │ green  │ diana-sima  │
└─────────────────────────────┴────────┴─────────────┘
5 results (8ms)

$ mkb s --semantic "who on my team might be burning out?"
Found 3 relevant documents (semantic + signal correlation):

1. [signal] Team Capacity Alert: MBNXT App (confidence: 0.87)
   → Sprint velocity: 42 → 31 pts over 2 sprints
   → Jane mentioned "stretched thin" in 2 standups

2. [signal] Overtime Pattern: Platform Team (confidence: 0.79)
   → 3 late-night commits from Adam K. in past week
   → PR review backlog growing (12 → 23 open PRs)

3. [person] Adam Korinek — Engineering Manager (confidence: 0.73)
   → Owns 2 red/yellow projects simultaneously
   → Last 1:1 flagged "too many context switches"

$ mkb q 'SELECT * FROM project WHERE health = "red"' \
    --format context --max-tokens 2000 | \
  llm "Based on this project data, draft a 3-bullet executive summary
       of engineering risks for the weekly leadership update."
```

---

## 16. Temporal Grounding Summary

**Core Principle:** No information enters the vault without a timestamp. Ever.

**Key Mechanisms:**
- **Mandatory fields:** `observed_at`, `valid_until`, `temporal_precision` on every document
- **Rejection gate:** Documents without temporal grounding are rejected and logged
- **Decay model:** Information becomes less trustworthy over time based on type
- **Temporal ordering:** Newer observations automatically supersede older ones
- **Query operators:** `CURRENT()`, `STALE()`, `EXPIRED()`, `EFF_CONFIDENCE()`, `AS OF`, `HISTORY`

**Benefits:**
- Prevents stale information from polluting context
- Enables point-in-time queries and trend analysis
- Automatic contradiction resolution
- Confidence decay reflects real-world information aging

*Temporal grounding is not a feature. It is the foundation.*
*Without time, there is no truth — only noise.*

---

*MKB v1.0 — Designed for engineering leaders who think in systems.*
