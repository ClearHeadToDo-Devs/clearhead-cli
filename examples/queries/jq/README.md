# jq Query Examples

This directory contains example `jq` queries for filtering and transforming `.actions` JSON data.

## Prerequisites

These queries work on JSON output that follows the canonical JSON schema (`schema/actions.schema.json`).

```bash
# Install jq if you don't have it
# macOS: brew install jq
# Linux: apt-get install jq / yum install jq
```

## Usage

### Basic Queries

**Priority 1 actions:**
```bash
jq -f examples/queries/jq/p1-actions.jq examples/sample.json
```

**Completed actions:**
```bash
jq -f examples/queries/jq/completed-actions.jq examples/sample.json
```

**In-progress actions:**
```bash
jq -f examples/queries/jq/in-progress.jq examples/sample.json
```

**Blocked actions:**
```bash
jq -f examples/queries/jq/blocked-actions.jq examples/sample.json
```

### Queries with Parameters

**Actions in a specific context:**
```bash
jq -f examples/queries/jq/by-context.jq --arg ctx "work" examples/sample.json
```

**Actions in a specific story/project:**
```bash
jq -f examples/queries/jq/by-story.jq --arg story "Weekly Errands" examples/sample.json
```

**Actions due today:**
```bash
# With dynamic date
jq -f examples/queries/jq/due-today.jq --arg today "$(date +%Y-%m-%d)" examples/sample.json

# Or modify the query inline for a specific date
jq '.actions[] | select(.doDate.datetime? // "" | startswith("2025-01-16"))' examples/sample.json
```

### Aggregate Queries

**Completion statistics by project:**
```bash
jq -f examples/queries/jq/completion-stats.jq examples/sample.json
```

Output:
```json
[
  {
    "story": "Weekly Errands",
    "total": 1,
    "completed": 1,
    "in_progress": 0,
    "not_started": 0,
    "blocked": 0,
    "completion_rate": 100
  },
  ...
]
```

**Priority summary:**
```bash
jq -f examples/queries/jq/priority-summary.jq examples/sample.json
```

### Structural Queries

**Actions with children:**
```bash
jq -f examples/queries/jq/with-children.jq examples/sample.json
```

**Flatten all actions (including nested children):**
```bash
jq -f examples/queries/jq/flatten-all.jq examples/sample.json
```

## Common Patterns

### Combining Filters

**P1 actions in "work" context:**
```bash
jq '.actions[] | select(.priority == 1 and (.contexts // [] | contains(["work"])))' examples/sample.json
```

**Not-started actions due this week:**
```bash
jq --arg week_start "2025-01-13" --arg week_end "2025-01-20" '
  .actions[] | select(
    .state == "not_started" and
    (.doDate.datetime? // "" >= $week_start) and
    (.doDate.datetime? // "" < $week_end)
  )
' examples/sample.json
```

### Formatting Output

**Just the action names:**
```bash
jq -r '.actions[] | select(.priority == 1) | .name' examples/sample.json
```

**CSV output:**
```bash
jq -r '.actions[] | [.name, .priority, .state] | @csv' examples/sample.json
```

**Table format:**
```bash
jq -r '.actions[] | "\(.name)\t\(.priority // "none")\t\(.state)"' examples/sample.json | column -t
```

### Transformations

**Extract just names and priorities:**
```bash
jq '.actions | map({name, priority})' examples/sample.json
```

**Add a computed field:**
```bash
jq '.actions | map(. + {has_deadline: (.doDate != null)})' examples/sample.json
```

**Sort by priority then name:**
```bash
jq '.actions | sort_by(.priority, .name)' examples/sample.json
```

## Integration with Tree-Sitter Parser

If you have a tool that converts `.actions` files to JSON:

```bash
# Hypothetical workflow
actions-to-json tasks.actions | jq -f examples/queries/jq/p1-actions.jq

# Or save intermediate JSON
actions-to-json tasks.actions > tasks.json
jq -f examples/queries/jq/completion-stats.jq tasks.json
```

## Tips

1. **Use `-r` for raw output** (no quotes around strings):
   ```bash
   jq -r '.actions[].name' examples/sample.json
   ```

2. **Use `-c` for compact output** (one JSON object per line):
   ```bash
   jq -c '.actions[]' examples/sample.json
   ```

3. **Pretty print with colors**:
   ```bash
   jq '.' examples/sample.json  # Default is colored and pretty
   ```

4. **Validate JSON against schema**:
   ```bash
   # Using ajv-cli or similar
   ajv validate -s schema/actions.schema.json -d examples/sample.json
   ```

## When to Use jq vs. Other Approaches

**Use jq when:**
- You need ad-hoc queries and one-off data analysis
- You're building Unix-style pipelines
- You want stateless, functional transformations
- File changes frequently (no persistent storage needed)

**Consider alternatives when:**
- **Simple pattern matching** → Use [tree-sitter queries](../../../queries/actions/) for fast filtering without JSON conversion
- **Persistent storage** → Use [SQL](../sql/) for databases, indexes, and multi-user access
- **Complex relational queries** → Use [SQL](../sql/) for joins, aggregations with performance at scale
- **Editor features** → Use [tree-sitter queries](../../../queries/actions/) for syntax highlighting and folding

See the [main README](../../../README.md#querying-actions) for a complete comparison of query approaches.

## Further Reading

- [jq Manual](https://jqlang.github.io/jq/manual/)
- [jq Cookbook](https://github.com/stedolan/jq/wiki/Cookbook)
- [JSON Schema](../../../schema/actions.schema.json)
- [Tree-sitter Queries](../../../queries/actions/) - For simpler pattern matching
- [SQL Queries](../sql/) - For persistent storage and complex queries
