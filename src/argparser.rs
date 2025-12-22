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
        #[arg(short, long, value_enum, default_value = "actions")]
        format: Format,

        /// Read all actions (reserved for future use)
        #[arg(short, long)]
        all: bool,
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
