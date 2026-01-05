# clearhead_cli

**Command Line Interface for the ClearHead framework**

A fast, flexible task manager that uses plain text `.actions` files - edit them with any text editor, manage them with `clearhead_cli`.

## Quick Start

### Installation

```bash
cargo install clearhead_cli
```

### Your First Task List

Create a file called `inbox.actions` (this is created for you in the default location otherwise):

```
[ ] Welcome to clearhead_cli $Your personal task manager
> [ ] Try different output formats $Run: clearhead_cli read --format table
> [x] Get started $You're already using it!
```

View it in different formats:

```bash
# Table view (great for browsing)
clearhead_cli read inbox.actions --format table

# JSON (great for scripting)
clearhead_cli read inbox.actions --format json

# Original format (round-trip safe)
clearhead_cli read inbox.actions
```

## Features

- **SQL queries**: Filter actions with WHERE clauses or full SQL (JOINs, CTEs, aggregations)
- **Multiple output formats**: actions, json, xml, table
- **Zero lock-in**: Plain text files, use any editor

## Configuration

Optionally create `~/.config/clearhead/config.json`:

```json
{
  "cli_format": "table",
  "default_file": "inbox.actions"
}
```

Data is stored in `~/.local/share/clearhead/` by default (respects XDG environment variables).

Override with environment variables:

```bash
CLEARHEAD_CLI_FORMAT=json clearhead_cli read
```

Or command-line arguments (highest priority):

```bash
clearhead_cli read --format table
```

## Action Syntax

For the full specification see [The Specification](https://github.com/ClearHeadToDo-Devs/tree-sitter-actions/blob/master/docs/action_specification.md)
```
[x] Completed Action $with description !1 +context,tags
> [ ] Child Action
>> [-] Grandchild in progress
```

**States:**
- `[ ]` Not started
- `[x]` Completed
- `[-]` In progress
- `[=]` Blocked/waiting
- `[_]` Cancelled

**Metadata:**
- `$description` - Task description
- `!N` - Priority (1-5)
- `+tag,tag` - Context tags
- `#uuid` - Unique ID (auto-generated if omitted)
- `*story` - Story/project (root actions only)

## Commands

### Read

```bash
# Read default file (~/.local/share/clearhead/inbox.actions)
clearhead_cli read

# Read specific file
clearhead_cli read ~/work.actions

# Output as JSON
clearhead_cli read --format json

# Output as table
clearhead_cli read --format table
```

**Filtering with SQL queries:**

```bash
# Simple WHERE clause
clearhead_cli read --where "priority = 1"
clearhead_cli read --where "state = 'completed'"

# Query by context (requires JOIN)
clearhead_cli read --sql "SELECT a.id FROM actions a \
  JOIN action_contexts c ON a.id = c.action_id \
  WHERE c.context = 'work'"

# Complex queries with recursive CTEs
clearhead_cli read --sql "WITH RECURSIVE descendants AS (
    SELECT * FROM actions WHERE story = 'Sprint 1'
    UNION ALL
    SELECT a.* FROM actions a JOIN descendants d ON a.parent_id = d.id
  ) SELECT id FROM descendants"
```

See [docs/SQL_QUERIES.md](docs/SQL_QUERIES.md) for the complete guide to SQL queries.

### Advanced Workflows

**Formatting**
Format `.actions` files with proper spacing (preserves existing UUIDs).
```bash
# Format a file with default compact style
clearhead_cli format ~/work.actions --write

# Preview formatted output without writing
clearhead_cli format ~/work.actions

# Format with specific style
clearhead_cli format ~/work.actions --style compact --write
```

**Normalization**
Ensure all actions have UUIDs (formats by default for clean output).
```bash
# Add UUIDs and format
clearhead_cli normalize ~/work.actions --write

# Add UUIDs without formatting
clearhead_cli normalize ~/work.actions --no-format --write
```

**Archiving**
Move completed action trees to monthly log files (e.g., `logs/2026-01.actions`). 
Note: Only entire trees (parent and all descendants) are moved, and only if every action in the tree is completed.
```bash
# Archive completed actions from default file
clearhead_cli archive

# Archive from specific file
clearhead_cli archive ~/work.actions

# Preview what would be archived
clearhead_cli archive --dry-run
```

**Linting**
Check your files for syntax errors, missing IDs, or convention violations.
```bash
# Lint a specific file
clearhead_cli lint ~/work.actions

# Lint output from another command
cat ~/work.actions | clearhead_cli lint
```

### Patching (Smart Sync)
Update a Primary file based on a modified Secondary view (even if lines were reordered).
```bash
# Apply changes from a temp file back to the source of truth
clearhead_cli patch --primary ~/work.actions --secondary ~/tmp/filtered_view.actions --write
```
This is the engine that powers editor plugins, allowing you to filter/sort a view, edit it, and save the changes back to the original file safely.

## Editor Integration

### Neovim
The official [clearhead.nvim](https://github.com/ClearHeadToDo-Devs/clearhead.nvim) plugin provides:
- Automatic LSP setup (diagnostics, code actions, inlay hints)
- Syntax highlighting
- State cycling and normalization commands

### Built-in LSP
The CLI includes a built-in Language Server. To use it with any LSP-compatible editor:
```bash
clearhead_cli lsp
```
See [docs/LSP.md](docs/LSP.md) for configuration details.

## Development

See [CONTRIBUTORS.md](docs/CONTRIBUTORS.md) for:
- Architecture overview
- How to add features
- Testing guidelines
- Code organization

## Philosophy

**Data-centric**: Plain text files, no databases, no lock-in
**Composable**: Use with grep, sed, jq, or any text tool
**Standards-based**: XDG directories, tree-sitter parsing
**Functional**: Pure functions, immutable data where possible

## Status

**Current:** Read command with multi-format output
**Next:** Create, Update, Delete commands
**Future:** TUI interface, collaborative editing (CRDTs)

## License

MIT
