use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// CLI argument parser - returns typed structs for ergonomic access
pub fn parse_cli() -> Cli {
    Cli::parse()
}

/// Valid column names for table output
pub const VALID_COLUMNS: &[&str] = &[
    "state",
    "name",
    "charter",
    "priority",
    "due",
    "dur",
    "recurrence",
    "context",
    "description",
    "id",
    "story", // backward-compat alias for charter
];

/// Validate column names and return error with valid options
pub fn validate_column_names(names: &[String]) -> Result<(), String> {
    let invalid: Vec<String> = names
        .iter()
        .filter(|name| !VALID_COLUMNS.contains(&name.to_lowercase().as_str()))
        .map(|s| s.clone())
        .collect();

    if !invalid.is_empty() {
        Err(format!(
            "Unknown column(s): {}\nValid columns are: {}",
            invalid.join(", "),
            VALID_COLUMNS.join(", ")
        ))
    } else {
        Ok(())
    }
}

/// CLI-specific table column filtering options (for use with clap arg parsing)
#[derive(Args, Clone, Default, Debug, PartialEq)]
pub struct CliTableOptions {
    /// Only show these columns (comma-separated: name,state,priority)
    #[arg(long, value_delimiter = ',')]
    pub columns: Option<Vec<String>>,

    /// Hide these columns (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub hide_columns: Option<Vec<String>>,

    /// List all available column names and descriptions
    #[arg(long)]
    pub list_columns: bool,
}

impl CliTableOptions {
    /// Convert CLI options to library's TableFormatOptions
    pub fn to_lib_opts(&self) -> clearhead_cli::format::TableFormatOptions {
        clearhead_cli::format::TableFormatOptions {
            columns: self.columns.clone(),
            hide_columns: self.hide_columns.clone(),
            list_columns: self.list_columns,
        }
    }
}

/// Shared action field options used by Add and Update commands
#[derive(Args, Clone, Default, Debug)]
pub struct ActionFields {
    /// Priority of the action (1-9)
    #[arg(short, long)]
    pub priority: Option<u32>,

    /// Contexts for the action (can be specified multiple times)
    #[arg(short, long)]
    pub context: Vec<String>,

    /// Description of the action
    #[arg(short, long)]
    pub description: Option<String>,

    /// Alias for referencing this action
    #[arg(short, long)]
    pub alias: Option<String>,

    /// State of the action
    #[arg(short, long, value_enum)]
    pub state: Option<ActionStateArg>,
}

/// Convert CLI ActionFields to library ActionUpdate
impl From<ActionFields> for clearhead_cli::ActionUpdate {
    fn from(f: ActionFields) -> Self {
        clearhead_cli::ActionUpdate {
            name: None, // name is handled separately
            priority: f.priority,
            description: f.description,
            context: if f.context.is_empty() {
                None
            } else {
                Some(f.context)
            },
            alias: f.alias,
            state: f.state.map(|s| s.into()),
        }
    }
}

/// Action state values for CLI
#[derive(Clone, Copy, ValueEnum, Debug)]
pub enum ActionStateArg {
    /// Not started (default)
    NotStarted,
    /// In progress
    InProgress,
    /// Blocked/waiting
    Blocked,
    /// Completed
    Completed,
    /// Cancelled
    Cancelled,
}

impl From<ActionStateArg> for clearhead_cli::ActionState {
    fn from(s: ActionStateArg) -> Self {
        match s {
            ActionStateArg::NotStarted => clearhead_cli::ActionState::NotStarted,
            ActionStateArg::InProgress => clearhead_cli::ActionState::InProgress,
            ActionStateArg::Blocked => clearhead_cli::ActionState::BlockedorAwaiting,
            ActionStateArg::Completed => clearhead_cli::ActionState::Completed,
            ActionStateArg::Cancelled => clearhead_cli::ActionState::Cancelled,
        }
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub debug: u8,

    #[command(subcommand)]
    pub command: Verb,
}

#[derive(Subcommand)]
pub enum Verb {
    /// Read and list items (plans, charters, agenda)
    Read {
        #[command(subcommand)]
        target: ReadTarget,
    },

