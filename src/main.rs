use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;
use tracing::{info, debug, warn, error, Level};
use tracing_subscriber::{FmtSubscriber, EnvFilter};

mod argparser;
use argparser::{parse_cli, Commands};

mod lsp;

pub mod environment_reader;
use environment_reader::{ensure_dir_exists, get_data_dir, load_config_with_project_discovery, resolve_file_path};

fn main() {
    let cli = parse_cli();

    // Initialize tracing
    let log_level = match cli.debug {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(io::stderr) // System logs usually go to stderr
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    debug!(debug_level = cli.debug, "Debug mode enabled");
    if let Some(ref config_path) = cli.config {
        debug!(config = ?config_path, "Custom config file specified");
    }

    if let Err(e) = run_command(&cli) {
        error!(error = %e, "Command failed");
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

    if let Some(ref ctx) = project_context {
        debug!(project_root = %ctx.root.display(), "Project root discovered");
    }
    debug!(data_dir = %data_dir.display(), "Data directory resolved");

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

            debug!(format = ?output_format, input_file = %input_file.display(), "Executing Read command");

            // Read input from file
            let content = read_input(Some(&input_file))?;

            // Parse, then optionally filter with SQL
            let actions = if let Some(sql_query) = sql {
                debug!(sql = %sql_query, "Filtering with custom SQL query");
                let all_actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
                clearhead_cli::run_sql_query(&all_actions, sql_query)?
            } else if let Some(where_clause) = where_clause {
                debug!(where_clause = %where_clause, "Filtering with SQL WHERE clause");
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

            info!(action_count = actions.len(), "Parsed actions successfully");

            // Format and output
            let formatted = clearhead_cli::format(&actions, output_format, None)?;

            println!("{}", formatted);
            Ok(())
        }
        Commands::Format { file, write, style, indent_style, indent_width } => {
            let input_file = file.as_ref();
            debug!(input_file = ?input_file, write = *write, "Executing Format command");
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
                    info!(path = %path.display(), "Writing formatted output to file");
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
            debug!(input_file = ?input_file, write = *write, "Executing Normalize command");
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
                    info!(path = %path.display(), "Writing normalized output to file");
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
            debug!(primary = %primary.display(), secondary = %secondary.display(), write = *write, "Executing Patch command");
            let primary_content = fs::read_to_string(primary)
                .map_err(|e| format!("Failed to read primary file: {}", e))?;
            let secondary_content = fs::read_to_string(secondary)
                .map_err(|e| format!("Failed to read secondary file: {}", e))?;

            let mut primary_actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &primary_content)?;
            let secondary_actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &secondary_content)?;

            clearhead_cli::patch_action_list(&mut primary_actions, &secondary_actions);

            let formatted = clearhead_cli::format(&primary_actions, clearhead_cli::OutputFormat::Actions, None)?;

            if *write {
                info!(path = %primary.display(), "Writing patched output to primary file");
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

            debug!(input_file = %input_file.display(), days = *days, "Executing Agenda command");

            let content = read_input(Some(&input_file))?;
            let all_actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;

            let now = Local::now();
            // Use start of today to include tasks that happened earlier today
            let start_of_day = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_local_timezone(Local).unwrap();
            let end_date = start_of_day + Duration::days(*days as i64);

            debug!(range_start = %start_of_day.format("%Y-%m-%d"), range_end = %end_date.format("%Y-%m-%d"), "Agenda projection range");

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

            info!(item_count = agenda_items.len(), "Projected agenda items");

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

            debug!(input_file = %input_file.display(), dry_run = *dry_run, "Executing Archive command");

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

            debug!(log_dir = %resolved_log_dir.display(), "Log directory resolved for archiving");

            ensure_dir_exists(&resolved_log_dir)
                .map_err(|e| format!("Failed to create log directory: {}", e))?;

            let (active_text, result) = clearhead_cli::archive::archive_actions(&content, &input_file, &resolved_log_dir)?;

            fs::write(&input_file, active_text)
                .map_err(|e| format!("Failed to update source file '{}': {}", input_file.display(), e))?;

            info!(archived_count = result.archived_count, log_path = %result.log_path.display(), "Actions archived successfully");
            println!("Archived {} actions to {}", result.archived_count, result.log_path.display());
            Ok(())
        }
        Commands::Export { file, output, open_only } => {
            debug!(input_file = ?file, output = ?output, open_only = *open_only, "Executing Export command");
            // Read input from file or stdin
            let content = read_input(file.as_ref())?;

            // Parse actions
            let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;

            // Format as iCalendar
            let icalendar = clearhead_cli::format_as_icalendar(&actions, *open_only)?;

            // Write to output file or stdout
            if let Some(output_path) = output {
                info!(output_path = %output_path.display(), "Writing iCalendar export to file");
                fs::write(output_path, icalendar)
                    .map_err(|e| format!("Failed to write to file: {}", e))?;
            } else {
                println!("{}", icalendar);
            }
            Ok(())
        }
        Commands::Lsp => {
            info!("Starting Language Server");
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("Failed to start async runtime: {}", e))?;

            rt.block_on(lsp::start_lsp());
            Ok(())
        }
        Commands::SyncEvents { file, dry_run } => {
            use clearhead_cli::events::{action_has_events, emit_event_with_timestamp};
            
            let input_file = file
                .as_ref()
                .map(|p| p.clone())
                .unwrap_or_else(|| resolve_file_path(&config.default_file, &data_dir));

            debug!(input_file = %input_file.display(), dry_run = *dry_run, "Executing SyncEvents command");

            let content = fs::read_to_string(&input_file)
                .map_err(|e| format!("Failed to read file '{}': {}", input_file.display(), e))?;
            
            let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
            let mut sync_count = 0;
            let mut skip_count = 0;

            for action in &actions {
                let uuid_str = action.id.to_string();
                if action_has_events(&uuid_str)? {
                    skip_count += 1;
                    continue;
                }

                if *dry_run {
                    println!("Would sync: {} #{}", action.name, uuid_str);
                } else {
                    let metadata = serde_json::json!({
                        "name": action.name,
                        "priority": action.priority,
                        "contexts": action.context_list,
                        "state": action.state.to_string(),
                        "backfilled": true,
                    });

                    // Use created date if available, otherwise now
                    let timestamp = action.created_date_time
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

                    emit_event_with_timestamp(
                        "action_created",
                        &uuid_str,
                        Some(input_file.to_string_lossy().as_ref()),
                        metadata,
                        &timestamp
                    )?;
                    debug!(action_uuid = %uuid_str, "Backfilled event for action");
                }
                sync_count += 1;
            }

            if *dry_run {
                info!(sync_count, skip_count, "SyncEvents dry run complete");
                println!("Dry run complete. {} actions to sync, {} already present.", sync_count, skip_count);
            } else {
                info!(sync_count, skip_count, "SyncEvents complete");
                println!("Sync complete. {} events backfilled, {} already present.", sync_count, skip_count);
            }
            Ok(())
        }
        Commands::Add { file, name, priority, context, description, write } => {
            use chrono::Local;
            use uuid::Uuid;
            use clearhead_cli::entities::{Action, ActionState};
            use clearhead_cli::events::emit_event;
            use clearhead_cli::crdt::ActionRepository;

            let input_file = file
                .as_ref()
                .map(|p| p.clone())
                .unwrap_or_else(|| resolve_file_path(&config.default_file, &data_dir));

            debug!(name = %name, input_file = %input_file.display(), "Executing Add command");

            // If file doesn't exist, create it
            if !input_file.exists() {
                info!(input_file = %input_file.display(), "Creating new actions file");
                if let Some(parent) = input_file.parent() {
                    fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
                }
                fs::write(&input_file, "").map_err(|e| format!("Failed to create file: {}", e))?;
            }

            // Phase 1: Load Repo (Sync In)
            // This loads CRDT and reconciles any manual file edits
            let mut repo = ActionRepository::load(input_file.clone())
                .map_err(|e| format!("Failed to load repository: {}", e))?;
            
            // Get current state from Source of Truth
            let mut actions = repo.get_actions()
                .map_err(|e| format!("Failed to hydrate actions: {}", e))?;

            let new_id = Uuid::now_v7();
            let new_action = Action {
                id: new_id,
                parent_id: None,
                state: ActionState::NotStarted,
                name: name.clone(),
                description: description.clone(),
                priority: *priority,
                context_list: if context.is_empty() { None } else { Some(context.clone()) },
                do_date_time: None,
                do_duration: None,
                recurrence: None,
                completed_date_time: None,
                created_date_time: Some(Local::now()),
                predecessors: None,
                story: None,
            };

            actions.push(new_action.clone());

            // Preview formatted output
            let formatted = clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, None)?;

            if *write {
                // Phase 2: Save Repo (Sync Out)
                // Updates CRDT, persists to disk, and updates file
                repo.save(&actions)
                    .map_err(|e| format!("Failed to save repository: {}", e))?;
                
                // Emit event (Analytics/History)
                let metadata = serde_json::json!({
                    "name": name,
                    "priority": priority,
                    "contexts": context,
                });
                
                if let Err(e) = emit_event("action_created", &new_id.to_string(), Some(input_file.to_string_lossy().as_ref()), metadata) {
                    warn!(error = %e, "Failed to log data event");
                }
                
                info!(name = %name, id = %new_id, "Action added successfully");
                println!("Added action: {} #{}", name, new_id);
            } else {
                println!("{}", formatted);
            }
            Ok(())
        }
        Commands::Complete { file, query, write } => {
            use chrono::Local;
            use clearhead_cli::entities::ActionState;
            use clearhead_cli::events::emit_event;
            use clearhead_cli::crdt::ActionRepository;

            let input_file = file
                .as_ref()
                .map(|p| p.clone())
                .unwrap_or_else(|| resolve_file_path(&config.default_file, &data_dir));

            debug!(query = %query, input_file = %input_file.display(), "Executing Complete command");

            // Phase 1: Load Repo (Sync In)
            let mut repo = ActionRepository::load(input_file.clone())
                .map_err(|e| format!("Failed to load repository: {}", e))?;
            
            let mut actions = repo.get_actions()
                .map_err(|e| format!("Failed to hydrate actions: {}", e))?;

            let mut found = false;
            let mut action_id = String::new();
            let mut action_name = String::new();

            for action in &mut actions {
                // Check if query matches ID or Name
                let id_match = action.id.to_string().starts_with(query); // Prefix match for ID
                let name_match = action.name.contains(query);

                if id_match || name_match {
                    if action.state != ActionState::Completed {
                        action.state = ActionState::Completed;
                        action.completed_date_time = Some(Local::now());
                        
                        found = true;
                        action_id = action.id.to_string();
                        action_name = action.name.clone();
                        break; // Only complete the first match for now
                    }
                }
            }

            if !found {
                warn!(query = %query, "No matching open action found");
                return Err(format!("No open action found matching '{}'", query));
            }

            let formatted = clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, None)?;

            if *write {
                // Phase 2: Save Repo (Sync Out)
                repo.save(&actions)
                    .map_err(|e| format!("Failed to save repository: {}", e))?;

                // Emit event
                let metadata = serde_json::json!({
                    "name": action_name,
                });

                if let Err(e) = emit_event("action_completed", &action_id, Some(input_file.to_string_lossy().as_ref()), metadata) {
                    warn!(error = %e, "Failed to log data event");
                }

                info!(name = %action_name, id = %action_id, "Action completed successfully");
                println!("Completed action: {} #{}", action_name, action_id);
            } else {
                println!("{}", formatted);
            }
            Ok(())
        }
        Commands::Lint { file } => {
            let input_file = file.as_ref();
            debug!(input_file = ?input_file, "Executing Lint command");
            let content = read_input(input_file)?;
            
            // We need the parsed document with source map for linting
            let parsed = clearhead_cli::get_parsed_document(&content)
                .map_err(|e| format!("Failed to parse document: {}", e))?;

            let diagnostics = clearhead_cli::lint::lint_document(&parsed);

            if diagnostics.is_empty() {
                info!("No linting errors found");
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
                warn!("Linting failed with errors");
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
