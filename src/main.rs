use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;

mod argparser;
use argparser::{parse_cli, Commands};

pub mod environment_reader;

fn main() {
    let cli = parse_cli();

    if cli.debug > 0 {
        eprintln!("Debug mode enabled (level: {})", cli.debug);
        eprintln!("Config file: {:?}", cli.config);
    }

    if let Err(e) = run_command(&cli) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn run_command(cli: &argparser::Cli) -> Result<(), String> {
    match &cli.command {
        Commands::Read { file, format, all: _ } => {
            // Read input from file or stdin
            let content = read_input(file.as_ref())?;

            // Parse the .actions content
            let actions = cliche::get_action_list_struct(&serde_json::json!({}), &content)?;

            // Format and output
            let output_format = (*format).into();
            let formatted = cliche::format(&actions, output_format)?;

            println!("{}", formatted);
            Ok(())
        }
    }
}

/// Read input from a file or stdin
fn read_input(file: Option<&PathBuf>) -> Result<String, String> {
    match file {
        Some(path) => fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e)),
        None => {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|e| format!("Failed to read from stdin: {}", e))?;
            Ok(buffer)
        }
    }
}
