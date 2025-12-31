# Get all actions that have children
# Usage: jq -f examples/queries/jq/with-children.jq examples/sample.json

.actions[] | select(.children and (.children | length) > 0)
