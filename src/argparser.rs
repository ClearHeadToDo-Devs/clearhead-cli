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
    "priority",
    "due",
    "dur",
    "recurrence",
    "context",
    "description",
    "id",
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
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Read and display actions (workspace-wide by default)
    Read {
        /// Output format
        #[arg(short, long, value_enum)]
        format: Option<Format>,

        /// SPARQL WHERE clause to filter actions (e.g., "?s actions:hasPriority 1")
        #[arg(short = 'w', long = "where")]
        where_clause: Option<String>,

        /// Full SPARQL query (overrides --where)
        #[arg(long, conflicts_with = "where_clause")]
        sparql: Option<String>,

        /// Table column filtering options
        #[command(flatten)]
        table_options: CliTableOptions,

        /// Input source (defaults to workspace-wide read)
        #[command(subcommand)]
        source: Option<ReadSource>,
    },

    /// Execute a SPARQL query against the actions database
    Query {
        /// The SPARQL query string
        query: Option<String>,

        /// Read query from file
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

    /// Format actions file with proper spacing and layout
    Format {
        /// File to format. If not provided, reads from stdin
        file: Option<PathBuf>,

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

    /// Ensure all actions in a file have UUIDs (formats by default)
    Normalize {
        /// File to normalize. If not provided, reads from stdin
        file: Option<PathBuf>,

        /// Overwrite the input file with the normalized version
        #[arg(short, long)]
        write: bool,

        /// Skip formatting after adding UUIDs
        #[arg(long)]
        no_format: bool,
    },

    /// Apply changes from a secondary patch file to a primary source file
    Patch {
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

    /// Show an agenda of upcoming actions, including expanded recurring instances
    Agenda {
        /// File to read (.actions format). If not provided, reads from stdin
        file: Option<PathBuf>,

        /// Number of days to project forward
        #[arg(short, long, default_value = "7")]
        days: u32,
    },

    /// Move completed actions to a log file
    Archive {
        /// File to archive from. If not provided, uses the default file
        file: Option<PathBuf>,

        /// Directory to store logs (defaults to logs/ in data_dir)
        #[arg(short, long)]
        log_dir: Option<PathBuf>,

        /// Dry run: show what would be archived without moving
        #[arg(long)]
        dry_run: bool,
    },

    /// Start the Language Server Protocol (LSP) server
    Lsp,

    /// Add a new action to a file
    Add {
        /// Name of the action
        name: String,

        /// File to add to. If not provided, uses the default file
        file: Option<PathBuf>,

        /// Action fields (priority, context, description, alias, state)
        #[command(flatten)]
        fields: ActionFields,

        /// Overwrite the input file with the added action
        #[arg(short, long)]
        write: bool,
    },

    /// Update an existing action
    Update {
        /// UUID, short UUID, alias, or name of the action to update
        query: String,

        /// File containing the action. If not provided, uses the default file
        file: Option<PathBuf>,

        /// New name for the action
        #[arg(short, long)]
        name: Option<String>,

        /// Action fields to update (priority, context, description, alias, state)
        #[command(flatten)]
        fields: ActionFields,

        /// Overwrite the input file with the updates
        #[arg(short, long)]
        write: bool,
    },

    /// Mark an action as completed
    Complete {
        /// UUID or name of the action to complete
        query: String,

        /// File containing the action. If not provided, uses the default file
        file: Option<PathBuf>,

        /// Overwrite the input file with the completed action
        #[arg(short, long)]
        write: bool,
    },

    /// Delete an action
    Delete {
        /// UUID or name of the action to delete
        query: String,

        /// File containing the action. If not provided, uses the default file
        file: Option<PathBuf>,

        /// Overwrite the input file with the deletion
        #[arg(short, long)]
        write: bool,
    },

    /// Synchronize existing actions with the events database (backfill missing events)
    SyncEvents {
        /// File to sync. If not provided, uses the default file
        file: Option<PathBuf>,

        /// Dry run: show what would be synced without writing to DB
        #[arg(long)]
        dry_run: bool,
    },

    /// Lint an actions file for errors and warnings
    Lint {
        /// File to lint. If not provided, reads from stdin
        file: Option<PathBuf>,
    },

    /// Export actions to calendar format (iCalendar)
    Export {
        /// File to export (.actions format). If not provided, reads from stdin
        file: Option<PathBuf>,

        /// Output file path. If not provided, writes to stdout
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Only export open actions (pending, in-progress, blocked)
        #[arg(long)]
        open_only: bool,
    },
}

/// Input source for the read command
#[derive(Subcommand)]
pub enum ReadSource {
    /// Read from a specific file
    File {
        /// Path to the .actions file
        path: PathBuf,
    },
    /// Read from standard input
    Stdio,
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
