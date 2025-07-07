use clap::{Parser, Subcommand};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

type Args = Map<String, Value>;
pub fn get_cli_map() -> Result<Args, String> {
    let cli = Cli::parse();

    let value = serde_json::to_value(cli)
        .map_err(|e| format!("unable to translate cli args to a json value {}", e))?;
    
    value.as_object()
        .ok_or("Failed to convert CLI args to object".to_string())
        .map(|obj| obj.clone())
}

#[derive(Parser, Serialize, Deserialize)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    pub config: Option<PathBuf>,

    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub debug: u8,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Serialize, Deserialize)]
enum Commands {
    Read {
        #[arg(short, long)]
        all: bool,
    },
}
