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

    let example_actions = HashMap::from([
        ("action1".to_string(), "value1".to_string()),
        ("action2".to_string(), "value2".to_string()),
    ]);

    // You can see how many times a particular flag or argument occurred
    // Note, only flags can have multiple occurrences
    match cli.get("debug").unwrap().as_u64().unwrap() {
        0 => println!("Debug mode is off"),
        1 => println!("Debug mode is kind of on"),
        2 => println!("Debug mode is on"),
        _ => println!("Don't be crazy"),
    }

    let cli_command: Map<String, Value> = cli["command"].as_object().unwrap().clone();

    let command_name: String = cli_command["name"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();

    match command_name.as_str() {
        "read" => {
            let all = cli_command["all"].as_bool().unwrap_or(false);
            if all {
                println!("Reading all actions");
            } else {
                println!("Reading specific actions");
            }
        }
        _ => println!("Unknown command"),
    }
}
