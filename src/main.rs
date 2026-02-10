use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;
use tracing::{Level, debug, error, info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

mod argparser;
use argparser::{Commands, parse_cli};

mod lsp;

pub mod environment_reader;
use environment_reader::{ensure_dir_exists, resolve_file_path};

mod commands;
use commands::{CommandContext, load_repo, save_repo, try_emit, write_or_print};

use clearhead_cli::telemetry::{
    TelemetryEvent, TelemetryRecord, Tool, emit, event_from_field_change,
};

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
    let ctx = CommandContext::new(cli)?;

    debug!(data_dir = %ctx.data_dir.display(), "Data directory resolved");

    match &cli.command {
        Commands::Read {
            format,
            where_clause,
            sparql,
            table_options,
            source,
        } => {
            use argparser::ReadSource;

            let output_format = format
                .map(|f| f.into())
                .or_else(|| parse_format(&ctx.config.cli_format).ok())
                .unwrap_or(clearhead_cli::OutputFormat::Actions);

            let has_filter = sparql.is_some() || where_clause.is_some();

            if source.is_some() && has_filter {
                return Err(
                    "SPARQL filtering (--where, --sparql) is only supported for workspace reads. \
                           Use 'read' without a subcommand to query the workspace."
                        .to_string(),
                );
            }

            let lib_table_opts = if output_format == clearhead_cli::OutputFormat::Table {
                if let Some(ref cols) = table_options.columns {
                    argparser::validate_column_names(cols)?;
                }
                if let Some(ref hide) = table_options.hide_columns {
                    argparser::validate_column_names(hide)?;
                }
                Some(&table_options.to_lib_opts())
            } else {
                None
            };

            let actions = match source {
                None => {
                    debug!(data_dir = %ctx.data_dir.display(), "Reading workspace");

                    if let Some(query) = sparql {
                        let workspace =
                            clearhead_cli::workspace::load_workspace_with_sources(&ctx.data_dir)?;
                        debug!(sparql = %query, "Filtering with SPARQL query");
                        clearhead_cli::run_workspace_sql_query(&workspace, query)?
                    } else if let Some(where_clause) = where_clause {
                        let workspace =
                            clearhead_cli::workspace::load_workspace_with_sources(&ctx.data_dir)?;
                        debug!(where_clause = %where_clause, "Filtering with WHERE clause");
                        clearhead_cli::run_workspace_sql_where(
                            &workspace,
                            where_clause,
                            None,
                            None,
                        )?
                    } else {
                        clearhead_cli::workspace::load_workspace_actions(&ctx.data_dir)?
                    }
                }
                Some(ReadSource::File { path }) => {
                    let resolved = resolve_file_path(&path.to_string_lossy(), &ctx.data_dir);
                    debug!(file = %resolved.display(), "Reading file");
                    let content = read_input(Some(&resolved))?;
                    clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?
                }
                Some(ReadSource::Stdio) => {
                    debug!("Reading stdin");
                    let content = read_input(None)?;
                    clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?
                }
            };

            info!(action_count = actions.len(), "Loaded actions");
            println!(
                "{}",
                clearhead_cli::format(&actions, output_format, None, lib_table_opts)?
            );
            Ok(())
        }
        Commands::Query { query, file } => {
            let sparql = if let Some(q) = query {
                q.clone()
            } else if let Some(path) = file {
                fs::read_to_string(path).map_err(|e| format!("Failed to read query file: {}", e))?
            } else {
                return Err("Must provide either query string or file".to_string());
            };

            let workspace = clearhead_cli::workspace::load_workspace_with_sources(&ctx.data_dir)?;
            let actions = clearhead_cli::run_workspace_sql_query(&workspace, &sparql)?;

            println!(
                "{}",
                clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, None, None)?
            );
            Ok(())
        }
        Commands::Format {
            file,
            write,
            style,
            indent_style,
            indent_width,
        } => {
            let input_file = file.as_ref();
            debug!(input_file = ?input_file, write = *write, "Executing Format command");
            let content = read_input(input_file)?;
            let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;

            let (config_indent_style, config_indent_width) = ctx.indent_config();

            let resolved_indent_style = indent_style
                .clone()
                .map(|i| i.into())
                .unwrap_or(config_indent_style);

            let resolved_indent_width = indent_width.unwrap_or(config_indent_width);

            let format_config = clearhead_cli::FormatConfig {
                style: style
                    .clone()
                    .map(|s| s.into())
                    .unwrap_or(clearhead_cli::FormatStyle::Compact),
                indent_style: resolved_indent_style,
                indent_width: resolved_indent_width,
                include_id: true,
            };

            let formatted = clearhead_cli::format(
                &actions,
                clearhead_cli::OutputFormat::Actions,
                Some(format_config),
                None,
            )?;

            write_or_print(&formatted, *write, input_file)?;
            Ok(())
        }
        Commands::Normalize {
            file,
            write,
            no_format,
        } => {
            let input_file = file.as_ref();
            debug!(input_file = ?input_file, write = *write, "Executing Normalize command");
            let content = read_input(input_file)?;
            let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;

            let output = if *no_format {
                clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, None, None)?
            } else {
                let (resolved_indent_style, resolved_indent_width) = ctx.indent_config();

                let format_config = clearhead_cli::FormatConfig {
                    style: clearhead_cli::FormatStyle::Compact,
                    indent_style: resolved_indent_style,
                    indent_width: resolved_indent_width,
                    include_id: true,
                };
                clearhead_cli::format(
                    &actions,
                    clearhead_cli::OutputFormat::Actions,
                    Some(format_config),
                    None,
                )?
            };

            write_or_print(&output, *write, input_file)?;
            Ok(())
        }
        Commands::Patch {
            primary,
            secondary,
            write,
        } => {
            debug!(primary = %primary.display(), secondary = %secondary.display(), write = *write, "Executing Patch command");
            let primary_content = fs::read_to_string(primary)
                .map_err(|e| format!("Failed to read primary file: {}", e))?;
            let secondary_content = fs::read_to_string(secondary)
                .map_err(|e| format!("Failed to read secondary file: {}", e))?;

            let mut primary_actions =
                clearhead_cli::get_action_list_struct(&serde_json::json!({}), &primary_content)?;
            let secondary_actions =
                clearhead_cli::get_action_list_struct(&serde_json::json!({}), &secondary_content)?;

            clearhead_cli::patch_action_list(&mut primary_actions, &secondary_actions);

            let formatted = clearhead_cli::format(
                &primary_actions,
                clearhead_cli::OutputFormat::Actions,
                None,
                None,
            )?;

            write_or_print(&formatted, *write, Some(primary))?;
            Ok(())
        }
        Commands::Agenda { file, days } => {
            use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};

            let input_file = ctx.resolve_action_file(file.as_ref());
            debug!(input_file = %input_file.display(), days = *days, "Executing Agenda command");

            let content = read_input(Some(&input_file))?;
            let all_actions =
                clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;

            let agenda_items = commands::agenda::project_agenda(&all_actions, *days);

            info!(item_count = agenda_items.len(), "Projected agenda items");

            if agenda_items.is_empty() {
                println!("No actions scheduled for the next {} days.", days);
                return Ok(());
            }

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

            for item in agenda_items {
                let date_str = item.datetime.format("%Y-%m-%d (%a)").to_string();
                let time_str = item.datetime.format("%H:%M").to_string();
                let name = item.action.name.clone();
                let contexts = item
                    .action
                    .context_list
                    .as_ref()
                    .map(|c| c.join(", "))
                    .unwrap_or_else(|| "-".to_string());
                let desc = item
                    .action
                    .description
                    .as_ref()
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "-".to_string());

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
        Commands::Archive {
            file,
            log_dir,
            dry_run,
        } => {
            let input_file = ctx.resolve_action_file(file.as_ref());
            debug!(input_file = %input_file.display(), dry_run = *dry_run, "Executing Archive command");

            let content = fs::read_to_string(&input_file)
                .map_err(|e| format!("Failed to read file '{}': {}", input_file.display(), e))?;

            if *dry_run {
                let all_actions =
                    clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
                let (_, archived_actions) =
                    clearhead_cli::archive::partition_actions_for_archive(&all_actions);

                if archived_actions.is_empty() {
                    println!("No completed action trees to archive.");
                    return Ok(());
                }

                println!(
                    "Would archive {} actions from {}:",
                    archived_actions.len(),
                    input_file.display()
                );
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
                    ctx.data_dir.join("logs")
                }
            });

            debug!(log_dir = %resolved_log_dir.display(), "Log directory resolved for archiving");

            ensure_dir_exists(&resolved_log_dir)
                .map_err(|e| format!("Failed to create log directory: {}", e))?;

            let (active_text, result) =
                clearhead_cli::archive::archive_actions(&content, &input_file, &resolved_log_dir)?;

            fs::write(&input_file, active_text).map_err(|e| {
                format!(
                    "Failed to update source file '{}': {}",
                    input_file.display(),
                    e
                )
            })?;

            info!(archived_count = result.archived_count, log_path = %result.log_path.display(), "Actions archived successfully");
            println!(
                "Archived {} actions to {}",
                result.archived_count,
                result.log_path.display()
            );
            Ok(())
        }
        Commands::Export {
            file,
            output,
            open_only,
        } => {
            debug!(input_file = ?file, output = ?output, open_only = *open_only, "Executing Export command");
            let content = read_input(file.as_ref())?;
            let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
            let icalendar = clearhead_cli::format_as_icalendar(&actions, *open_only)?;

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
            let input_file = ctx.resolve_action_file(file.as_ref());
            debug!(input_file = %input_file.display(), dry_run = *dry_run, "Executing SyncEvents command");

            let content = fs::read_to_string(&input_file)
                .map_err(|e| format!("Failed to read file '{}': {}", input_file.display(), e))?;

            let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
            let mut sync_count = 0;
            let skip_count = 0; // TODO: track which events already exist

            for action in &actions {
                let uuid_str = action.id.to_string();

                if *dry_run {
                    println!("Would sync: {} #{}", action.name, uuid_str);
                } else {
                    let timestamp = action
                        .created_date_time
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(chrono::Utc::now);

                    let record = TelemetryRecord::with_timestamp(
                        timestamp,
                        Tool::Cli,
                        Some(uuid_str.clone()),
                        TelemetryEvent::ActionCreated {
                            name: action.name.clone(),
                            file_path: input_file.display().to_string(),
                        },
                    );

                    if let Err(e) = emit(record) {
                        warn!(error = %e, "Failed to emit backfill event");
                    }

                    debug!(action_uuid = %uuid_str, "Backfilled event for action");
                }
                sync_count += 1;
            }

            if *dry_run {
                info!(sync_count, skip_count, "SyncEvents dry run complete");
                println!(
                    "Dry run complete. {} actions to sync, {} already present.",
                    sync_count, skip_count
                );
            } else {
                info!(sync_count, skip_count, "SyncEvents complete");
                println!(
                    "Sync complete. {} events backfilled, {} already present.",
                    sync_count, skip_count
                );
            }
            Ok(())
        }
        Commands::Add {
            file,
            name,
            fields,
            write,
        } => {
            use chrono::Local;
            use clearhead_cli::{Action, ActionState};
            use uuid::Uuid;

            let input_file = ctx.resolve_action_file(file.as_ref());
            debug!(name = %name, input_file = %input_file.display(), "Executing Add command");

            // If file doesn't exist, create it
            if !input_file.exists() {
                info!(input_file = %input_file.display(), "Creating new actions file");
                if let Some(parent) = input_file.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| format!("Failed to create directory: {}", e))?;
                }
                fs::write(&input_file, "").map_err(|e| format!("Failed to create file: {}", e))?;
            }

            let (mut repo, mut actions) = load_repo(&input_file)?;

            let new_id = Uuid::now_v7();
            let new_action = Action {
                id: new_id,
                parent_id: None,
                state: fields
                    .state
                    .map(|s| s.into())
                    .unwrap_or(ActionState::NotStarted),
                name: name.clone(),
                description: fields.description.clone(),
                priority: fields.priority,
                context_list: if fields.context.is_empty() {
                    None
                } else {
                    Some(fields.context.clone())
                },
                do_date_time: None,
                do_duration: None,
                recurrence: None,
                completed_date_time: None,
                created_date_time: Some(Local::now()),
                predecessors: None,
                story: None,
                alias: fields.alias.clone(),
                is_sequential: None,
            };

            actions.push(new_action.clone());

            let formatted =
                clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, None, None)?;

            if *write {
                save_repo(&mut repo, &actions)?;

                try_emit(
                    &new_id,
                    TelemetryEvent::ActionCreated {
                        name: name.clone(),
                        file_path: input_file.display().to_string(),
                    },
                );

                info!(name = %name, id = %new_id, "Action added successfully");
                println!("Added action: {} #{}", name, new_id);
            } else {
                println!("{}", formatted);
            }
            Ok(())
        }
        Commands::Complete { file, query, write } => {
            let input_file = ctx.resolve_action_file(file.as_ref());
            debug!(query = %query, input_file = %input_file.display(), "Executing Complete command");

            let (mut repo, mut actions) = load_repo(&input_file)?;

            let result = commands::complete::complete_action(&mut actions, query)?;

            let formatted =
                clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, None, None)?;

            if *write {
                save_repo(&mut repo, &actions)?;
                try_emit(&result.action_id, result.event);

                if result.is_recurring {
                    println!(
                        "Completed instance of recurring action: {} #{}",
                        result.action_name, result.action_id
                    );
                    println!("Template updated for next occurrence.");
                } else {
                    info!(name = %result.action_name, id = %result.action_id, "Action completed successfully");
                    println!(
                        "Completed action: {} #{}",
                        result.action_name, result.action_id
                    );
                }
            } else {
                println!("{}", formatted);
            }
            Ok(())
        }
        Commands::Update {
            file,
            query,
            name,
            fields,
            write,
        } => {
            use clearhead_cli::{ActionUpdate, apply_updates, resolve_reference};

            let input_file = ctx.resolve_action_file(file.as_ref());
            debug!(query = %query, input_file = %input_file.display(), "Executing Update command");

            let (mut repo, mut actions) = load_repo(&input_file)?;

            let resolved = resolve_reference(&actions, query)
                .ok_or_else(|| format!("No action found matching '{}'", query))?;

            debug!(index = resolved.index, match_type = ?resolved.match_type, "Resolved action reference");

            let mut updates: ActionUpdate = fields.clone().into();
            updates.name = name.clone();

            let old_action = actions[resolved.index].clone();
            let action_id = old_action.id;

            apply_updates(&mut actions[resolved.index], updates);

            let new_action = actions[resolved.index].clone();
            let action_name = new_action.name.clone();

            let formatted =
                clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, None, None)?;

            if *write {
                save_repo(&mut repo, &actions)?;

                use clearhead_core::diff::diff_actions;
                let changes = diff_actions(&vec![old_action], &vec![new_action]);
                if let Some(action_diff) = changes.modified.first() {
                    for change in &action_diff.changes {
                        if let Some(evt) = event_from_field_change(change) {
                            try_emit(&action_id, evt);
                        }
                    }
                }

                info!(name = %action_name, id = %action_id, "Action updated successfully");
                println!("Updated action: {} #{}", action_name, action_id);
            } else {
                println!("{}", formatted);
            }
            Ok(())
        }
        Commands::Delete { file, query, write } => {
            let input_file = ctx.resolve_action_file(file.as_ref());
            debug!(query = %query, input_file = %input_file.display(), "Executing Delete command");

            let (mut repo, mut actions) = load_repo(&input_file)?;

            let target_index = actions
                .iter()
                .position(|action| {
                    let id_match = action.id.to_string().starts_with(query);
                    let name_match = action.name.contains(query);
                    id_match || name_match
                })
                .ok_or_else(|| format!("No action found matching '{}'", query))?;

            let action = actions.remove(target_index);

            if *write {
                save_repo(&mut repo, &actions)?;

                try_emit(
                    &action.id,
                    TelemetryEvent::ActionDeleted {
                        name: action.name.clone(),
                    },
                );

                info!(name = %action.name, id = %action.id, "Action deleted successfully");
                println!("Deleted action: {} #{}", action.name, action.id);
            } else {
                let formatted = clearhead_cli::format(
                    &actions,
                    clearhead_cli::OutputFormat::Actions,
                    None,
                    None,
                )?;
                println!("{}", formatted);
            }
            Ok(())
        }
        Commands::Charter { action } => {
            use argparser::CharterAction;
            use clearhead_cli::workspace::{
                CharterSource, discover_charters, load_workspace, resolve_charter,
            };
            use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};

            match action {
                CharterAction::List {
                    format,
                    explicit_only,
                } => {
                    let mut charters = discover_charters(&ctx.data_dir)?;

                    if *explicit_only {
                        charters.retain(|sc| sc.is_explicit());
                    }

                    if charters.is_empty() {
                        println!("No charters found.");
                        return Ok(());
                    }

                    let use_json = matches!(format, Some(argparser::Format::Json));

                    if use_json {
                        let json_charters: Vec<_> = charters.iter().map(|sc| &sc.charter).collect();
                        let json = serde_json::to_string_pretty(&json_charters)
                            .map_err(|e| format!("Failed to serialize charters: {}", e))?;
                        println!("{}", json);
                    } else {
                        let mut table = Table::new();
                        table
                            .load_preset(UTF8_FULL)
                            .set_content_arrangement(ContentArrangement::Dynamic);

                        table.set_header(vec![
                            Cell::new("Name").fg(Color::Cyan),
                            Cell::new("Type").fg(Color::Cyan),
                            Cell::new("Alias").fg(Color::Cyan),
                            Cell::new("Source").fg(Color::Cyan),
                        ]);

                        for sc in &charters {
                            let type_str = if sc.is_explicit() {
                                "explicit"
                            } else {
                                "implicit"
                            };
                            let alias = sc.charter.alias.as_deref().unwrap_or("-");
                            let source_str = match &sc.source {
                                CharterSource::ExplicitFile(p) => p.display().to_string(),
                                CharterSource::ImplicitFromFile(p) => {
                                    format!("{} (inferred)", p.display())
                                }
                                CharterSource::ImplicitFromDirectory(p) => {
                                    format!("{} (inferred)", p.display())
                                }
                            };

                            table.add_row(vec![
                                Cell::new(&sc.charter.title),
                                Cell::new(type_str),
                                Cell::new(alias),
                                Cell::new(source_str),
                            ]);
                        }

                        println!("{}", table);
                    }
                    Ok(())
                }
                CharterAction::Show { query } => {
                    let charters = discover_charters(&ctx.data_dir)?;
                    let found = resolve_charter(&charters, query)
                        .ok_or_else(|| format!("No charter found matching '{}'", query))?;

                    let formatted = clearhead_core::format_charter(&found.charter);
                    println!("{}", formatted);

                    let workspace = load_workspace(&ctx.data_dir)?;
                    let charter_title = found.charter.title.to_lowercase();
                    let plan_count = workspace
                        .actions
                        .sourced_actions
                        .iter()
                        .filter(|sa| {
                            sa.source
                                .project
                                .as_ref()
                                .is_some_and(|p| p.to_lowercase() == charter_title)
                        })
                        .count();

                    if plan_count > 0 {
                        println!("Plans: {}", plan_count);
                    }

                    Ok(())
                }
                CharterAction::Add {
                    title,
                    alias,
                    parent,
                    write,
                } => {
                    use clearhead_core::domain::Charter;

                    let id = uuid::Uuid::now_v7();
                    let charter = Charter {
                        id,
                        title: title.clone(),
                        description: None,
                        alias: alias.clone(),
                        parent: parent.clone(),
                        objectives: None,
                    };

                    let formatted = clearhead_core::format_charter(&charter);

                    if *write {
                        let filename = alias
                            .as_deref()
                            .unwrap_or_else(|| title.as_str())
                            .to_lowercase()
                            .replace(' ', "-")
                            .replace('&', "and");
                        let file_path = ctx.data_dir.join(format!("{}.md", filename));

                        if file_path.exists() {
                            return Err(format!("File already exists: {}", file_path.display()));
                        }

                        fs::write(&file_path, &formatted)
                            .map_err(|e| format!("Failed to write charter file: {}", e))?;
                        info!(title = %title, id = %id, path = %file_path.display(), "Charter created");
                        println!(
                            "Created charter: {} #{}\nWritten to: {}",
                            title,
                            id,
                            file_path.display()
                        );
                    } else {
                        println!("{}", formatted);
                    }
                    Ok(())
                }
            }
        }
        Commands::Lint { file } => {
            let input_file = file.as_ref();
            debug!(input_file = ?input_file, "Executing Lint command");
            let content = read_input(input_file)?;

            let parsed = clearhead_cli::get_parsed_document(&content)
                .map_err(|e| format!("Failed to parse document: {}", e))?;

            let results = clearhead_cli::lint_document(&parsed);

            if results.errors.is_empty() && results.warnings.is_empty() && results.info.is_empty() {
                info!("No linting errors found");
                return Ok(());
            }

            let has_errors = !results.errors.is_empty();
            for diag in results {
                let severity_str = match diag.severity {
                    clearhead_cli::LintSeverity::Error => "ERROR",
                    clearhead_cli::LintSeverity::Warning => "WARN",
                    clearhead_cli::LintSeverity::Info => "INFO",
                };

                let file_str = input_file
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<stdin>".to_string());
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
