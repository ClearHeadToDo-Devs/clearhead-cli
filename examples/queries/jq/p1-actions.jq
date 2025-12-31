# Get all priority 1 actions
# Usage: jq -f examples/queries/jq/p1-actions.jq examples/sample.json

.actions[] | select(.priority == 1)
