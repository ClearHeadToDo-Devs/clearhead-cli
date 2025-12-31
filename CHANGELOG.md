# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),

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