    /// Show details of a single item
    Show {
        #[command(subcommand)]
        target: ShowTarget,
    },

    /// Add a new item
    Add {
        #[command(subcommand)]
        target: AddTarget,
    },

    /// Update an existing item
    Update {
        #[command(subcommand)]
        target: UpdateTarget,
    },

    /// Mark an item as completed
    Complete {
        #[command(subcommand)]
        target: CompleteTarget,
    },

    /// Delete an item
    Delete {
        #[command(subcommand)]
        target: DeleteTarget,
    },

    /// Format a file
    Format {
        #[command(subcommand)]
        target: FormatTarget,
    },

    /// Lint a file for errors and warnings
    Lint {
        #[command(subcommand)]
        target: LintTarget,
    },

    /// Normalize a file (ensure UUIDs, format)
    Normalize {
        #[command(subcommand)]
        target: NormalizeTarget,
    },

    /// Apply changes from a patch file
    Patch {
        #[command(subcommand)]
        target: PatchTarget,
    },

    /// Archive completed items
    Archive {
        #[command(subcommand)]
        target: ArchiveTarget,
    },

    /// Export items to external formats
    Export {
        #[command(subcommand)]
        target: ExportTarget,
    },

    /// Expand recurring plans into PlannedAct instances
    Expand {
        #[command(subcommand)]
        target: ExpandTarget,
    },

    /// Cancel an item (sets phase to Cancelled without deleting)
    Cancel {
        #[command(subcommand)]
        target: CancelTarget,
    },

    /// Start a service
    Start {
        #[command(subcommand)]
        target: StartTarget,
    },

    /// Synchronize data with external systems
    Sync {
        #[command(subcommand)]
        target: SyncTarget,
    },

    /// Execute SPARQL queries against the workspace RDF graph
    Query {
        #[command(subcommand)]
        target: QueryTarget,
    },
}

// =============================================================================
// Query targets
// =============================================================================

#[derive(Subcommand)]
pub enum QueryTarget {
    /// Run a raw SPARQL query
    Run {
        /// Full SPARQL SELECT query
        #[arg(conflicts_with = "where_clause")]
        sparql: Option<String>,

        /// SPARQL WHERE clause (auto-injects prefixes, selects all variables)
        #[arg(short = 'w', long = "where", conflicts_with = "sparql")]
        where_clause: Option<String>,

        /// Output format
        #[arg(short, long, value_enum)]
        format: Option<QueryFormat>,
    },

    /// Run a named query stored in ~/.clearhead/queries/ or <workspace>/.clearhead/queries/
    #[command(name = "named")]
    NamedRun {
        /// Name of the query (stem of the .sparql file)
        name: String,

        /// Output format
        #[arg(short, long, value_enum)]
        format: Option<QueryFormat>,
    },

    /// List available named queries
    List,
}

// =============================================================================
// Read targets
// =============================================================================

#[derive(Subcommand)]
pub enum ReadTarget {
    /// Read and display plans (workspace-wide by default)
    Plans {
        /// Output format
        #[arg(short, long, value_enum)]
        format: Option<Format>,

        /// Filter plans by charter name, alias, or UUID
        #[arg(long, conflicts_with_all = ["file", "stdio"])]
        charter: Option<String>,

        /// Include plans from sub-charters recursively (requires --charter)
        #[arg(long, requires = "charter")]
        recursive: bool,

        /// Read from a specific file instead of workspace
        #[arg(long, conflicts_with_all = ["charter", "stdio"])]
        file: Option<PathBuf>,

        /// Read from standard input instead of workspace
        #[arg(long, conflicts_with_all = ["file", "charter"])]
        stdio: bool,

        /// Table column filtering options
        #[command(flatten)]
        table_options: CliTableOptions,
    },

