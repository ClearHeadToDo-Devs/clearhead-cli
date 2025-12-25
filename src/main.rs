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
        Commands::Read { file, format, where_clause, sql, select, from, all: _ } => {
            // Resolve format: CLI > Env > Config > Default
            let output_format = format
                .map(|f| f.into())
                .or_else(|| parse_format(&base_config.format).ok())
                .unwrap_or(clearhead_cli::OutputFormat::Actions);

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

            // Parse, then optionally filter with SQL
            let actions = if let Some(sql_query) = sql {
                // Full SQL query
                let all_actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
                clearhead_cli::run_sql_query(&all_actions, sql_query)?
            } else if let Some(where_clause) = where_clause {
                // SQL WHERE clause
                let all_actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
                clearhead_cli::run_sql_where(
                    &all_actions,
                    where_clause,
                    select.as_deref(),
                    from.as_deref(),
                )?
            } else {
                // No filter - parse all actions
                clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?
            };

            // Format and output
            let formatted = clearhead_cli::format(&actions, output_format)?;

            println!("{}", formatted);
            Ok(())
        }
        Commands::Normalize { file, write } => {
            let input_file = file.as_ref();
            let content = read_input(input_file)?;
            let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
            let formatted = clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions)?;

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

            let mut primary_actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &primary_content)?;
            let secondary_actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &secondary_content)?;

            clearhead_cli::patch_action_list(&mut primary_actions, &secondary_actions);

            let formatted = clearhead_cli::format(&primary_actions, clearhead_cli::OutputFormat::Actions)?;

            if *write {
                fs::write(primary, formatted).map_err(|e| format!("Failed to write to primary file: {}", e))?;
            } else {
                println!("{}", formatted);
            }
            Ok(())
        }
        Commands::Agenda { file, days } => {
            use chrono::{Duration, Local};
            use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};

            let input_file = file
                .as_ref()
                .map(|p| p.clone())
                .unwrap_or_else(|| data_dir.join(&base_config.file));

            let content = read_input(Some(&input_file))?;
            let all_actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;

            let now = Local::now();
            // Use start of today to include tasks that happened earlier today
            let start_of_day = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).unwrap();
            let end_date = start_of_day + Duration::days(*days as i64);

            if cli.debug > 0 {
                eprintln!("Agenda range: {} to {}", start_of_day.format("%Y-%m-%d"), end_date.format("%Y-%m-%d"));
            }

            // Project occurrences
            let mut agenda_items = Vec::new();
            let mut completed_instances = std::collections::HashSet::new();

            // First pass: identify completed instances
            for action in &all_actions {
                if action.state == clearhead_cli::entities::ActionState::Completed {
                    if let Some(do_dt) = action.do_date_time {
                        // Store (template_id, date) to skip projected occurrences
                        // We use the ID as a proxy for the template link
                        // In a real system, the instance ID might be {template-id}-{date}
                        // For now, we'll match by Name and Date if the action has no recurrence but matches a template
                        completed_instances.insert((action.name.clone(), do_dt.date_naive()));
                    }
                }
            }

            for action in &all_actions {
                // Skip the actual completed log entries in the second pass
                if action.state == clearhead_cli::entities::ActionState::Completed && action.recurrence.is_none() {
                    continue;
                }

                if action.recurrence.is_some() {
                    // Expand recurrence
                    // limit to 100 to avoid infinite loops or excessive output
                    let occurrences = action.expand_occurrences(100);
                    for occ in occurrences {
                        // Convert rrule Tz to chrono Local for comparison
                        let occ_local = occ.with_timezone(&Local);
                        if occ_local >= start_of_day && occ_local <= end_date {
                            // Check if this specific instance was already completed
                            if !completed_instances.contains(&(action.name.clone(), occ_local.date_naive())) {
                                agenda_items.push((occ_local, action));
                            }
                        }
                    }
                } else if let Some(do_dt) = action.do_date_time {
                    // Single occurrence
                    if do_dt >= start_of_day && do_dt <= end_date {
                        agenda_items.push((do_dt, action));
                    }
                }
            }

            // Sort by date
            agenda_items.sort_by_key(|(dt, _)| *dt);

            // Display as table
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .set_content_arrangement(ContentArrangement::Dynamic);

            table.set_header(vec![
                Cell::new("Date").fg(Color::Cyan),
                Cell::new("Time").fg(Color::Cyan),
                Cell::new("Action").fg(Color::Cyan),
                Cell::new("Context").fg(Color::Cyan),
                Cell::new("Description").fg(Color::Cyan),
            ]);

            if agenda_items.is_empty() {
                println!("No actions scheduled for the next {} days.", days);
                return Ok(());
            }

            for (dt, action) in agenda_items {
                let date_str = dt.format("%Y-%m-%d (%a)").to_string();
                let time_str = dt.format("%H:%M").to_string();
                let name = action.name.clone();
                let contexts = action.context_list.as_ref().map(|c| c.join(", ")).unwrap_or_else(|| "-".to_string());
                let desc = action.description.as_ref().map(|d| d.to_string()).unwrap_or_else(|| "-".to_string());

                table.add_row(vec![
                    Cell::new(date_str),
                    Cell::new(time_str),
                    Cell::new(name),
                    Cell::new(contexts),
                    Cell::new(desc),
                ]);
            }

            println!("Agenda for the next {} days:", days);
            println!("{}", table);

            Ok(())
        }
    }
}

/// Parse format string to OutputFormat
fn parse_format(s: &str) -> Result<clearhead_cli::OutputFormat, String> {
    match s.to_lowercase().as_str() {
        "actions" => Ok(clearhead_cli::OutputFormat::Actions),
        "json" => Ok(clearhead_cli::OutputFormat::Json),
        "xml" => Ok(clearhead_cli::OutputFormat::Xml),
        "table" => Ok(clearhead_cli::OutputFormat::Table),
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
