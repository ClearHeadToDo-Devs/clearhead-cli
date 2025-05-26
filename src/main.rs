use std::collections::HashMap;

mod argparser;
use argparser::get_cli_map;
use cliche::parse_actions;
mod settings;
use settings::get_config_map;

fn main() {
    let cli = get_cli_map();
    let config_map = get_config_map(cli.get("config").unwrap().into());
    let example_actions = HashMap::from([
        ("action1".to_string(), "value1".to_string()),
        ("action2".to_string(), "value2".to_string()),
    ]);

    // You can see how many times a particular flag or argument occurred
    // Note, only flags can have multiple occurrences
    match cli.get("debug").unwrap().into() {
        0 => println!("Debug mode is off"),
        1 => println!("Debug mode is kind of on"),
        2 => println!("Debug mode is on"),
        _ => println!("Don't be crazy"),
    }

    let cli_command: HashMap<String, argparser::CommandHashValue> =
        cli.get("command").unwrap().into();

    let command_name: String = cli_command.get("Read").unwrap().into();

    match command_name.as_str() {
        "Read" => {
            let all = cli_command.get("all").unwrap().into();
            if all {
                println!("Reading all actions");
            } else {
                println!("Reading specific actions");
            }
        }
        _ => println!("Unknown command"),
    }
}
