# Get all actions in a specific story/project
# Usage: jq -f examples/queries/jq/by-story.jq --arg story "Weekly Errands" examples/sample.json

.actions[] | select(.story == $story)
