# ClearHead LSP

The ClearHead CLI includes a built-in Language Server Protocol (LSP) server to provide a rich editing experience for `.actions` files.

## Architecture

The LSP is built in Rust using the `tower-lsp-server` library (the community-maintained fork of `tower-lsp`). It leverages the project's existing `tree-sitter-actions` grammar and `clearhead_cli` library for shared logic.

### Core Components (`src/lsp.rs`)

1.  **Backend Struct**: Implements the `LanguageServer` trait. It holds the state of open documents.
2.  **DocumentState**: Stores the full text and the parsed `tree-sitter` Tree for each open document.
3.  **Pure Logic Functions**: The core intelligence is extracted into pure functions to facilitate unit testing:
    *   `compute_diagnostics`: Scans the tree for errors (e.g., missing UUIDs).
    *   `compute_code_actions`: Generates quick fixes (e.g., "Hydrate Action").
    *   `compute_semantic_tokens`: Maps `tree-sitter` nodes to LSP token types (semantic highlighting).
    *   `compute_inlay_hints`: Computes relative dates (e.g., "due in 3d") and other contextual info.

## Features

| Feature | Description | Trigger |
| :--- | :--- | :--- |
| **Diagnostics** | Warns if an action is missing a UUID (`#...`). | Open/Change file |
| **Code Actions** | "Hydrate Action": Automatically generates and inserts a UUIDv7. | Ctrl+. on warning |
| **Semantic Highlighting** | Colors keywords, dates, contexts, and IDs distinctively. | Open/Change file |
| **Inlay Hints** | Shows relative time for `do_date` ("due in 5d") and `completed_date`. | Open/Change file |
| **Go to Definition** | Jumps to the *first* occurrence of a Story (`*`) or Context (`+`) tag in the file. | F12 / Cmd+Click |
| **Find References** | Lists *all* occurrences of a Story (`*`) or Context (`+`) tag in the file. | Shift+F12 |
| **Formatting** | Formats the document using the canonical `topiary` rules (indentation, spacing). | Shift+Alt+F / On Save |

## Running the LSP

To start the LSP server (usually done by your editor):

```bash
clearhead-cli lsp
```

## Testing

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
