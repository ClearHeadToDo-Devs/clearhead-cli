# cliche

**Command Line Interface for the ClearHead framework**

A fast, flexible task manager that uses plain text `.actions` files - edit them with any text editor, manage them with `cliche`.

## Quick Start

### Installation

```bash
cargo install cliche
```

### Your First Task List

Create a file called `inbox.actions`:

```
[ ] Welcome to cliche $Your personal task manager
> [ ] Try different output formats $Run: cliche read --format table
> [x] Get started $You're already using it!
```

View it in different formats:

```bash
# Table view (great for browsing)
cliche read inbox.actions --format table

# JSON (great for scripting)
cliche read inbox.actions --format json

# Original format (round-trip safe)
cliche read inbox.actions
```

## Features

- **Multiple output formats**: actions, json, xml, table
- **XDG compliant**: Follows standard directory conventions
- **Config precedence**: CLI args → env vars → config file → defaults
- **Zero lock-in**: Plain text files, use any editor
- **Type-safe**: Rust library + CLI tool

## Configuration

Optionally create `~/.config/cliche/config.toml`:

```toml
format = "table"           # Default output format
file = "inbox.actions"     # Default file in ~/.local/share/cliche/
```

Override with environment variables:

```bash
CLICHE_FORMAT=json cliche read
```

Or command-line arguments (highest priority):

```bash
cliche read --format table
```

## Task Syntax

```
[x] Completed task $with description !1 +context,tags
> [ ] Child task
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
# Read default file (~/.local/share/cliche/inbox.actions)
cliche read

# Read specific file
cliche read ~/work.actions

# Output as JSON
cliche read --format json

# Output as table
cliche read --format table
```

### Advanced Workflows

**Normalization (Hydration)**
To enable advanced features like sorting and patching, your file needs stable UUIDs.
```bash
# Add UUIDs to all actions in the file
cliche normalize ~/work.actions --write
```

**Patching (Smart Sync)**
Update a Primary file based on a modified Secondary view (even if lines were reordered).
```bash
# Apply changes from a temp file back to the source of truth
cliche patch --primary ~/work.actions --secondary ~/tmp/filtered_view.actions --write
```
This is the engine that powers editor plugins, allowing you to filter/sort a view, edit it, and save the changes back to the original file safely.

## Development

See [CONTRIBUTORS.md](CONTRIBUTORS.md) for:
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
