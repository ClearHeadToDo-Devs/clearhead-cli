# Flatten all actions (including children) into a single array
# Usage: jq -f examples/queries/jq/flatten-all.jq examples/sample.json

def flatten_actions:
  .actions as $root_actions |
  [
    $root_actions[] |
    . as $action |
    ($action | del(.children)),
    (if $action.children then $action.children[] else empty end)
  ];

flatten_actions
