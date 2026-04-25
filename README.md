# clearhead_cli

**Command Line Interface for the ClearHead framework**

A fast, flexible task manager that uses plain text `.actions` files - edit them with any text editor, manage them with `clearhead_cli`.

This is the Command Line Interface (CLI) as well as Language Server Protocol (LSP) server for the ClearHead intention management framework. It provides powerful tools to read, write, query, and manipulate your plan data stored in plain text files.

Importantly, this tools attempts to adhere to the [ClearHead Process](https://github.com/ClearHeadToDo-Devs/specifications/blob/master/process.md) so be sure to review that for the larger "what" behind the structure

### Installation

```bash
cargo install clearhead_cli
```

### Your First Plan List

Create a file called `inbox.actions` (this is created for you in the default location otherwise):

```actions
[ ] Buy groceries !2 +errands
[ ] Finish project report @2026-01-25T17:00 D120 !1 +work
[ ] Call Alice @2026-01-21T15:00 D30 !3 +personal
```

Maybe you have some work that you want to break down so you make a `work.md` charter file with the following content:

```markdown
---
id: 123e4567-e89b-12d3-a456-426614174000
objectives: [launch_v1]
alias: work
---
# Work projects charter
all the things i need to capture for work go in this charter
```

And an objective file to capture that objective:

```markdown
---
id: 123e4567-e89b-12d3-a456-426614
target date: 2026-02-01
---
# Launch v1 of our new app
This objective captures the high-level goal of launching version 1 of our app by February 1st, 2026. The associated charter (`work.md`) will contain all the related tasks and projects that contribute to this objective.
```

unless otherwise specified, the filename of the charter/objective also serve as the alias from a reference standpoint

View it in different formats:

```bash
# Read entire workspace (all .actions, charter, and objectives files)
clearhead_cli read

# Table view (great for browsing)
clearhead_cli read --format table

# JSON (great for scripting)
clearhead_cli read --format json

# Read a specific file
clearhead_cli read file inbox.actions
```

## Features

- **CRDT-Based Sync**: CRDTs allow for us to merge changes from multiple devices/editors without conflicts
- **Calendar export**: Export planned acts to iCalendar (`.ics`) for calendar tooling interoperability
- **SPARQL queries**: Filter actions with WHERE clauses or full SPARQL queries for powerful all-graph filtering
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

For more details see [The Specification](https://github.com/ClearHeadToDo-Devs/tree-sitter-actions/blob/master/docs/action_specification.md)
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
- `$description$` - Planned act description
- `!N` - Priority (1-4, where 1 is highest)
- `+tag,tag` - Context tags
- `@YYYY-MM-DDTHH:MM` - Do date/time (when task should be done)
- `DN` - Duration in minutes (e.g., `D30` for 30 minutes)
- `%YYYY-MM-DDTHH:MM` - Completed date/time
- `^YYYY-MM-DDTHH:MM` - Created date/time
- `#uuid` - Unique ID (auto-generated if omitted)
- `*story` - Story/project (root actions only)

Note: recurrence/schedule definitions are represented in `.ics` schedules, not in `.actions` lines.

## Commands

Commands follow a verb-noun structure, such that you can operate on different entities (plans, planned acts, charters, objectives) with the same command. here we will cover the nouns in general, and you may assume that this can be used for any entities in the system unless otherwise specified

### Read

The `read` command operates in three modes:

```bash
# Workspace-wide read (default) - reads ALL .actions,charter, objectives files from data directory
clearhead_cli read

# Read specific file
clearhead_cli read file ~/work.actions

# Read from stdin
cat tasks.actions | clearhead_cli read stdio
```

**Output formats:**

```bash
clearhead_cli read --format table   # Table view (great for browsing)
clearhead_cli read --format actions # Original format (default)
clearhead_cli read --format json    # JSON (great for scripting)
clearhead_cli read --format xml     # XML format
clearhead_cli read --format calendar # iCalendar format (for actions with due dates)
```

### Add

Add a new schedule plan to an `.ics` file. Generated or manually managed planned acts live in `.actions` files.

```bash
# Add a one-off scheduled plan
clearhead_cli add plan "Buy groceries" --scheduled-at "2026-04-28T10:00:00-07:00"

# Add with metadata
clearhead_cli add plan "Weekly review" --scheduled-at "2026-04-28T10:00:00-07:00" --rrule "FREQ=WEEKLY" --context work --description "Check logs"
```

### Complete

Mark a planned act as completed. Plans are schedules and do not have completion state.

```bash
# Complete by name
clearhead_cli complete act "Buy groceries"

# Complete by UUID prefix
clearhead_cli complete act 019baae
```

**Schedule-Generated Acts:**
When you complete an act generated from a schedule, ClearHead records completion on that act instance. Future instances are produced by schedule expansion from `.ics` sources.


### Format

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

### Normalization**
Ensure all actions have UUIDs (formats by default for clean output).
```bash
# Add UUIDs and format
clearhead_cli normalize ~/work.actions --write

# Add UUIDs without formatting
clearhead_cli normalize ~/work.actions --no-format --write
```


**Linting**
Check your files for syntax errors, missing IDs, or convention violations.
```bash
# Lint a specific file
clearhead_cli lint ~/work.actions

# Lint output from another command
cat ~/work.actions | clearhead_cli lint
```


### System Logging (Operational)

ClearHead uses tiered logging for full transparency:
1. **Data Level**: `actions.oxigraph` stores semantic user actions (Audit trail).
2. **Application Level**: Standard structured logs (via `tracing`) are emitted to `stderr` for system loggers like `journald`.

Use the `-v`, `-vv`, or `-vvv` flags to increase CLI verbosity.

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

**Current:** Workspace-first architecture with cross-file SQL queries
**Next:** CRDT sync between devices
**Future:** TUI interface, collaborative editing


## License

MIT
