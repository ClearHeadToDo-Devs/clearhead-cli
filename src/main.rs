use clap::Parser;
use std::collections::HashMap;

mod argparser;
use argparser::{Cli, Commands};
use cliche::parse_actions;

fn main() {
    let cli = Cli::parse();
    let example_config = HashMap::from([
        ("key1".to_string(), "value1".to_string()),
        ("key2".to_string(), "value2".to_string()),
    ]);

    let example_actions = HashMap::from([
        ("action1".to_string(), "value1".to_string()),
        ("action2".to_string(), "value2".to_string()),
    ]);

    if let Some(config_path) = cli.config.as_deref() {
        println!("Value for config: {}", config_path.display());
    }

    // You can see how many times a particular flag or argument occurred
    // Note, only flags can have multiple occurrences
    match cli.debug {
        0 => println!("Debug mode is off"),
        1 => println!("Debug mode is kind of on"),
        2 => println!("Debug mode is on"),
        _ => println!("Don't be crazy"),
    }

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command {
        Some(Commands::Read { all }) => {
            if *all {
                println!(
                    "{:?}",
                    parse_actions(example_config, example_actions).unwrap()
                )
            } else {
                println!("Not printing testing lists...");
            }
        }
        None => {}
    }

    // Continued program logic goes here...
}
