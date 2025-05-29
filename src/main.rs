use cliche::merge_hashmaps;
use std::collections::HashMap;
use std::path::PathBuf;

use serde_json::Value;
mod argparser;
use argparser::get_cli_map;

pub mod environment_reader;
use environment_reader::get_config_map;

use tree_sitter::Node;
use tree_sitter::Parser;
use tree_sitter::Tree;
use tree_sitter_actions::LANGUAGE;

type ActionTree = Tree;

fn main() {
    let cli = get_cli_map().expect("Failed to parse CLI arguments");

    let config_map = get_config_map(match cli.get("config") {
        Some(Value::String(path)) => Some(PathBuf::from(path)),
        _ => None,
    });

    let opts = merge_hashmaps(&config_map, &cli);

    if let Some(debug) = opts.get("debug") {
        if debug.as_u64().unwrap_or(0) > 0 {
            println!("Full opts Map: {:#?}", opts);
        }
    }

    process_subcommand(&opts);
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

// this is the function where we actually use treesitter to parse the actions into the tree, and
// translate that into a proper vector of hashmaps so that we are passing back plain data
// fn get_action_list(opts: &HashMap<String, Value>, actions: String) -> Vec<HashMap<String, Value>> {
//     let action_parser = tree_sitter::Parser::new();
//
//     action_parser
//         .set_language(&tree_sitter_actions::LANGUAGE.into())
//         .expect("Failed to set language for tree-sitter parser");
//
//     let tree = action_parser
//         .parse(actions.into(), None)
//         .expect("Failed to parse actions");
//
//     // Here we would traverse the tree and extract the actions into a vector of hashmaps.
// }
