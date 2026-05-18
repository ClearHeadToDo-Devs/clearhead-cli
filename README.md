# clearhead

**CLI and LSP server for the ClearHead action management framework.**

Work items live in plain-text `.actions` files that any editor can read and
write. Recurring schedules live in `.ics` (vdir) files. Completed history
accumulates in a single `archive.ttl` file. `clearhead` provides the
command-line interface, a built-in LSP server, and the Rust core library that
backs both.

## Installation

```bash
cargo install clearhead
```

Or build from source:

```bash
git clone https://github.com/ClearHeadToDo-Devs/clearhead-cli
cd clearhead-cli
cargo build --release
```

## Quick start

```bash
# Add an action to your inbox
clearhead add action "Buy oat milk" --charter inbox

# List open actions
clearhead read actions --open-only

# Complete it
clearhead complete action "Buy oat milk"

# Archive completed actions out of active files
clearhead archive actions

# Show resolved config and workspace layout
clearhead debug
```

## Documentation

Full reference documentation is in the man page:

```bash
man clearhead
```

Every subcommand also has inline help:

```bash
clearhead --help
clearhead read --help
clearhead archive charter --help
```

## Editor integration

The official Neovim plugin provides LSP setup, syntax highlighting, state
cycling, depth hotkeys, workspace pickers, and archiving commands:

- **[clearhead.nvim](https://github.com/ClearHeadToDo-Devs/clearhead.nvim)**

For other LSP-compatible editors, start the server directly:

```bash
clearhead start lsp
```

## Specifications

The file format, workspace layout, and process model are defined in the
[ClearHead specifications](https://github.com/ClearHeadToDo-Devs/specifications):

- [Action file format](https://github.com/ClearHeadToDo-Devs/specifications/blob/master/action_file_format.md)
- [Naming conventions and workspace layout](https://github.com/ClearHeadToDo-Devs/specifications/blob/master/naming_conventions.md)
- [Process](https://github.com/ClearHeadToDo-Devs/specifications/blob/master/process.md)

## License

MIT
