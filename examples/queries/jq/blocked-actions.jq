# Get all blocked actions
# Usage: jq -f examples/queries/jq/blocked-actions.jq examples/sample.json

.actions[] | select(.state == "blocked")
