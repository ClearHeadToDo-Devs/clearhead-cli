# Contributors Guide

Welcome! This document contains technical details for developers working on `clearhead_cli`.

## Table of Contents

- [Architecture Overview](#architecture-overview)
- [Code Organization](#code-organization)
- [Data Model](#data-model)
- [Config System](#config-system)
- [Testing Strategy](#testing-strategy)
- [Adding New Features](#adding-new-features)
- [Philosophy & Values](#philosophy--values)

## Architecture Overview

### Library vs CLI Boundary

**Critical principle**: The library (`lib.rs`, `entities.rs`, `format.rs`) is completely separate from CLI concerns.

**Library** (`src/lib.rs`, `src/entities.rs`, `src/format.rs`, `src/treesitter.rs`):
- Takes simple types: `&str`, `&ActionList`, `OutputFormat`
- No dependencies on `clap`, `config`, or CLI-specific crates
- Pure functions that parse and format data
- Usable from any context: Rust, FFI, web services, Lua plugins

**CLI** (`src/main.rs`, `src/argparser.rs`, `src/environment_reader.rs`):
- Handles user interaction, file I/O, config loading
- Uses typed structs (not JSON Maps) for type safety
- Translates CLI types → library types
- Example: `argparser::Format` converts to `clearhead_cli::OutputFormat`

### Hub-and-Spoke Format System

ActionList is the hub, formatters are spokes:

```rust
ActionList (hub)
    ├─> Actions format  (round-trip safe .actions files)
    ├─> JSON format     (via serde)
    ├─> XML format      (via serde + quick-xml)
    └─> Table format    (via comfy-table)
```

Adding a new format is easy:
1. Add variant to `OutputFormat` enum
2. Implement `format_as_*` function
3. Add to dispatcher in `format()`

## Code Organization

```
src/
├── lib.rs              # Public API, parsing functions
├── entities.rs         # Action/ActionList data structures
├── format.rs           # Output formatters (Actions/JSON/XML/Table)
├── treesitter.rs       # Tree-sitter wrappers
├── main.rs             # CLI entry point
├── argparser.rs        # CLI argument parsing (clap)
└── environment_reader.rs  # Config loading (XDG + precedence)

tests/
├── lib.rs              # Integration tests using grammar test data
└── integration.rs      # E2E tests with isolated environments

examples/
└── format_demo.rs      # Demo showing all output formats
```

## Data Model

### Flat List Structure

Actions are stored in a **flat Vec**, not a tree. This is intentional for performance and flexibility:

```rust
pub struct Action {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,  // Links to parent
    pub state: ActionState,
    pub name: String,
    pub description: Option<String>,
    pub priority: Option<usize>,
    pub context_list: Option<Vec<String>>,
    pub do_date_time: Option<DateTime<Local>>,
    pub completed_date_time: Option<DateTime<Local>>,
    pub story: Option<String>,
}

pub type ActionList = Vec<Action>;
```

**Why flat instead of nested?**
- ✅ Simple: Just a Vec, easy to understand
- ✅ Fast queries: Filter/map directly on the Vec
- ✅ Easy mutations: Moving an action = changing parent_id
- ✅ Serde friendly: Serializes naturally to JSON/XML
- ✅ Grammar enforces depth: No need for compile-time nesting

**Depth calculation** is available via `action.depth(&action_list)` - it walks up the parent chain.

### Type Alias vs Newtype

`ActionList` is a **type alias** (`Vec<Action>`), not a newtype wrapper. This keeps it ergonomic:
- Direct Vec operations: `.len()`, `.iter()`, indexing
- No wrapping/unwrapping needed
- Works seamlessly with serde

Trade-off: Can't implement traits like `Display` on the alias. Instead, we use free functions like `format_action_list()`.

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
