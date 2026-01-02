# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),

## 2026-01-01

### Added
- **Topiary Formatting Integration:** Integrated Topiary as the official formatter for `.actions` files, implementing a "cargo fmt" equivalent.
  - Added `topiary-core` and `topiary-tree-sitter-facade` dependencies.
  - Created `queries/actions/topiary.scm` query file defining formatting rules (currently implements compact mode).
  - Added `FormatConfig` with `style` (Compact/List), `indent_width`, and `include_id` options.
  - **New `format` command:** Pure formatting without UUID modification. Preserves existing UUIDs, applies spacing rules.
  - Created comprehensive snapshot tests in `tests/formatting.rs` for both formatting modes.
  - Note: List mode infrastructure exists but currently produces same output as compact. Indentation scoping in Topiary query needed for full list mode support.

### Changed
- **Separated `format` and `normalize` commands:** Clarified conceptual distinction between formatting (cosmetic) and normalization (data integrity).
  - `format`: Formats `.actions` files with proper spacing, preserves existing UUIDs (doesn't add new ones).
  - `normalize`: Ensures all actions have UUIDs, formats by default (use `--no-format` to skip formatting).
- **Grammar Improvements:** Refactored `tree-sitter-actions` grammar to improve formatter compatibility.
  - Made `priority_level` and `story_name` into named node types (previously anonymous regex nodes).
  - This enables Topiary to preserve priority numbers and story names during formatting.

### Fixed
- **UUID Side-Effect Control:** Added `include_id: bool` field to `FormatConfig` to control UUID output.
  - Parser auto-generates UUIDs for all actions (useful for normal pipeline).
  - Formatter can now omit UUIDs via `include_id: false` (useful for testing stable output).
  - This eliminates the need for UUID redaction in snapshot tests.

## 2025-12-30

### Added
- **Automerge Integration:** Added `automerge` and `autosurgeon` dependencies to enable CRDT-based state synchronization.
- **Sync Utilities:** Created `src/sync_utils.rs` with helpers (`reconcile_date`, `hydrate_date`) to handle `chrono::DateTime` conversion for Automerge.
- **Snapshot Testing:** Implemented a rigorous test harness using `insta` and RON (Rusty Object Notation).
  - Tests now automatically generate/verify snapshots for all examples provided by `tree-sitter-actions`.
  - Added "Golden Data" files in `tests/snapshots/` to serve as the source of truth for the IR.
- **LSP Scaffolding:** Added `tower-lsp` and `tokio` dependencies.
  - Implemented `src/lsp.rs` with a basic Language Server loop.
  - Added `Lsp` command to the CLI, running on an isolated local Tokio runtime to preserve the synchronous nature of other CLI commands.
  - **Surgical Hydration:** Implemented the first LSP feature: a `Code Action` that detects actions missing an ID and surgically injects a `#uuid` (UUIDv7) without rewriting the rest of the file.

### Changed
- **Architecture:** Formalized the "Action-centric" Hub-and-Spoke model. The `Action` struct is now the primary Intermediate Representation (IR) for all sync and query operations.
- **Entities:**
  - `Action` struct now derives `Reconcile` and `Hydrate` for seamless Automerge sync.
  - Changed `priority` field from `usize` to `u32` for better compatibility with Automerge types.
  - Flattened `do_date_time`, `do_duration`, and `recurrence` fields in the `Action` struct (and Schema) to better match the flat-file format and simplfy sync logic.
- **Schema:** Updated `tree-sitter-actions/schema/actions.schema.json`:
  - Added `parent_id` to the allowed properties (supporting the Adjacency List model).
  - Flattened the `doDate` object properties into top-level fields (`doDateTime`, `doDuration`, `recurrence`).
- **SQL:** Updated `src/sql.rs` to handle `priority` as `u32`.

### Fixed
- **JSON Validation:** Fixed regression where the CLI's JSON output (flat list) conflicted with the previous Schema expectation (nested tree). The Schema was updated to match the implementation.
