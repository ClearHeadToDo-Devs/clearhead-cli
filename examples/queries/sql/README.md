# SQL Query Examples

This directory contains SQL query examples for working with the canonical `.actions` SQL schema.

## Prerequisites

- SQLite 3.x (or PostgreSQL, MySQL with adaptations)
- Database created using `schema/actions.sql`
- Data imported from JSON or `.actions` files

## Setup

### 1. Create the database schema

```bash
# SQLite
sqlite3 actions.db < schema/actions.sql

# PostgreSQL
psql -d yourdb -f schema/actions.sql

# MySQL
mysql -u user -p yourdb < schema/actions.sql
```

### 2. Import data

See `import-json.sql` for examples of loading JSON data into the schema.

```bash
sqlite3 actions.db < examples/queries/sql/import-json.sql
```

## Query Files

### `common-queries.sql`

Contains frequently-used queries organized by category:

- **Basic Filters** - By state, priority, context
- **Date-Based Queries** - Due today, this week, overdue
- **Story/Project Queries** - Filter and aggregate by project
- **Hierarchical Queries** - Parents, children, tree traversal
- **Aggregate Queries** - Statistics, completion rates, distributions
- **Complex Combined Queries** - Multi-criteria filters
- **Recurring Actions** - Query recurrence rules
- **Reporting Queries** - Dashboard summaries, top lists

### `import-json.sql`

Shows how to import JSON data (conforming to `schema/actions.schema.json`) into the SQL schema.

## Usage Examples

### Basic Queries

**Find all P1 actions:**
```bash
sqlite3 actions.db "SELECT * FROM actions WHERE priority = 1;"
```

**Find actions in 'work' context:**
```bash
sqlite3 actions.db "SELECT a.* FROM actions a JOIN action_contexts c ON a.id = c.action_id WHERE c.context = 'work';"
```

**Actions due today:**
```bash
sqlite3 actions.db "SELECT * FROM actions WHERE date(do_datetime) = date('now');"
```

### Running Query Files

**Execute all common queries:**
```bash
sqlite3 actions.db < examples/queries/sql/common-queries.sql
```

**Extract specific queries:**
```bash
# Get just the P1 actions query
grep -A 1 "All priority 1 actions" examples/queries/sql/common-queries.sql | tail -1 | sqlite3 actions.db
```

### Interactive Exploration

```bash
# Start interactive session
sqlite3 actions.db

# SQLite commands:
.tables              # List all tables
.schema actions      # Show table schema
.mode column         # Pretty column output
.headers on          # Show column headers

# Run queries
SELECT * FROM actions WHERE priority = 1;

# Use views
SELECT * FROM root_actions WHERE state = 'not_started';
```

## Common Patterns

### Filtering and Combining

**P1 actions in 'work' context due this week:**
```sql
SELECT a.* FROM actions a
JOIN action_contexts c ON a.id = c.action_id
WHERE a.priority = 1
  AND c.context = 'work'
  AND a.do_datetime >= date('now', 'weekday 0', '-7 days')
  AND a.do_datetime < date('now', 'weekday 0');
```

**Not-started actions without children (leaf tasks):**
```sql
SELECT a.*
FROM actions a
LEFT JOIN actions c ON c.parent_id = a.id
WHERE a.state = 'not_started'
  AND c.id IS NULL;
```

### Aggregation and Reporting

**Completion rate by project:**
```sql
SELECT
  story,
  COUNT(*) as total,
  SUM(CASE WHEN state = 'completed' THEN 1 ELSE 0 END) as completed,
  ROUND(100.0 * SUM(CASE WHEN state = 'completed' THEN 1 ELSE 0 END) / COUNT(*), 2) as rate
FROM actions
WHERE story IS NOT NULL AND depth = 0
GROUP BY story
ORDER BY rate DESC;
```

**Dashboard summary:**
```sql
SELECT
  (SELECT COUNT(*) FROM actions WHERE state = 'not_started') as not_started,
  (SELECT COUNT(*) FROM actions WHERE state = 'in_progress') as in_progress,
  (SELECT COUNT(*) FROM actions WHERE state = 'completed') as completed,
  (SELECT COUNT(*) FROM actions WHERE priority = 1) as p1_count;
```

### Tree Traversal

**All descendants of an action:**
```sql
WITH RECURSIVE descendants AS (
  SELECT * FROM actions WHERE id = 'parent-action-id'
  UNION ALL
  SELECT a.* FROM actions a
  JOIN descendants d ON a.parent_id = d.id
)
SELECT * FROM descendants WHERE id != 'parent-action-id';
```

