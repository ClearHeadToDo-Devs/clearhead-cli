use clap::{Parser, Subcommand};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use rpds::HashTrieMap;

type Args = HashTrieMap<String, Value>;

pub fn get_cli_map() -> Result<Args, String> {
    let cli = Cli::parse();

    let args_map: Args = cli.into();

    return Ok(args_map);
}

impl From<Cli> for Args {
    fn from(cli: Cli) -> Self {
        return HashTrieMap::new()
            .insert(
                "config".to_string(),
                cli.config.map_or(Value::Null, |p| {
                    Value::String(p.into_os_string().to_string_lossy().to_string())
                }),
            )
            .insert(
                "debug".to_string(),
                Value::Number(serde_json::Number::from(cli.debug)),
            )
            .insert(
                "command".to_string(),
                serde_json::to_value(cli.command).unwrap(),
            );
    }
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
