use cliche::merge_hashmaps;
use std::path::PathBuf;

use serde_json::Value;
mod argparser;
use argparser::get_cli_map;

pub mod environment_reader;
use environment_reader::get_config_map;

fn main() {
    let cli = get_cli_map().expect("Failed to parse CLI arguments");

    let config_map = get_config_map(match cli.get("config") {
        Some(Value::String(path)) => Some(PathBuf::from(path)),
        _ => None,
    });

    let opts = merge_hashmaps(&config_map, &cli).unwrap();

    if let Some(debug) = opts.get("debug") {
        if debug.as_u64().unwrap_or(0) > 0 {
            println!("Full opts Map: {:#?}", opts);
        }
    }

    process_subcommand(&opts);
}

fn process_subcommand(opts: &Value) {
    if let Some(command) = opts.get("command") {
        if let Some(name) = command.get("name").and_then(Value::as_str) {
            match name {
                "read" => {
                    let all = command.get("all").and_then(Value::as_bool).unwrap_or(false);
                    if all {
                        println!("Reading all actions");
                    } else {
                        println!("Reading specific actions");
                    }
                }
                _ => println!("Unknown command"),
            }
        }
    }
}
