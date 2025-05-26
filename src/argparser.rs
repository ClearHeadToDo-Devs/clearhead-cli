use clap::{Parser, Subcommand};
use std::{collections::HashMap, path::PathBuf};

use super::config::{CommandHashValue, ConfigHashValue};

pub fn get_cli_map() -> HashMap<String, ConfigHashValue> {
    let cli = Cli::parse();
    return cli.into();
}

#[derive(Parser)]
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

#[derive(Subcommand)]
enum Commands {
    Read {
        #[arg(short, long)]
        all: bool,
    },
}

impl Into<HashMap<String, ConfigHashValue>> for Cli {
    fn into(self) -> HashMap<String, ConfigHashValue> {
        return HashMap::from([
            ("config".to_string(), ConfigHashValue::Path(self.config)),
            ("debug".to_string(), ConfigHashValue::Int(self.debug)),
            (
                "command".to_string(),
                ConfigHashValue::HashMap(self.command.into()),
            ),
        ]);
    }
}

impl Into<HashMap<String, CommandHashValue>> for Commands {
    fn into(self) -> HashMap<String, CommandHashValue> {
        match self {
            Commands::Read { all } => HashMap::from([
                (
                    "name".to_string(),
                    CommandHashValue::String("read".to_string()),
                ),
                ("all".to_string(), CommandHashValue::Bool(all)),
            ]),
        }
    }
}
