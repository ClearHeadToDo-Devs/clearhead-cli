# ClearHead LSP

The ClearHead CLI includes a built-in Language Server Protocol (LSP) server to provide a rich editing experience for `.actions` files.

## Architecture

The LSP follows a **Sidecar Metadata** pattern to keep high-level logic pure and decoupled from file-specific location tracking.

### The Two-Pronged Model

When a document is opened or changed, the LSP parses it into a `ParsedDocument` which contains two distinct structures:

1.  **The Domain Model (`ActionList`)**: A flat list of `Action` structs. This is the "What" — it contains task names, states, due dates, etc. It is identical to the data used by the CLI and DB layers.
2.  **The Source Map (`HashMap<Uuid, SourceMetadata>`)**: A mapping from an action's unique ID to its concrete location in the source file. This is the "Where" — it contains line and column ranges for the action root, due dates, and other metadata.

### Why this split?
- **Zero Pollution**: The `Action` struct remains a pure domain entity. It doesn't need to know about line numbers, making it easier to sync, serialize, and store.
- **Ergonomic Logic**: LSP features like "Inlay Hints" or "Diagnostics" simply iterate over the `ActionList` (business logic) and then look up the location in the `SourceMap` (presentation logic).
- **Performance**: High-level features avoid expensive `O(N^2)` tree traversals by using direct ID lookups.

## Features

| Feature | Description | Implementation |
| :--- | :--- | :--- |
| **Diagnostics** | Warns if an action is missing a UUID (`#...`). | Checks `SourceMetadata::is_id_generated` for each action. |
| **Code Actions** | "Hydrate Action": Automatically generates and inserts a UUIDv7. | Intersects cursor range with `SourceMetadata::root`. |
| **Inlay Hints** | Shows relative time for `do_date` ("due in 5d") and `completed_date`. | Places hints at `SourceMetadata::do_date` / `completed_date` ranges. |
| **Semantic Highlighting** | Colors keywords, dates, contexts, and IDs distinctively. | Uses raw `tree-sitter` traversal for fine-grained token coloring. |
| **Go to Definition** | Jumps to Story (`*`) or Context (`+`) tags. | Uses raw `tree-sitter` to find references by text. |
| **Formatting** | Formats the document using canonical rules. | Re-serializes the `ActionList` using the library's `format` module. |

## Client Setup

The LSP server is invoked via the `lsp` subcommand: `clearhead_cli lsp`.

### Neovim (Manual Setup)

If you are not using a plugin, you can start the LSP manually in your `init.lua`:

```lua
vim.api.nvim_create_autocmd("FileType", {
  pattern = "actions",
  callback = function()
    vim.lsp.start({
      name = "clearhead-lsp",
      cmd = { "clearhead_cli", "lsp" },
      root_dir = vim.fs.dirname(vim.fs.find({ ".git", "inbox.actions" }, { upward = true })[1]),
    })
  end,
})
```

### Neovim (via clearhead.nvim)

The [clearhead.nvim](https://github.com/ClearHeadToDo-Devs/clearhead.nvim) plugin is the recommended way to use the LSP. It handles binary detection and provides additional buffer-local keybindings for features like "Hydrate Action".

## Technical Specifications
- **Transport**: Stdio
- **Initialization**: Standard LSP `initialize` request.
- **Capabilities**: Full document sync, diagnostics, code actions, semantic tokens, inlay hints, definition, references, and formatting.

### Unit Tests
The logic functions are unit-tested in `src/lsp.rs`. Run them with:

```bash
cargo test lsp
```

### Manual Testing (VSCode)
To test with VSCode during development:
1.  Use a generic LSP client extension (like "Run on Save" or a dedicated LSP client).
2.  Configure it to run the compiled binary: `target/debug/clearhead_cli lsp`.

## Future Work
- **Go to Definition**: Jump between Story links and their definitions.
- **Completion**: Autocomplete Contexts (`+tag`) based on other tags in the file.
- **Rename**: Rename a tag and update all occurrences.
