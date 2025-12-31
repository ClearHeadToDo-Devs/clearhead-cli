# Summary of actions grouped by priority
# Usage: jq -f examples/queries/jq/priority-summary.jq examples/sample.json

.actions
| group_by(.priority // 999)
| map({
    priority: (.[0].priority // "none"),
    count: length,
    actions: map(.name)
  })
| sort_by(.priority)
