use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;
mod argparser;
use argparser::get_cli_map;

pub mod environment_reader;
use environment_reader::get_config_map;

fn main() {
    let cli = get_cli_map().expect("Failed to parse CLI arguments");

    let config_map = get_config_map(cli["config"].as_str().map(|s| PathBuf::from(s)));

    let opts = merge_hashmaps(&config_map, &cli);

    if let Some(debug) = cli.get("debug") {
        if debug.as_u64().unwrap_or(0) > 0 {
            println!("Full config: {:#?}", opts);
        }
    }

    process_subcommand(&opts);
}

fn merge_hashmaps(
    config: &HashMap<String, Value>,
    args: &HashMap<String, Value>,
) -> HashMap<String, Value> {
    let mut merged = config.clone();
    for (key, value) in args {
        if value.is_null() {
            continue; // Skip null values
        }

        // if the value is a map, we need to recursively merge it
        if let Some(existing_value) = merged.get(key) {
            if existing_value.is_object() && value.is_object() {
                let existing_map = existing_value.as_object().unwrap();
                let new_map = value.as_object().unwrap();
                let merged_map = merge_hashmaps(
                    &existing_map
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                    &new_map
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                );
                merged.insert(
                    key.clone(),
                    Value::Object(serde_json::Map::from_iter(merged_map)),
                );
                continue;
            }
        }
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
