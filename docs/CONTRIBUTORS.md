# Contributors Guide

Welcome! This document contains technical details for developers working on `clearhead_cli`.

## Conceptual Model

The **Rust struct (IR) is the canonical representation**. Everything else is a view or persistence mechanism. The IR aligns with the [Actions Ontology](../ontology/README.md), specifically the ActionPlan/ActionProcess distinction from BFO/CCO.

```
                         IR (Rust Structs)
                    (canonical in-memory type)
                    ActionPlan + ActionProcess
                              │
           ┌──────────────────┼──────────────────┐
           │                  │                  │
           ▼                  ▼                  ▼
         CRDT               DSL              Oxigraph
     (durable with      (text view        (query cache
      sync/merge)       for editors)       for SPARQL)
```

- **CRDT** is durable storage with merge semantics (via autosurgeon). Stores both ActionPlans and ActionProcesses.
- **DSL** is text serialization for editor workflows (plans: `*.actions`, processes: `*.log.actions`)
- **Oxigraph** is an ephemeral query cache materialized from the IR. Enables SPARQL queries and SHACL validation. 

The IR isn't an intermediary "hub" - it's the primary thing. CRDT ↔ IR is nearly zero-cost via autosurgeon's `Hydrate`/`Reconcile`. Oxigraph is rebuilt from IR as needed.

## Architecture Overview

### Library vs CLI Boundary

**Critical principle**: The library (`lib.rs`, `entities.rs`, `format.rs`) is completely separate from CLI concerns.

**Library** (`src/lib.rs`, `src/entities.rs`, `src/format.rs`, `src/treesitter.rs`, etc...):
- Takes simple types: `&str`, `&ActionList`, `OutputFormat`
- No dependencies on `clap`, `config`, or CLI-specific crates
- Pure functions that parse and format data
- Usable from any context: Rust, FFI, web services, Lua plugins

**CLI** (`src/main.rs`, `src/argparser.rs`, `src/environment_reader.rs`, `src/workspace.rs`):
- Handles user interaction, file I/O, config loading
- Uses typed structs (not JSON Maps) for type safety
- Translates CLI types → library types
- Example: `argparser::Format` converts to `clearhead_cli::OutputFormat`

### Hub-and-Spoke Format System

