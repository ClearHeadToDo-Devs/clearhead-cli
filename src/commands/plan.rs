use std::fs;
use tracing::{debug, info};

use crate::argparser;
use crate::commands::{CommandContext, load_repo, parse_format, read_input, save_repo, try_emit};
use clearhead_cli::telemetry::{TelemetryEvent, event_from_field_change};

pub fn read_plans(
    ctx: &CommandContext,
    format: &Option<argparser::Format>,
    where_clause: &Option<String>,
    sparql: &Option<String>,
    sparql_file: &Option<std::path::PathBuf>,
    file: &Option<std::path::PathBuf>,
    stdio: bool,
    table_options: &argparser::CliTableOptions,
) -> Result<(), String> {
    use crate::environment_reader::resolve_file_path;

    let output_format = format
        .map(|f| f.into())
        .or_else(|| parse_format(&ctx.config.cli_format).ok())
        .unwrap_or(clearhead_cli::OutputFormat::Actions);

    let lib_table_opts = if output_format == clearhead_cli::OutputFormat::Table {
        if let Some(ref cols) = table_options.columns {
            argparser::validate_column_names(cols)?;
        }
        if let Some(ref hide) = table_options.hide_columns {
            argparser::validate_column_names(hide)?;
        }
        Some(table_options.to_lib_opts())
    } else {
        None
    };

    let actions = if let Some(path) = file {
        let resolved = resolve_file_path(&path.to_string_lossy(), &ctx.data_dir);
        debug!(file = %resolved.display(), "Reading file");
        let content = read_input(Some(&resolved))?;
        clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?
    } else if stdio {
        debug!("Reading stdin");
        let content = read_input(None)?;
        clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?
    } else {
        debug!(data_dir = %ctx.data_dir.display(), "Reading workspace");

        // Handle SPARQL from file
        let sparql_query = if let Some(sparql_path) = sparql_file {
            Some(
                fs::read_to_string(sparql_path)
                    .map_err(|e| format!("Failed to read SPARQL file: {}", e))?,
            )
        } else {
            sparql.clone()
        };

        if let Some(query) = &sparql_query {
            let workspace = clearhead_cli::workspace::load_workspace_with_sources(&ctx.data_dir)?;
            debug!(sparql = %query, "Filtering with SPARQL query");
            clearhead_cli::run_workspace_sql_query(&workspace, query)?
        } else if let Some(where_clause) = where_clause {
            let workspace = clearhead_cli::workspace::load_workspace_with_sources(&ctx.data_dir)?;
            debug!(where_clause = %where_clause, "Filtering with WHERE clause");
            clearhead_cli::run_workspace_sql_where(&workspace, where_clause, None, None)?
        } else {
            clearhead_cli::workspace::load_workspace_actions(&ctx.data_dir)?
        }
    };

    info!(action_count = actions.len(), "Loaded actions");
    println!(
        "{}",
        clearhead_cli::format(&actions, output_format, None, lib_table_opts.as_ref())?
    );
    Ok(())
}

pub fn show_plan(
    ctx: &CommandContext,
    query: &str,
    file: &Option<std::path::PathBuf>,
    format: &Option<argparser::Format>,
    table_options: &argparser::CliTableOptions,
) -> Result<(), String> {
    let input_file = ctx.resolve_action_file(file.as_ref());
    debug!(query = %query, input_file = %input_file.display(), "Executing Show Plan");

    let (_, actions) = load_repo(&input_file)?;

    let resolved = clearhead_cli::resolve_reference(&actions, query)
        .ok_or_else(|| format!("No plan found matching '{}'", query))?;

    let single = vec![actions[resolved.index].clone()];

    let output_format = format
        .map(|f| f.into())
        .or_else(|| parse_format(&ctx.config.cli_format).ok())
        .unwrap_or(clearhead_cli::OutputFormat::Actions);

    let lib_table_opts = if output_format == clearhead_cli::OutputFormat::Table {
        if let Some(ref cols) = table_options.columns {
            argparser::validate_column_names(cols)?;
        }
        if let Some(ref hide) = table_options.hide_columns {
            argparser::validate_column_names(hide)?;
        }
        Some(table_options.to_lib_opts())
    } else {
        None
    };

    println!(
        "{}",
        clearhead_cli::format(&single, output_format, None, lib_table_opts.as_ref())?
    );
    Ok(())
}

