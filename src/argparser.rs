use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

type Args = HashMap<String, Value>;
pub fn get_cli_map() -> Result<Args, String> {
    let cli = Cli::parse();

    let args_map: Args = cli.into();

    return Ok(args_map);
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

impl From<Cli> for Args {
    fn from(cli: Cli) -> Self {
        return HashMap::from([
            (
                "config".to_string(),
                cli.config.map_or(Value::Null, |p| {
                    Value::String(p.into_os_string().to_string_lossy().to_string())
                }),
            ),
            (
                "debug".to_string(),
                Value::Number(serde_json::Number::from(cli.debug)),
            ),
            (
                "command".to_string(),
                match cli.command {
                    Commands::Read { all } => Value::Object(serde_json::Map::from_iter(vec![
                        ("name".to_string(), Value::String("read".to_string())),
                        ("all".to_string(), Value::Bool(all)),
                    ])),
                },
            ),
        ]);
    }
}
