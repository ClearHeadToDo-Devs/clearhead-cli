# SQL Query Guide

This guide shows how to use SQL queries to filter and search actions in clearhead_cli.

## Overview

The CLI supports three query modes:
1. **Simple WHERE clause** - For common filters
2. **Full SQL queries** - For complex queries with JOINs, CTEs, etc.
3. **Tree-sitter queries** - For pattern-based filtering (existing feature)

All SQL queries use an in-memory SQLite database that follows the canonical schema from the [actions specification](https://github.com/ClearHeadToDo-Devs/tree-sitter-actions/blob/main/docs/action_specification.md#sql-storage-schema).

## Quick Start

### Basic WHERE Queries

Use `--where` for simple filtering:

```bash
# All P1 actions
clearhead_cli read tasks.actions --where "priority = 1"

# All completed actions
clearhead_cli read tasks.actions --where "state = 'completed'"

# Actions without a parent (root actions)
clearhead_cli read tasks.actions --where "parent_id IS NULL"

# Multiple conditions
clearhead_cli read tasks.actions --where "priority = 1 AND state <> 'completed'"
```

### Querying by Context

To filter by context (tags), you need to JOIN with the `action_contexts` table:

```bash
# Actions in 'work' context
clearhead_cli read tasks.actions --sql "SELECT a.id FROM actions a \
  JOIN action_contexts c ON a.id = c.action_id \
  WHERE c.context = 'work'"

# Actions in 'work' OR 'urgent' context
clearhead_cli read tasks.actions --sql "SELECT DISTINCT a.id FROM actions a \
  JOIN action_contexts c ON a.id = c.action_id \
  WHERE c.context IN ('work', 'urgent')"
```

### Complex Queries

For advanced queries, use `--sql` with full SQL:

```bash
# P1 actions in work context
clearhead_cli read tasks.actions --sql "SELECT a.id FROM actions a \
  JOIN action_contexts c ON a.id = c.action_id \
  WHERE a.priority = 1 AND c.context = 'work'"

# All descendants of a specific story (recursive CTE)
clearhead_cli read tasks.actions --sql "WITH RECURSIVE descendants AS (
    SELECT * FROM actions WHERE story = 'Sprint 1'
    UNION ALL
    SELECT a.* FROM actions a
    JOIN descendants d ON a.parent_id = d.id
  )
  SELECT id FROM descendants"
```

## Database Schema

The in-memory database uses these tables:

### `actions` Table
```sql
CREATE TABLE actions (
    id TEXT PRIMARY KEY,
    parent_id TEXT REFERENCES actions(id),
    state TEXT CHECK(state IN ('not_started', 'completed', 'in_progress', 'blocked', 'cancelled')),
    name TEXT NOT NULL,
    description TEXT,
    priority INTEGER CHECK(priority >= 0),
    story TEXT,
    do_datetime TEXT,
    do_duration INTEGER,
    completed_datetime TEXT,
    depth INTEGER DEFAULT 0 CHECK(depth >= 0 AND depth <= 5)
);
```

### `action_contexts` Table
```sql
CREATE TABLE action_contexts (
    action_id TEXT NOT NULL REFERENCES actions(id),
    context TEXT NOT NULL,
    PRIMARY KEY (action_id, context)
);
```

### `action_recurrence` Table
```sql
CREATE TABLE action_recurrence (
    action_id TEXT PRIMARY KEY REFERENCES actions(id) ON DELETE CASCADE,
    frequency TEXT NOT NULL CHECK(frequency IN ('secondly', 'minutely', 'hourly', 'daily', 'weekly', 'monthly', 'yearly')),
    interval INTEGER DEFAULT 1 CHECK(interval >= 1),
    count INTEGER CHECK(count >= 1),
    until_date TEXT,
    by_second TEXT,
    by_minute TEXT,
    by_hour TEXT,
    by_day TEXT,
    by_month_day TEXT,
    by_year_day TEXT,
    by_week_no TEXT,
    by_month TEXT,
    by_set_pos TEXT,
    week_start TEXT DEFAULT 'MO' CHECK(week_start IN ('MO', 'TU', 'WE', 'TH', 'FR', 'SA', 'SU'))
);
```

### Indexes
Indexes are created on commonly-queried fields:
- `state`, `priority`, `story`, `do_datetime`, `parent_id`, `depth`, `context`

## Common Query Patterns

### By State

```bash
# Not started
clearhead_cli read tasks.actions --where "state = 'not_started'"

# In progress
clearhead_cli read tasks.actions --where "state = 'in_progress'"

# Blocked
clearhead_cli read tasks.actions --where "state = 'blocked'"
```

### By Priority

```bash
# High priority (P1)
clearhead_cli read tasks.actions --where "priority = 1"

# P1 or P2
clearhead_cli read tasks.actions --where "priority IN (1, 2)"

# Has any priority set
clearhead_cli read tasks.actions --where "priority IS NOT NULL"
```

### Hierarchical Queries

```bash
# Root actions only
clearhead_cli read tasks.actions --where "parent_id IS NULL"

# All children of a specific action
clearhead_cli read tasks.actions --where "parent_id = '<UUID>'"

# Actions with children (have at least one child)
clearhead_cli read tasks.actions --sql "SELECT DISTINCT p.id FROM actions p \
  JOIN actions c ON c.parent_id = p.id"

# Leaf actions (no children)
clearhead_cli read tasks.actions --sql "SELECT a.id FROM actions a \
  LEFT JOIN actions c ON c.parent_id = a.id \
  WHERE c.id IS NULL"
```

### By Story/Project

```bash
# All actions in a story
clearhead_cli read tasks.actions --where "story = 'Sprint 1'"

# Actions without a story
clearhead_cli read tasks.actions --where "story IS NULL"
```

### Advanced Patterns

#### Combining Multiple Contexts

```bash
# Actions that have BOTH 'work' AND 'urgent' contexts
clearhead_cli read tasks.actions --sql "SELECT action_id as id FROM action_contexts \
  WHERE context IN ('work', 'urgent') \
  GROUP BY action_id \
  HAVING COUNT(DISTINCT context) = 2"
```

#### Incomplete High-Priority Work

```bash
clearhead_cli read tasks.actions --sql "SELECT a.id FROM actions a \
  JOIN action_contexts c ON a.id = c.action_id \
  WHERE a.priority = 1 \
    AND a.state <> 'completed' \
    AND c.context = 'work'"
```

#### Actions Without Context Tags

```bash
clearhead_cli read tasks.actions --sql "SELECT a.id FROM actions a \
  LEFT JOIN action_contexts c ON a.id = c.action_id \
  WHERE c.context IS NULL"
```

## Tips & Tricks

### Shell Quoting

Use proper quoting to avoid shell interpretation issues:

```bash
# Good - use double quotes for whole query, single quotes for strings
clearhead_cli read tasks.actions --where "priority = 1 AND state = 'completed'"

# Good - use <> instead of != to avoid shell issues
clearhead_cli read tasks.actions --where "state <> 'completed'"

# Bad - shell interprets ! as history expansion
clearhead_cli read tasks.actions --where "state != 'completed'"
```

### Debugging Queries

Use `--debug` to see what's happening:

```bash
clearhead_cli --debug read tasks.actions --where "priority = 1"
```

### Output Formats

Combine SQL queries with different output formats:

```bash
# JSON output
clearhead_cli read tasks.actions --where "priority = 1" --format json

# Table format (default)
clearhead_cli read tasks.actions --where "priority = 1" --format table

# Actions format (for further processing)
clearhead_cli read tasks.actions --where "priority = 1" --format actions
```

## Examples from the Spec

These examples are from the [official specification](https://github.com/ClearHeadToDo-Devs/tree-sitter-actions/blob/main/docs/action_specification.md#example-queries):

### Basic Filters

```bash
# All P1 actions
clearhead_cli read tasks.actions --where "priority = 1"

# All completed actions
clearhead_cli read tasks.actions --where "state = 'completed'"

# Actions in 'work' context
clearhead_cli read tasks.actions --sql "SELECT a.id FROM actions a \
  JOIN action_contexts c ON a.id = c.action_id \
  WHERE c.context = 'work'"
```

### Date Queries

```bash
# Actions due today (SQLite date functions)
clearhead_cli read tasks.actions --where "date(do_datetime) = date('now')"

# Overdue not-started actions
clearhead_cli read tasks.actions --where "state = 'not_started' \
  AND do_datetime < datetime('now')"
```

### Aggregate Queries

```bash
# Count by state
clearhead_cli read tasks.actions --sql "SELECT state, COUNT(*) as count \
  FROM actions GROUP BY state"

# Priority distribution
clearhead_cli read tasks.actions --sql "SELECT \
  COALESCE(priority, 999) as priority, \
  COUNT(*) as count \
  FROM actions \
  GROUP BY priority \
  ORDER BY priority"
```

Note: Aggregate queries return IDs, so they may not show meaningful data in the default format. Consider using `--format json` for aggregate results.

## Comparison with Tree-sitter Queries

**Use SQL when:**
- You need complex filtering (multiple conditions, JOINs)
- You want to query by relationships (descendants, ancestors)
- You need aggregations or statistics
- You're familiar with SQL

**Use Tree-sitter queries when:**
- You need pattern matching on the syntax structure
- You're filtering by action text patterns
- You want built-in query templates (e.g., `--query p1`)

**Example:**

```bash
# SQL - precise, flexible
clearhead_cli read tasks.actions --where "priority = 1 AND state = 'completed'"

# Tree-sitter - simpler for basic cases
clearhead_cli read tasks.actions --query p1
```

## Further Reading

- [Actions Specification - SQL Section](https://github.com/ClearHeadToDo-Devs/tree-sitter-actions/blob/main/docs/action_specification.md#sql-storage-schema)
- [SQLite Date Functions](https://www.sqlite.org/lang_datefunc.html)
- [SQLite CTE Documentation](https://www.sqlite.org/lang_with.html)
