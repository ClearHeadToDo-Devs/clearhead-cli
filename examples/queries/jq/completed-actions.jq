# Get all completed actions
# Usage: jq -f examples/queries/jq/completed-actions.jq examples/sample.json

.actions[] | select(.state == "completed")