    /// List all discovered charters
    Charters {
        /// Output format
        #[arg(short, long, value_enum)]
        format: Option<Format>,

        /// Show only explicit (file-backed) charters
        #[arg(long)]
        explicit_only: bool,
    },

    /// Show an agenda of upcoming actions
    Agenda {
        /// File to read (.actions format). If not provided, uses the default file
        file: Option<PathBuf>,

        /// Number of days to project forward
        #[arg(short, long, default_value = "7")]
        days: u32,
    },

    /// Read and display planned acts
    Acts {
        /// Output format (table or json)
        #[arg(short, long, value_enum)]
        format: Option<ActFormat>,

        /// Filter acts by plan UUID, short UUID, alias, or name
        #[arg(long)]
        plan: Option<String>,

        /// Only show open acts (excludes Completed and Cancelled)
        #[arg(long)]
        open_only: bool,

        /// Read from a specific .actions file (also loads sibling .acts.jsonld)
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

// =============================================================================
// Show targets
// =============================================================================

#[derive(Subcommand)]
pub enum ShowTarget {
    /// Show details of a specific plan
    Plan {
        /// UUID, short UUID, alias, or name of the plan
        query: String,

        /// File containing the plan. If not provided, uses the default file
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Output format
        #[arg(short = 'F', long, value_enum)]
        format: Option<Format>,

        /// Table column filtering options
        #[command(flatten)]
        table_options: CliTableOptions,
    },

    /// Show details of a specific charter
    Charter {
        /// UUID, alias, or name of the charter
        query: String,
    },
}

// =============================================================================
// Add targets
// =============================================================================

#[derive(Subcommand)]
pub enum AddTarget {
    /// Add a new plan
    Plan {
        /// Name of the plan
        name: String,

        /// File to add to. If not provided, uses the default file
        #[arg(short, long, conflicts_with = "charter")]
        file: Option<PathBuf>,

        /// Charter to add the plan to (name, alias, or UUID). Routes to the charter's primary file.
        #[arg(long, conflicts_with = "file")]
        charter: Option<String>,

        /// Parent plan reference: alias/name (same-file) or charter/plan (cross-charter)
        #[arg(long)]
        parent: Option<String>,

        /// Action fields (priority, context, description, alias, state)
        #[command(flatten)]
        fields: ActionFields,

        /// Preview what would be added without writing
        #[arg(long)]
        dry_run: bool,
    },

    /// Create a new charter
    Charter {
        /// Title of the charter
        title: String,

        /// Short alias for the charter
        #[arg(short, long)]
        alias: Option<String>,

        /// Parent charter reference
        #[arg(short, long)]
        parent: Option<String>,

        /// Preview what would be created without writing
        #[arg(long)]
        dry_run: bool,
    },
}

// =============================================================================
// Update / Complete / Delete targets
// =============================================================================

#[derive(Subcommand)]
pub enum UpdateTarget {
    /// Update an existing plan
    Plan {
        /// UUID, short UUID, alias, or name of the plan to update
        query: String,

        /// File containing the plan. If not provided, uses the default file
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// New name for the plan
        #[arg(short, long)]
        name: Option<String>,

        /// Action fields to update (priority, context, description, alias, state)
        #[command(flatten)]
        fields: ActionFields,

        /// Preview what would be updated without writing
        #[arg(long)]
        dry_run: bool,
    },

    /// Update a planned act's scheduled time or duration
    Act {
        /// UUID or 8-char prefix of the act
        query: String,

        /// New scheduled datetime (RFC 3339, e.g. "2026-04-01T09:00:00+00:00")
        #[arg(long)]
        scheduled_at: Option<String>,

        /// New duration in minutes
        #[arg(long)]
        duration: Option<u32>,

        /// File containing the .actions file (sidecar is derived). If not provided, workspace search.
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Preview what would be updated without writing
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum CompleteTarget {
    /// Mark a plan as completed
    Plan {
        /// UUID or name of the plan to complete
        query: String,

        /// File containing the plan. If not provided, uses the default file
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Preview what would be completed without writing
        #[arg(long)]
        dry_run: bool,
    },

    /// Mark a planned act as completed
    Act {
        /// UUID or 8-char prefix of the act
        query: String,

        /// File containing the .actions file (sidecar is derived). If not provided, workspace search.
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Preview what would be completed without writing
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum DeleteTarget {
    /// Delete a plan
    Plan {
        /// UUID or name of the plan to delete
        query: String,

        /// File containing the plan. If not provided, uses the default file
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Preview what would be deleted without writing
        #[arg(long)]
        dry_run: bool,
    },
}

// =============================================================================
// File operation targets
// =============================================================================

#[derive(Subcommand)]
pub enum FormatTarget {
    /// Format an actions file with proper spacing and layout
    File {
        /// File to format. If not provided, reads from stdin
        path: Option<PathBuf>,

        /// Overwrite the input file with the formatted version
        #[arg(short, long)]
        write: bool,

        /// Formatting style (compact or list)
        #[arg(short, long, value_enum)]
        style: Option<Style>,

        /// Indentation style (spaces or tabs)
        #[arg(long, value_enum)]
        indent_style: Option<Indent>,

        /// Indentation width (for list style or child padding)
        #[arg(short, long)]
        indent_width: Option<usize>,
    },
}

#[derive(Subcommand)]
pub enum LintTarget {
    /// Lint an actions file for errors and warnings
    File {
        /// File to lint. If not provided, reads from stdin
        path: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum NormalizeTarget {
    /// Ensure all actions in a file have UUIDs (formats by default)
    File {
        /// File to normalize. If not provided, reads from stdin
        path: Option<PathBuf>,

        /// Overwrite the input file with the normalized version
        #[arg(short, long)]
        write: bool,

        /// Skip formatting after adding UUIDs
        #[arg(long)]
        no_format: bool,
    },
}

#[derive(Subcommand)]
pub enum PatchTarget {
    /// Apply changes from a secondary patch file to a primary source file
    File {
        /// The primary source file (source of truth)
        #[arg(short, long)]
        primary: PathBuf,

        /// The secondary file containing updates/patches
        #[arg(short, long)]
        secondary: PathBuf,

        /// Overwrite the primary file with the patched version
        #[arg(short, long)]
        write: bool,
    },
}

// =============================================================================
// Collection operation targets
// =============================================================================

#[derive(Subcommand)]
pub enum ArchiveTarget {
    /// Move completed plans to a <charter>.completed.actions file. Workspace-wide by default.
    Plans {
        /// Domain path to scope: "health", "health/exercise", etc. Workspace-wide if omitted.
        scope: Option<String>,

        /// Escape hatch: explicit file path for out-of-workspace use.
        #[arg(long, conflicts_with = "scope")]
        file: Option<PathBuf>,

        /// Dry run: show what would be archived without moving
        #[arg(long)]
        dry_run: bool,
    },

    /// Move completed/cancelled acts from open.ttl to closed.ttl. Workspace-wide by default.
    Acts {
        /// Domain path to scope: "health", "health/exercise", etc. Workspace-wide if omitted.
        scope: Option<String>,

        /// Escape hatch: explicit file path for out-of-workspace use.
        #[arg(long, conflicts_with = "scope")]
        file: Option<PathBuf>,

        /// Dry run: show counts without writing
        #[arg(long)]
        dry_run: bool,
    },
}

// =============================================================================
// Expand targets
// =============================================================================

#[derive(Subcommand)]
pub enum ExpandTarget {
    /// Expand recurring plans into sidecar PlannedAct instances
    Acts {
        /// File to expand (.actions format). If not provided, uses the default file.
        file: Option<PathBuf>,

        /// Number of days to project forward
        #[arg(long, default_value = "90")]
        days: u32,

        /// Preview what would be written without writing
        #[arg(long)]
        dry_run: bool,
    },
}

// =============================================================================
// Cancel targets
// =============================================================================

#[derive(Subcommand)]
pub enum CancelTarget {
    /// Cancel a planned act (sets phase to Cancelled)
    Act {
        /// UUID or 8-char prefix of the act
        query: String,

        /// File containing the .actions file (sidecar is derived). If not provided, workspace search.
        #[arg(short, long)]
        file: Option<PathBuf>,

        /// Preview what would be cancelled without writing
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum ExportTarget {
    /// Export plans to calendar format (iCalendar)
    #[command(alias = "calendar")]
    Plans {
        /// Reference to export (charter/plan/act alias or UUID, or .actions file)
        #[arg(value_name = "REFERENCE")]
        reference: Option<String>,

        /// Output file path. If not provided, writes to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Only export open plans (pending, in-progress, blocked)
        #[arg(long)]
        open_only: bool,

        /// Include sub-charters when exporting a charter reference
        #[arg(long, requires = "reference")]
        recursive: bool,
    },
}

// =============================================================================
// Service targets
// =============================================================================

#[derive(Subcommand)]
pub enum StartTarget {
    /// Start the Language Server Protocol (LSP) server
    Lsp,
}

#[derive(Subcommand)]
pub enum SyncTarget {
    /// Synchronize existing actions with the events database
    Events {
        /// File to sync. If not provided, uses the default file
        file: Option<PathBuf>,

        /// Dry run: show what would be synced without writing to DB
        #[arg(long)]
        dry_run: bool,
    },
}

// =============================================================================
// Shared value enums
// =============================================================================

/// Output format for act listing
#[derive(Clone, Copy, ValueEnum, Debug)]
pub enum ActFormat {
    /// Pretty-printed table (default)
    Table,
    /// JSON array
    Json,
}

/// Output format for raw SPARQL query results
#[derive(Clone, Copy, ValueEnum, Debug)]
pub enum QueryFormat {
    /// Pretty-printed table
    Table,
    /// JSON array of objects
    Json,
}

/// CLI-specific format enum that maps to library's OutputFormat
#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    /// .actions file format
    Actions,
    /// JSON format
    Json,
    /// XML format
    Xml,
    /// Table format
    Table,
}

impl From<Format> for clearhead_cli::OutputFormat {
    fn from(f: Format) -> Self {
        match f {
            Format::Actions => clearhead_cli::OutputFormat::Actions,
            Format::Json => clearhead_cli::OutputFormat::Json,
            Format::Xml => clearhead_cli::OutputFormat::Xml,
            Format::Table => clearhead_cli::OutputFormat::Table,
        }
    }
}

/// CLI-specific style enum that maps to library's FormatStyle
#[derive(Clone, Copy, ValueEnum)]
pub enum Style {
    /// Compact: metadata on same line
    Compact,
    /// List: metadata on separate indented lines
    List,
}

impl From<Style> for clearhead_cli::FormatStyle {
    fn from(s: Style) -> Self {
        match s {
            Style::Compact => clearhead_cli::FormatStyle::Compact,
            Style::List => clearhead_cli::FormatStyle::List,
        }
    }
}

/// CLI-specific indent enum that maps to library's IndentStyle
#[derive(Clone, Copy, ValueEnum)]
pub enum Indent {
    /// Use spaces for indentation
    Spaces,
    /// Use tabs for indentation
    Tabs,
}

impl From<Indent> for clearhead_cli::IndentStyle {
    fn from(i: Indent) -> Self {
        match i {
            Indent::Spaces => clearhead_cli::IndentStyle::Spaces,
            Indent::Tabs => clearhead_cli::IndentStyle::Tabs,
        }
    }
}
