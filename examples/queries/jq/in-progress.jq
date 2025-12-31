# Get all in-progress actions
# Usage: jq -f examples/queries/jq/in-progress.jq examples/sample.json

.actions[] | select(.state == "in_progress")
