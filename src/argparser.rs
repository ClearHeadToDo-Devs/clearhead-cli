use clap::{Parser, Subcommand};
use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub fn get_cli_map() -> HashMap<String, Value> {
    let cli = Cli::parse();
    return Result::expect(cli.try_into(), "Failed to parse CLI arguments");
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

impl Into<HashMap<String, Value>> for Cli {
    fn into(self) -> HashMap<String, Value> {
        return HashMap::from([
            ("config".to_string(), json!(self.config)),
            ("debug".to_string(), json!(self.debug)),
            (
                "command".to_string(),
                serde_json::to_value(self.command).unwrap(),
            ),
        ]);
    }
}
