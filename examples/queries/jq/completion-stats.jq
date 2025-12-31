# Get completion statistics by project/story
# Usage: jq -f examples/queries/jq/completion-stats.jq examples/sample.json

.actions
| group_by(.story // "No Project")
| map({
    story: .[0].story // "No Project",
    total: length,
    completed: map(select(.state == "completed")) | length,
    in_progress: map(select(.state == "in_progress")) | length,
    not_started: map(select(.state == "not_started")) | length,
    blocked: map(select(.state == "blocked")) | length
  })
| map(. + {
    completion_rate: (
      if .total > 0
      then (.completed / .total * 100 | round)
      else 0
      end
    )
  })