**All ancestors (path to root):**
```sql
WITH RECURSIVE ancestors AS (
  SELECT * FROM actions WHERE id = 'child-action-id'
  UNION ALL
  SELECT a.* FROM actions a
  JOIN ancestors d ON d.parent_id = a.id
)
SELECT * FROM ancestors WHERE id != 'child-action-id';
```

## Output Formats

### CSV Export

```bash
# SQLite
sqlite3 -csv actions.db "SELECT * FROM actions WHERE priority = 1;" > p1-actions.csv

# With headers
sqlite3 -header -csv actions.db "SELECT name, priority, state FROM actions;" > actions.csv
```

### JSON Export

```bash
# SQLite (3.33+)
sqlite3 -json actions.db "SELECT * FROM actions WHERE priority = 1;"

# Or using jq for formatting
sqlite3 -json actions.db "SELECT * FROM actions;" | jq .
```

### HTML Table

```bash
sqlite3 -html actions.db "SELECT * FROM actions WHERE priority = 1;" > report.html
```

## Integration with Other Tools

### Combining with jq

```bash
# Export to JSON, filter with jq, re-import
sqlite3 -json actions.db "SELECT * FROM actions;" | \
  jq '.[] | select(.priority == 1)' | \
  # ... process and re-import
```

### Combining with tree-sitter

```bash
# Hypothetical workflow:
# 1. Parse .actions file with tree-sitter
# 2. Convert to JSON
# 3. Import to SQL
# 4. Query with SQL
# 5. Export results

tree-sitter parse tasks.actions | \
  actions-to-json | \
  sqlite3 actions.db < import-json.sql
```

## Database-Specific Notes

### SQLite

- Date functions: `date()`, `datetime()`, `julianday()`
- JSON support: `json_extract()`, `json_each()`
- Recursive CTEs: `WITH RECURSIVE`
- Full-text search: `CREATE VIRTUAL TABLE ... USING fts5`

### PostgreSQL

- Use `TIMESTAMP` instead of `TEXT` for dates
- Native array types: `TEXT[]` for contexts
- JSONB for recurrence fields
- Better full-text search with `tsvector`
- Window functions for advanced analytics

### MySQL

- Use `DATETIME` for date fields
- JSON type for recurrence fields
- `ENUM` type for state (optional)
- Different date functions: `CURDATE()`, `DATE_ADD()`

## Performance Tips

1. **Use indexes** - The schema includes indexes on commonly-queried fields
2. **EXPLAIN queries** - Use `EXPLAIN QUERY PLAN` to check index usage
3. **Limit results** - Use `LIMIT` for large datasets
4. **Avoid SELECT \*** - Select only needed columns
5. **Use views** - Pre-defined views like `root_actions` can simplify queries

## Validation Queries

**Check data integrity:**
```sql
-- Orphaned children (should be 0)
SELECT COUNT(*) FROM actions a
WHERE a.parent_id IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM actions p WHERE p.id = a.parent_id);

-- Invalid depth values (should be 0)
SELECT COUNT(*) FROM actions
WHERE depth < 0 OR depth > 5;

-- Root actions with story at wrong depth (should be 0)
SELECT COUNT(*) FROM actions
WHERE story IS NOT NULL AND depth != 0;
```

## When to Use SQL vs. Other Approaches

**Use SQL when:**
- Building an application that needs persistent storage
- Querying the same dataset repeatedly
- Need complex joins, aggregations, or multi-criteria filters
- Dataset is large (1000+ actions) and needs indexed performance
- Multiple users need concurrent access

**Consider alternatives when:**
- **Quick one-off queries** → Use [jq](../jq/) for stateless filtering without database setup
- **Simple pattern matching** → Use [tree-sitter queries](../../../queries/actions/) for fast filtering on plaintext
- **Pipeline processing** → Use [jq](../jq/) for composable Unix-style transformations
- **Editor features** → Use [tree-sitter queries](../../../queries/actions/) for syntax highlighting

**You can combine approaches:**
```bash
# Parse → jq filter → SQL import
actions-to-json tasks.actions | \
  jq '.actions[] | select(.priority == 1)' | \
  sqlite3 myapp.db < import-filtered.sql
```


## Further Reading

- `schema/actions.sql` - Complete schema definition
- `docs/action_specification.md` - Full specification with SQL section
- [jq Query Examples](../jq/) - For ad-hoc JSON processing
- [Tree-sitter Queries](../../../queries/actions/) - For simple pattern matching
