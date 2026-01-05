use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;

mod argparser;
use argparser::{parse_cli, Commands};

mod lsp;

pub mod environment_reader;
use environment_reader::{ensure_dir_exists, get_data_dir, load_config_with_project_discovery, resolve_file_path};

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
    // Load config with automatic project discovery
    // This returns both the config and the discovered project context
    let (config, project_context) = load_config_with_project_discovery(cli.config.clone())
        .map_err(|e| format!("Failed to load config: {}", e))?;

    // Resolve directories (with shell expansion)
    let data_dir = resolve_file_path(&config.data_dir, &get_data_dir());
    let config_dir = resolve_file_path(&config.config_dir, &environment_reader::get_config_dir());

    // Ensure directories exist
    ensure_dir_exists(&data_dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
    ensure_dir_exists(&config_dir)
        .map_err(|e| format!("Failed to create config dir: {}", e))?;

    if cli.debug > 0 {
        if let Some(ref ctx) = project_context {
            eprintln!("Project root discovered: {}", ctx.root.display());
        }
        eprintln!("Data directory: {}", data_dir.display());
        eprintln!("Config: {:?}", config);
    }

    match &cli.command {
        Commands::Read { file, format, where_clause, sql, select, from, all: _ } => {
            // Resolve format: CLI > Env > Config > Default
            let output_format = format
                .map(|f| f.into())
                .or_else(|| parse_format(&config.cli_format).ok())
                .unwrap_or(clearhead_cli::OutputFormat::Actions);

            // Determine input source:
            // 1. Explicit CLI argument
            // 2. Project-local default (next.actions or .clearhead/inbox.actions)
            // 3. Global default (from config)
            let input_file = file
                .as_ref()
                .map(|p| p.clone())
                .or_else(|| project_context.as_ref().and_then(|ctx| ctx.default_file.clone()))
                .unwrap_or_else(|| resolve_file_path(&config.default_file, &data_dir));

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
            let formatted = clearhead_cli::format(&actions, output_format, None)?;

            println!("{}", formatted);
            Ok(())
        }
        Commands::Format { file, write, style, indent_style, indent_width } => {
            let input_file = file.as_ref();
            let content = read_input(input_file)?;
            let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;

            // Resolve indent settings: CLI > Config/Env > Default
            let resolved_indent_style = indent_style
                .clone()
                .map(|i| i.into())
                .unwrap_or_else(|| parse_indent_style(&config.cli_indent_style));

            let resolved_indent_width = indent_width
                .unwrap_or(config.cli_indent_width);

            // Format with style options, preserving existing UUIDs
            let format_config = clearhead_cli::FormatConfig {
                style: style.clone().map(|s| s.into()).unwrap_or(clearhead_cli::FormatStyle::Compact),
                indent_style: resolved_indent_style,
                indent_width: resolved_indent_width,
                include_id: true,  // Preserve existing UUIDs (don't add new ones)
            };

            let formatted = clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, Some(format_config))?;

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
        Commands::Normalize { file, write, no_format } => {
            let input_file = file.as_ref();
            let content = read_input(input_file)?;
            // Parse and ensure all actions have UUIDs (parser adds them automatically)
            let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;

            let output = if *no_format {
                // Just output with UUIDs, no formatting
                clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, None)?
            } else {
                // Resolve indent settings from config
                let resolved_indent_style = parse_indent_style(&config.cli_indent_style);
                let resolved_indent_width = config.cli_indent_width;

                // Format with default compact style and UUIDs
                let format_config = clearhead_cli::FormatConfig {
                    style: clearhead_cli::FormatStyle::Compact,
                    indent_style: resolved_indent_style,
                    indent_width: resolved_indent_width,
                    include_id: true,
                };
                clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, Some(format_config))?
            };

            if *write {
                if let Some(path) = input_file {
                    fs::write(path, output).map_err(|e| format!("Failed to write to file: {}", e))?;
                } else {
                    return Err("Cannot use --write without specifying a file".to_string());
                }
            } else {
                println!("{}", output);
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

            let formatted = clearhead_cli::format(&primary_actions, clearhead_cli::OutputFormat::Actions, None)?;

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
                .unwrap_or_else(|| resolve_file_path(&config.default_file, &data_dir));

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
        Commands::Archive { file, log_dir, dry_run } => {
            let input_file = file
                .as_ref()
                .map(|p| p.clone())
                .unwrap_or_else(|| resolve_file_path(&config.default_file, &data_dir));

            let content = fs::read_to_string(&input_file)
                .map_err(|e| format!("Failed to read file '{}': {}", input_file.display(), e))?;
            
            if *dry_run {
                let all_actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
                let (_, archived_actions) = clearhead_cli::archive::partition_actions_for_archive(&all_actions);
                
                if archived_actions.is_empty() {
                    println!("No completed action trees to archive.");
                    return Ok(());
                }

                println!("Would archive {} actions from {}:", archived_actions.len(), input_file.display());
                for action in &archived_actions {
                    if action.parent_id.is_none() {
                        println!("  - {} (tree)", action.name);
                    }
                }
                return Ok(());
            }

            let resolved_log_dir = log_dir.clone().unwrap_or_else(|| {
                if input_file.to_string_lossy().contains(".clearhead") {
                     input_file.parent().unwrap().join("logs")
                } else {
                     data_dir.join("logs")
                }
            });

            ensure_dir_exists(&resolved_log_dir)
                .map_err(|e| format!("Failed to create log directory: {}", e))?;

            let (active_text, result) = clearhead_cli::archive::archive_actions(&content, &input_file, &resolved_log_dir)?;

            fs::write(&input_file, active_text)
                .map_err(|e| format!("Failed to update source file '{}': {}", input_file.display(), e))?;

            println!("Archived {} actions to {}", result.archived_count, result.log_path.display());
            Ok(())
        }
        Commands::Lsp => {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("Failed to start async runtime: {}", e))?;
            
            rt.block_on(lsp::start_lsp());
            Ok(())
        }
        Commands::Lint { file } => {
            let input_file = file.as_ref();
            let content = read_input(input_file)?;
            
            // We need the parsed document with source map for linting
            let parsed = clearhead_cli::get_parsed_document(&content)
                .map_err(|e| format!("Failed to parse document: {}", e))?;

            let diagnostics = clearhead_cli::lint::lint_document(&parsed);

            if diagnostics.is_empty() {
                // No output on success, standard unix philosophy
                return Ok(());
            }

            let mut has_errors = false;
            for diag in diagnostics {
                let severity_str = match diag.severity {
                    clearhead_cli::LintSeverity::Error => {
                        has_errors = true;
                        "ERROR"
                    },
                    clearhead_cli::LintSeverity::Warning => "WARN",
                    clearhead_cli::LintSeverity::Info => "INFO",
                };

                // Simple format: file:line:col: severity: message [code]
                let file_str = input_file.map(|p| p.display().to_string()).unwrap_or_else(|| "<stdin>".to_string());
                println!(
                    "{}:{}:{}: {}: {} [{}]",
                    file_str,
                    diag.range.start_row + 1,
                    diag.range.start_col + 1,
                    severity_str,
                    diag.message,
                    diag.code
                );
            }

            if has_errors {
                std::process::exit(1);
            }
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

fn parse_indent_style(s: &str) -> clearhead_cli::IndentStyle {
    match s.to_lowercase().as_str() {
        "tabs" => clearhead_cli::IndentStyle::Tabs,
        _ => clearhead_cli::IndentStyle::Spaces,
    }
}
