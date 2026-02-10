# MKQL Quick Reference

## SELECT

```sql
SELECT field1, field2      FROM type     WHERE condition
SELECT *                   FROM *        WHERE BODY CONTAINS "text"
SELECT COUNT(*), field     FROM type     GROUP BY field
SELECT *, RELEVANCE        FROM type     WHERE NEAR("semantic query", 0.7)
```

## Operators

| Op | Example |
|----|---------|
| `=`, `!=`, `>`, `<`, `>=`, `<=` | `priority = "P1"` |
| `IN (...)` | `status IN ("active", "blocked")` |
| `LIKE` | `title LIKE "%mobile%"` |
| `CONTAINS` | `tags CONTAINS "ux"` |
| `MATCHES` | `title MATCHES "^API.*v\d+"` |
| `IS NULL` / `IS NOT NULL` | `owner IS NOT NULL` |
| `BODY CONTAINS` | `BODY CONTAINS "cart abandonment"` |
| `NEAR(text, threshold)` | `NEAR("burnout signals", 0.75)` |
| `FRESH(duration)` | `FRESH(7d)` |
| `LINKED(rel -> type)` | `LINKED(owns -> project)` |
| `IMPLICIT(signal_type)` | `IMPLICIT("risk")` |

## LINK (Graph Traversal)

```sql
-- Forward: docs this links TO
SELECT p.title, o.name
FROM project AS p
LINK owner -> person AS o

-- Reverse: docs that link TO this
SELECT person.name
FROM person
LINK owns <- project WHERE health = "red"
```

## Aggregations

```sql
SELECT health, COUNT(*) FROM project GROUP BY health
SELECT AVG(confidence), provenance FROM signal GROUP BY provenance
SELECT UNNEST(tags) AS t, COUNT(*) FROM * GROUP BY t ORDER BY COUNT(*) DESC
```

## Context Assembly

```sql
SELECT * FROM project, signal
WHERE FRESH(7d) AND health != "green"
CONTEXT WINDOW 4000 FORMAT full    -- token-budgeted LLM context
CONTEXT FORMAT summary             -- compact cards
CONTEXT FORMAT frontmatter         -- metadata only
```

## Temporal

```sql
SELECT * FROM project WHERE id = "x" AS OF "2025-01-15"
SELECT modified, health FROM project WHERE id = "x" HISTORY
```

## CLI Shortcuts

```bash
mkb q 'QUERY'                    # run query
mkb q 'QUERY' --format json      # output as JSON
mkb q 'QUERY' --explain          # show execution plan
mkb s "text"                     # quick FTS search
mkb s --semantic "meaning"       # vector search
mkb s --natural "plain english"  # AI translates to MKQL
mkb repl                         # interactive shell
```