pub fn add_plan(
    ctx: &CommandContext,
    name: &str,
    file: &Option<std::path::PathBuf>,
    fields: &argparser::ActionFields,
    write: bool,
) -> Result<(), String> {
    use chrono::Local;
    use clearhead_cli::{Action, ActionState};
    use uuid::Uuid;

    let input_file = ctx.resolve_action_file(file.as_ref());
    debug!(name = %name, input_file = %input_file.display(), "Executing Add Plan");

    if !input_file.exists() {
        info!(input_file = %input_file.display(), "Creating new actions file");
        if let Some(parent) = input_file.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
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
        name: name.to_string(),
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

    if write {
        save_repo(&mut repo, &actions)?;

        try_emit(
            &new_id,
            TelemetryEvent::ActionCreated {
                name: name.to_string(),
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

pub fn update_plan(
    ctx: &CommandContext,
    query: &str,
    file: &Option<std::path::PathBuf>,
    name: &Option<String>,
    fields: &argparser::ActionFields,
    write: bool,
) -> Result<(), String> {
    use clearhead_cli::{ActionUpdate, apply_updates, resolve_reference};

    let input_file = ctx.resolve_action_file(file.as_ref());
    debug!(query = %query, input_file = %input_file.display(), "Executing Update Plan");

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

    if write {
        save_repo(&mut repo, &actions)?;

        use clearhead_core::diff_actions;
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

pub fn complete_plan(
    ctx: &CommandContext,
    query: &str,
    file: &Option<std::path::PathBuf>,
    write: bool,
) -> Result<(), String> {
    let input_file = ctx.resolve_action_file(file.as_ref());
    debug!(query = %query, input_file = %input_file.display(), "Executing Complete Plan");

    let (mut repo, mut actions) = load_repo(&input_file)?;

    let result = crate::commands::complete::complete_action(&mut actions, query)?;

    let formatted =
        clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, None, None)?;

    if write {
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

pub fn delete_plan(
    ctx: &CommandContext,
    query: &str,
    file: &Option<std::path::PathBuf>,
    write: bool,
) -> Result<(), String> {
    let input_file = ctx.resolve_action_file(file.as_ref());
    debug!(query = %query, input_file = %input_file.display(), "Executing Delete Plan");

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

    if write {
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
        let formatted =
            clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions, None, None)?;
        println!("{}", formatted);
    }
    Ok(())
}

pub fn archive_plans(
    ctx: &CommandContext,
    file: &Option<std::path::PathBuf>,
    log_dir: &Option<std::path::PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    use crate::environment_reader::ensure_dir_exists;

    let input_file = ctx.resolve_action_file(file.as_ref());
    debug!(input_file = %input_file.display(), dry_run = dry_run, "Executing Archive Plans");

    let content = fs::read_to_string(&input_file)
        .map_err(|e| format!("Failed to read file '{}': {}", input_file.display(), e))?;

    if dry_run {
        let all_actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
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

pub fn export_plans(
    _ctx: &CommandContext,
    file: &Option<std::path::PathBuf>,
    output: &Option<std::path::PathBuf>,
    open_only: bool,
) -> Result<(), String> {
    debug!(input_file = ?file, output = ?output, open_only = open_only, "Executing Export Plans");
    let content = read_input(file.as_ref())?;
    let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
    let icalendar = clearhead_cli::format_as_icalendar(&actions, open_only)?;

    if let Some(output_path) = output {
        info!(output_path = %output_path.display(), "Writing iCalendar export to file");
        fs::write(output_path, icalendar).map_err(|e| format!("Failed to write to file: {}", e))?;
    } else {
        println!("{}", icalendar);
    }
    Ok(())
}
