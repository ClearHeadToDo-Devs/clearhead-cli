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
# Read entire workspace (all .actions files)
clearhead_cli read

# Table view (great for browsing)
clearhead_cli read --format table

# JSON (great for scripting)
clearhead_cli read --format json

# Read a specific file
clearhead_cli read file inbox.actions
```

## Features

- **Event logging**: Persistent append-only history of all action changes (completions, additions, deletions) for analytics.
- **Calendar export**: Export actions with due dates to iCalendar (`.ics`) format with full recurrence support
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
- `$description` - Task description
- `!N` - Priority (1-4, where 1 is highest)
- `+tag,tag` - Context tags
- `@YYYY-MM-DDTHH:MM` - Do date/time (when task should be done)
- `DN` - Duration in minutes (e.g., `D30` for 30 minutes)
- `R:FREQ=...` - Recurrence rule (RRULE format)
- `%YYYY-MM-DDTHH:MM` - Completed date/time
- `^YYYY-MM-DDTHH:MM` - Created date/time
- `#uuid` - Unique ID (auto-generated if omitted)
- `*story` - Story/project (root actions only)

## Commands

### Read

The `read` command operates in three modes:

```bash
# Workspace-wide read (default) - reads ALL .actions files from data directory
clearhead_cli read

# Read specific file
clearhead_cli read file ~/work.actions

# Read from stdin
cat tasks.actions | clearhead_cli read stdio
```

**Output formats:**

```bash
clearhead_cli read --format json    # JSON (great for scripting)
clearhead_cli read --format table   # Table view (great for browsing)
clearhead_cli read --format xml     # XML format
clearhead_cli read --format actions # Original format (default)
```

**Filtering with SQL queries (workspace mode only):**

SQL filtering is available for workspace-wide reads, enabling cross-file queries:

```bash
# Simple WHERE clause
clearhead_cli read --where "priority = 1"
clearhead_cli read --where "state = 'completed'"

# Filter by project (inferred from directory structure)
clearhead_cli read --where "project = 'work'"

# Filter by source file
clearhead_cli read --where "file_path LIKE '%inbox%'"

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

The SQL schema includes `file_path` and `project` columns for cross-file filtering. Project names are inferred from directory structure (e.g., `work/next.actions` → project "work").

See [docs/SQL_QUERIES.md](docs/SQL_QUERIES.md) for the complete guide to SQL queries.

### Add

Add a new action to a file. If the file doesn't exist, it will be created.

```bash
# Add a simple task
clearhead_cli add "Buy groceries" --write

# Add with metadata
clearhead_cli add "Fix critical bug" --priority 1 --context work --description "Check logs" --write
```

### Complete

Mark an action as completed. Can match by name or UUID prefix.

```bash
# Complete by name
clearhead_cli complete "Buy groceries" --write

# Complete by UUID prefix
clearhead_cli complete 019baae --write
```

**Recurring Actions:**
When you complete a recurring action (e.g., "Laundry R:FREQ=WEEKLY"), ClearHead will:
1. Log a completed instance for today.
2. Advance the template's due date to the next occurrence.
3. Keep the template open for the future.

### Agenda

View your upcoming schedule, including projected recurring events.

```bash
# Show agenda for the next 7 days (default)
clearhead_cli agenda

# Show agenda for the next 30 days
clearhead_cli agenda --days 30
```

The agenda view projects future occurrences of recurring tasks without creating clutter in your file, giving you a clear view of your upcoming workload.

### Sync Events

Synchronize existing actions with the events database. This is useful for backfilling history from files created before event logging was enabled. It automatically skips actions that already have events logged.

```bash
# Backfill events for default file
clearhead_cli sync-events

# Preview what would be backfilled
clearhead_cli sync-events --dry-run
```

### Export to Calendar

Export actions with due dates to iCalendar (`.ics`) format for import into Google Calendar, Apple Calendar, Outlook, or any calendar app.

```bash
# Export to stdout
clearhead_cli export inbox.actions

# Export to a file
clearhead_cli export inbox.actions -o calendar.ics

# Export only open actions (skip completed/cancelled)
clearhead_cli export inbox.actions --open-only -o calendar.ics

# Export from stdin
cat work.actions | clearhead_cli export > work.ics
```

**Supported features:**
- **Recurring events** - RRULE patterns (daily, weekly, monthly, yearly) are preserved
- **Event duration** - Uses `do_duration` or defaults to 15 minutes
- **Descriptions** - Action descriptions become event descriptions
- **Priority** - Mapped from ClearHead priority (1-4) to iCalendar (1-9)
- **Categories** - Context tags become event categories
- **Status** - Action states map to event statuses (tentative, confirmed, cancelled)

**Example:**
```actions
[ ] Daily standup @2026-01-20T09:00 D15 R:FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR
    $ Check in with team
    !2
    +Work,Meeting
```

Exports to:
```ical
BEGIN:VEVENT
SUMMARY:Daily standup
DESCRIPTION:Check in with team
DTSTART:20260120T170000Z
DTEND:20260120T171500Z
RRULE:FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR
PRIORITY:3
CATEGORIES:Work,Meeting
STATUS:TENTATIVE
END:VEVENT
```

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

### Event Logging (Analytics)

ClearHead maintains a persistent history of all changes in a SQLite database. This enables time-series analytics like completion rates, streaks, and audit trails.

Events are emitted automatically by:
- The `add` and `complete` CLI commands.
- The LSP server whenever a `.actions` file is saved (detects hand-edits via structural diff).

The database is stored at `~/.local/state/clearhead/events.db` by default.

### System Logging (Operational)

ClearHead uses tiered logging for full transparency:
1. **Data Level**: `events.db` stores semantic user actions (Audit trail).
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
