# Get all actions due today
# Usage: jq -f examples/queries/jq/due-today.jq examples/sample.json
# Note: This example uses a hardcoded date - in practice you'd use: --arg today "$(date +%Y-%m-%d)"

.actions[] | select(
  .doDate.datetime? // "" | startswith("2025-01-16")
)
