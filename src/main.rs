use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;

mod argparser;
use argparser::{parse_cli, Commands};

pub mod environment_reader;
use environment_reader::{ensure_dir_exists, get_data_dir, load_base_config};

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
    // Load base config (defaults → file → env vars)
    let base_config = load_base_config(cli.config.clone())
        .map_err(|e| format!("Failed to load config: {}", e))?;

    // Get XDG directories
    let data_dir = get_data_dir();
    let config_dir = environment_reader::get_config_dir();

    // Ensure directories exist
    ensure_dir_exists(&data_dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
    ensure_dir_exists(&config_dir)
        .map_err(|e| format!("Failed to create config dir: {}", e))?;

    if cli.debug > 0 {
        eprintln!("Data directory: {}", data_dir.display());
        eprintln!("Config directory: {}", config_dir.display());
        eprintln!("Base config: {:?}", base_config);
    }

    match &cli.command {
        Commands::Read { file, format, query, all: _ } => {
            // Resolve format: CLI > Env > Config > Default
            let output_format = format
                .map(|f| f.into())
                .or_else(|| parse_format(&base_config.format).ok())
                .unwrap_or(cliche::OutputFormat::Actions);

            // Determine input source:
            // - If file specified on CLI: use it
            // - Otherwise: use default file from data_dir
            let input_file = file
                .as_ref()
                .map(|p| p.clone())
                .unwrap_or_else(|| data_dir.join(&base_config.file));

            if cli.debug > 0 {
                eprintln!("Output format: {:?}", output_format);
                eprintln!("Input file: {}", input_file.display());
            }

            // Read input from file
            let content = read_input(Some(&input_file))?;

            // Parse or Query
            let actions = if let Some(query_path) = query {
                // Read query from file
                let query_source = fs::read_to_string(query_path)
                    .map_err(|e| format!("Failed to read query file '{}': {}", query_path.display(), e))?;
                
                cliche::run_query(&content, &query_source)?
            } else {
                // Parse the full .actions content
                cliche::get_action_list_struct(&serde_json::json!({}), &content)?
            };

            // Format and output
            let formatted = cliche::format(&actions, output_format)?;

            println!("{}", formatted);
            Ok(())
        }
        Commands::Normalize { file, write } => {
            let input_file = file.as_ref();
            let content = read_input(input_file)?;
            let actions = cliche::get_action_list_struct(&serde_json::json!({}), &content)?;
            let formatted = cliche::format(&actions, cliche::OutputFormat::Actions)?;

            if *write {
                if let Some(path) = input_file {
                    fs::write(path, formatted).map_err(|e| format!("Failed to write to file: {}", e))?;
                } else {
                    return Err("Cannot use --write without specifying a file".to_string());
                }
            } else {
                println!("{}", formatted);
            }
            Ok(())
        }
        Commands::Patch { primary, secondary, write } => {
            let primary_content = fs::read_to_string(primary)
                .map_err(|e| format!("Failed to read primary file: {}", e))?;
            let secondary_content = fs::read_to_string(secondary)
                .map_err(|e| format!("Failed to read secondary file: {}", e))?;

            let mut primary_actions = cliche::get_action_list_struct(&serde_json::json!({}), &primary_content)?;
            let secondary_actions = cliche::get_action_list_struct(&serde_json::json!({}), &secondary_content)?;

            cliche::patch_action_list(&mut primary_actions, &secondary_actions);

            let formatted = cliche::format(&primary_actions, cliche::OutputFormat::Actions)?;

            if *write {
                fs::write(primary, formatted).map_err(|e| format!("Failed to write to primary file: {}", e))?;
            } else {
                println!("{}", formatted);
            }
            Ok(())
        }
    }
}

/// Parse format string to OutputFormat
fn parse_format(s: &str) -> Result<cliche::OutputFormat, String> {
    match s.to_lowercase().as_str() {
        "actions" => Ok(cliche::OutputFormat::Actions),
        "json" => Ok(cliche::OutputFormat::Json),
        "xml" => Ok(cliche::OutputFormat::Xml),
        "table" => Ok(cliche::OutputFormat::Table),
        _ => Err(format!("Unknown format: {}", s)),
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
