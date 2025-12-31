# Get all actions in a specific context
# Usage: jq -f examples/queries/jq/by-context.jq --arg ctx "work" examples/sample.json
# Or inline: jq '.actions[] | select(.contexts // [] | contains(["work"]))' examples/sample.json

.actions[] | select(.contexts // [] | contains([$ctx]))
