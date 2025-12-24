use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// CLI argument parser - returns typed structs for ergonomic access
pub fn parse_cli() -> Cli {
    Cli::parse()
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
    /// Read and display actions from a file or stdin
    Read {
        /// File to read (.actions format). If not provided, reads from stdin
        file: Option<PathBuf>,

        /// Output format
        #[arg(short, long, value_enum)]
        format: Option<Format>,

        /// Path to a Tree-sitter query file (.scm) to filter actions
        #[arg(short, long)]
        query: Option<PathBuf>,

        /// SQL WHERE clause to filter actions (e.g., "priority = 1")
        #[arg(short = 'w', long = "where", conflicts_with = "query")]
        where_clause: Option<String>,

        /// Full SQL query (overrides --where, --select, --from)
        #[arg(long, conflicts_with_all = ["query", "where_clause"])]
        sql: Option<String>,

        /// SQL SELECT clause (default: "id")
        #[arg(long, requires = "where_clause")]
        select: Option<String>,

        /// SQL FROM clause (default: "actions")
        #[arg(long, requires = "where_clause")]
        from: Option<String>,

        /// Read all actions (reserved for future use)
        #[arg(short, long)]
        all: bool,
    },

    /// Ensure all actions in a file have UUIDs and are properly formatted
    Normalize {
        /// File to normalize. If not provided, reads from stdin
        file: Option<PathBuf>,

        /// Overwrite the input file with the normalized version
        #[arg(short, long)]
        write: bool,
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

impl From<Format> for cliche::OutputFormat {
    fn from(f: Format) -> Self {
        match f {
            Format::Actions => cliche::OutputFormat::Actions,
            Format::Json => cliche::OutputFormat::Json,
            Format::Xml => cliche::OutputFormat::Xml,
            Format::Table => cliche::OutputFormat::Table,
        }
    }
}
