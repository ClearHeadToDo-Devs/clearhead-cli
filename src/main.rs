use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::{Map, Value};
mod argparser;
use argparser::get_cli_map;

pub mod environment_reader;
use environment_reader::get_config_map;

fn main() {
    let cli = get_cli_map().expect("Failed to parse CLI arguments");

    let config_map = get_config_map(cli["config"].as_str().map(|s| PathBuf::from(s)));

    let opts = merge_hashmaps(&config_map, &cli);

    match opts.get("debug").unwrap().as_u64().unwrap() {
        0 => println!("Debug mode is off"),
        1 => println!("Debug mode is kind of on"),
        2 => println!("Debug mode is on"),
        _ => println!("Don't be crazy"),
    }

    process_subcommand(&opts);
}

fn merge_hashmaps(
    config: &HashMap<String, Value>,
    args: &HashMap<String, Value>,
) -> HashMap<String, Value> {
    let mut merged = config.clone();
    for (key, value) in args {
        merged.insert(key.clone(), value.clone());
    }
    merged
}

fn process_subcommand(opts: &HashMap<String, Value>) {
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