We use a hub-and-spoke architecture where the structs act as the strongly-typed glue that hold together several systems that are working together. They are mediated through this core structure:
- Action Domain Specific Language (DSL) - the text representation of actions as defined in [the file format](https://github.com/ClearHeadToDo-Devs/specifications/blob/master/action_file_format.md)
  - This is the primary interface for users with desktop editors so we pull in the [tree-sitter-actions](https://github.com/ClearHeadToDo-Devs/tree-sitter-actions/blob/master/README.md) grammar to parse and format this data
- Automerge (Conflict-free replicated data types) - the durable, syncable representation of actions
  - This keeps the door open for peer-to-peer syncing and offline-first editing while avoiding the sync issues we would deal with if we are lacking a true central server
  - our [sync specification](https://github.com/ClearHeadToDo-Devs/specifications/blob/master/sync_architecture.md) goes into more detail on this from a language-agnostic way
- Oxigraph (RDF triple store) - the semantic representation of actions that allows us to run SPARQL queries and SHACL validation on the data as 
  - This opens the door for advanced querying and integration with semantic web technologies
  - more of this is covered in [the ontology documentation](https://github.com/ClearHeadToDo-Devs/specifications/blob/master/ontology.md)

### Domain Model vs. Source Metadata

A core architectural principle is the separation of **Domain Data** from **Source Representation**.

1.  **The Domain Model (`Action`)**: Lives in `entities.rs`. It represents the abstract task (name, priority, state). It is kept "pure" and knows nothing about file line numbers or columns. This allows it to be easily used in databases, sync protocols, and serial formats without noise.
2.  **The Source Metadata (`SourceMetadata`)**: Lives in `entities.rs`. It tracks where an action (and its specific fields) are located in a concrete text file. 
  1. now, we USE that source data to inform the data model but keeping this separate allows us to avoid polluting the domain model with file-specific concerns.
3.  **The Parsed Document (`ParsedDocument`)**: A container returned by the parser that holds both the clean `ActionList` and a `HashMap<Uuid, SourceMetadata>`.

**Why this matters:**
- **Syncing**: When we sync actions between files, we only care about the `Action` data. The `SourceMetadata` is transient and file-specific.
- **LSP Ergonomics**: The LSP can iterate over the "pure" data to perform logic (e.g. "is this overdue?") and then use the sidecar map to find exactly where to place the visual hint in the editor.
- **Testing**: We can unit test business logic using just the `Action` struct without having to mock line numbers or file ranges.

Adding a new format is easy:
1. Add variant to `OutputFormat` enum
2. Implement `format_as_*` function
3. Add to dispatcher in `format()`

## Data Model

## Config System

### XDG Directories

```
~/.config/clearhead_cli/
  └── config.toml         # User preferences

~/.local/share/clearhead_cli/
  └── inbox.actions       # Default task list
```

### Precedence Chain

**CLI args > Environment variables > Config file > Defaults**

Implemented using the `config` crate, which handles file loading and env var parsing automatically:

```rust
ConfigBuilder::builder()
    .set_default("format", "actions")                    // 1. Defaults
    .add_source(File::from(config_path).required(false)) // 2. Config file
    .add_source(Environment::with_prefix("CLICHE"))      // 3. Env vars (CLICHE_FORMAT, etc.)
    .build()?
    .try_deserialize::<BaseConfig>()                     // Type-safe!
```

CLI arguments are applied in `main.rs` after loading the base config:

```rust
let output_format = cli_format           // CLI arg (highest priority)
    .or(env_format)                      // Env var
    .or(config_format)                   // Config file
    .unwrap_or(default_format);          // Default
```

### Why Typed Structs?

Previously, the config system serialized everything to `Map<String, Value>` (JSON). We refactored to use typed structs for:
- Compile-time safety
- IDE autocomplete
- Pattern matching
- Clear APIs

The library still uses simple types, only the CLI uses config structs.

## Testing Strategy

### Three Test Levels

1. **Unit tests** (`src/entities.rs`, `src/format.rs`):
   - Test individual functions in isolation
   - Fast, focused, no I/O

2. **Integration tests** (`tests/lib.rs`):
   - Use grammar's built-in test data (`get_test_data()`)
   - Test parsing → formatting round-trips
   - Verify all metadata types work

3. **E2E tests** (`tests/integration.rs`):
   - Test the full CLI pipeline in isolated environments
   - Use `tempfile` for temporary directories
   - Override XDG env vars to avoid pollution
   - Verify config precedence, all formats, error handling

### Running Tests

```bash
# All tests (unit + integration + E2E)
cargo test

# Just library tests
cargo test --lib

# Just integration tests
cargo test --test integration

# With output
cargo test -- --nocapture
```

### Testing the Argument Parser

We use `assert_cmd` for E2E CLI testing, but you can also test argument parsing directly using `clap`'s built-in test utilities:

```rust
#[test]
fn test_cli_parsing() {
    use clap::CommandFactory;
    let app = Cli::command();
    app.debug_assert();  // Validates the CLI definition
}
```

See the [clap testing tutorial](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html) for more approaches.

### Writing E2E Tests

Use the `TestEnv` helper to create isolated environments:

```rust
#[test]
fn test_my_feature() {
    let env = TestEnv::new();  // Creates temp XDG directories

    env.write_config("format = \"json\"");
    env.write_actions("test.actions", "[ ] My task");

    env.command()
        .arg("read")
        .assert()
        .success()
        .stdout(predicate::str::contains("My task"));
}
```

### Shared Test Helpers

We maintain a suite of shared test helpers in `tests/common/mod.rs` to keep tests functional and dry:

- **`ActionBuilder`**: A fluent interface for constructing `Action` structs in code. Use this instead of manual struct literals to keep tests resilient to changes in the data model.
- **`read_example` / `get_examples`**: Standardized functions for accessing the vendored specification examples.

### Snapshot Testing 
Another interesting approach we use is snapshot testing with `insta`. We do this by actually working through the individual examples provided by the `tree-sitter-actions` grammar tests, parsing them into our IR, and then generating snapshots of the resulting data structures.

You can actually see the snapshots themselves in the `snapshots` directory within the `tests` folder. These snapshots are stored in RON (Rusty Object Notation) format, which is a human-readable serialization format similar to JSON but more Rust-friendly.

With this, one can both see the structure and data of the parsed actions, and also verify that any changes to the parsing logic do not inadvertently alter the expected output.

While this does mean new examples added to the grammar tests will need corresponding snapshots, it provides a very robust way to ensure the integrity of the parsing logic over time. and to ensure that changes on the tree-sitter side are caught by the test suite if they arent caught by the other tests.

### Example Vendoring 
We vendor the examples hosted at the [specification repo](https://github.com/ClearHeadToDo-Devs/specifications.git) directly to reduce explicit coupling between depenencies. 

Instead, new tests and test modifications will be brought over as commits to ensure that the overview is done well while still having everything we need

## Adding New Features

### Adding a New Output Format

1. Add variant to `OutputFormat` enum in `src/format.rs`:
```rust
pub enum OutputFormat {
    Actions,
    Json,
    Xml,
    Table,
    Csv,  // New!
}
```

2. Implement formatter function:
```rust
fn format_as_csv(list: &ActionList) -> Result<String, String> {
    // Implementation
}
```

3. Add to dispatcher:
```rust
pub fn format(list: &ActionList, format: OutputFormat) -> Result<String, String> {
    match format {
        // ...
        OutputFormat::Csv => format_as_csv(list),
    }
}
```

4. Update CLI enum in `src/argparser.rs`:
```rust
pub enum Format {
    // ...
    Csv,
}

impl From<Format> for clearhead_cli::OutputFormat {
    fn from(f: Format) -> Self {
        match f {
            // ...
            Format::Csv => clearhead_cli::OutputFormat::Csv,
        }
    }
}
```

5. Write tests!

### Adding a New Command

1. Add command to `src/argparser.rs`:
```rust
pub enum Commands {
    Read { ... },
    Create {  // New!
        name: String,
        #[arg(short, long)]
        priority: Option<usize>,
    },
}
```

2. Handle in `src/main.rs`:
```rust
match &cli.command {
    Commands::Read { ... } => { ... }
    Commands::Create { name, priority } => {
        // Implementation
    }
}
```

3. Write E2E tests in `tests/integration.rs`

## Philosophy & Values

### Data-Centric

We prefer plain data structures over complex types:
- Public API uses simple types: `&str`, `Vec<Action>`, `OutputFormat`
- Enables FFI bindings for other languages
- Makes serialization trivial
- Values are immutable where possible

### Functional Programming

Pure functions that take immutable data and return new data:
```rust
// Good: pure function
pub fn format(list: &ActionList, format: OutputFormat) -> Result<String, String>

// Avoid: mutation
pub fn format_mut(list: &mut ActionList, format: OutputFormat)
```

Where side effects are necessary (file I/O, config loading), aggregate them in one place (usually `main.rs`).

### Minimal & Composable

- Don't reinvent wheels: Use `config` crate, `comfy-table`, `serde`
- Plain text files: Users can edit with any tool
- Unix philosophy: Do one thing well, compose with other tools
- Standards-based: XDG directories, tree-sitter grammar

### Pragmatism Over Purity

We're pragmatic Rustaceans, not zealots:
- Use type aliases when newtypes add friction
- Use `clap::ValueEnum` in CLI (but not in library)
- Side effects are OK when necessary, just isolate them
- "Perfect is the enemy of good"

## Future Directions

### Near-term
- **Create command**: Add new actions to files
- **Update command**: Modify existing actions
- **Delete command**: Remove actions

### Medium-term
- **TUI interface**: Interactive UI (ratatui)
- **Query language**: Filter/search actions
- **Templates**: Quick action creation

### Long-term
- **CRDTs**: Collaborative editing
- **RDF export**: Semantic web integration
- **Persistent data structures**: Efficient immutability

## Getting Help

- Open an issue on GitHub
- Read the code! It's documented with comments
- Run tests to see examples: `cargo test -- --nocapture`

## Code Style

- Use `rustfmt` for formatting
- Run `clippy` before committing: `cargo clippy`
- Write doc comments for public APIs
- Test coverage for new features
- Keep library pure, CLI can be impure

Happy hacking! 
