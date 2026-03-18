use std::fs;
use tracing::{debug, info};

use crate::argparser;
use crate::commands::{CommandContext, load_file, parse_format, read_input, save_file, try_emit};
use clearhead_cli::telemetry::{TelemetryEvent, event_from_field_change};

/// Derive the charter name used in the workspace graph from a discovered charter.
///
/// Explicit charters (from .md files) may have a human-readable title like
/// "Build the ClearHead Platform", but the workspace graph is built from .actions
/// files where the charter name is inferred from the directory or file stem
/// (e.g., "build_clearhead"). This function returns the inferred name so that
/// SPARQL queries and in-memory filters match what's actually in the graph/model.
fn charter_graph_name(discovered: &clearhead_core::DiscoveredCharter) -> String {
    if !discovered.is_explicit {
        return discovered.charter.title.clone();
    }
    let path = std::path::Path::new(&discovered.source_key);
    if path.file_name().map(|n| n == "README.md").unwrap_or(false) {
        // "build_clearhead/README.md" → "build_clearhead"
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or(&discovered.source_key)
            .to_string()
    } else {
        // "health.md" → "health"
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&discovered.source_key)
            .to_string()
    }
}

/// Collect the charter with the given title plus all its descendants (transitively).
fn collect_charter_tree(
    charters: &[clearhead_core::Charter],
    root_title: &str,
) -> Vec<clearhead_core::Charter> {
    let mut result = Vec::new();
    let mut queue = vec![root_title.to_string()];
    while let Some(current) = queue.pop() {
        for charter in charters {
            let title_lower = charter.title.to_lowercase();
            if title_lower == current {
                result.push(charter.clone());
            } else if charter.parent.as_deref().map(|p| p.to_lowercase()).as_deref()
                == Some(&current)
            {
                result.push(charter.clone());
                queue.push(title_lower);
            }
        }
    }
    result
}

pub fn read_plans(
    ctx: &CommandContext,
    format: &Option<argparser::Format>,
    charter: &Option<String>,
    recursive: bool,
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
        let action_list = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;
        if output_format == clearhead_cli::OutputFormat::Table {
            use clearhead_core::workspace::store::infer_project_name;
            use std::path::Path;
            let relative = resolved.strip_prefix(&ctx.data_dir).unwrap_or(&resolved);
            let charter_name = infer_project_name(Path::new(relative.to_string_lossy().as_ref()));
            let model = clearhead_core::workspace::actions::convert::from_actions_with_charter(
                &action_list,
                charter_name,
            );
            println!(
                "{}",
                clearhead_core::format_domain_as_table(&model, lib_table_opts.as_ref())?
            );
            return Ok(());
        }
        action_list
    } else if stdio {
        debug!("Reading stdin");
        let content = read_input(None)?;
        clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?
    } else if let Some(charter_query) = charter {
        use clearhead_core::{FsWorkspaceStore, WorkspaceStore};

        debug!(charter = %charter_query, recursive = recursive, "Filtering by charter");
        let fs_store = FsWorkspaceStore::new(&ctx.data_dir);
        let charters = fs_store.discover_charters().map_err(|e| e.to_string())?;

        let found = crate::commands::charter::resolve_discovered_charter(&charters, charter_query)
            .ok_or_else(|| format!("No charter found matching '{}'", charter_query))?;

        let title = &found.charter.title;
        // The workspace graph is built from .actions files using inferred names
        // (e.g., "build_clearhead"), not the human-readable title from .md files
        // (e.g., "Build the ClearHead Platform"). Use the graph name for filtering.
        let graph_name = charter_graph_name(&found);
        debug!(charter_title = %title, graph_name = %graph_name, "Resolved charter");

        // Table: filter the full workspace DomainModel in memory
        if output_format == clearhead_cli::OutputFormat::Table {
            let model = clearhead_cli::load_workspace_domain_model(&ctx.data_dir)?;
            let graph_name_lower = graph_name.to_lowercase();
            let filtered_charters = if recursive {
                collect_charter_tree(&model.charters, &graph_name_lower)
            } else {
                model
                    .charters
                    .into_iter()
                    .filter(|c| c.title.to_lowercase() == graph_name_lower)
                    .collect()
            };
            let combined_model = clearhead_core::DomainModel {
                objectives: vec![],
                charters: filtered_charters,
            };
            let plan_count = combined_model.all_plans().len();
            info!(plan_count, "Loaded domain model for table");
            if plan_count == 0 {
                let kind = if found.is_explicit { "explicit" } else { "implicit" };
                println!(
                    "Charter '{}' resolved ({}, source: {}, graph name: '{}') but contains no plans.",
                    title, kind, found.source_key, graph_name
                );
                if recursive {
                    println!("(searched recursively through sub-charters)");
                } else {
                    println!(
                        "Tip: try --recursive to include sub-charters, or \
                         `clearhead query --where \"?s a <https://clearhead.us/vocab/actions/v4#Charter> ; \
                         <http://schema.org/name> ?name\"` to see what the graph sees."
                    );
                }
                return Ok(());
            }
            println!(
                "{}",
                clearhead_core::format_domain_as_table(&combined_model, lib_table_opts.as_ref())?
            );
            return Ok(());
        }

        // Non-table: SPARQL — strict (direct has_part) or recursive (transitive hasSubCharter+).
        // The recursive case uses two self-contained UNION branches instead of BIND+UNION
        // because BIND inside a sub-group with property paths behaves inconsistently in
        // some SPARQL engines (Oxigraph included).
        let query = if recursive {
            format!(
                "SELECT ?id WHERE {{ \
                    {{ \
                        ?charter a <{actions}Charter> . \
                        ?charter <{schema}name> \"{graph_name}\" . \
                        ?charter <{bfo}BFO_0000051> ?plan . \
                        ?plan <{actions}id> ?id \
                    }} UNION {{ \
                        ?root a <{actions}Charter> . \
                        ?root <{schema}name> \"{graph_name}\" . \
                        ?root <{actions}hasSubCharter>+ ?charter . \
                        ?charter <{bfo}BFO_0000051> ?plan . \
                        ?plan <{actions}id> ?id \
                    }} \
                }}",
                actions = "https://clearhead.us/vocab/actions/v4#",
                schema = "http://schema.org/",
                bfo = "http://purl.obolibrary.org/obo/",
                graph_name = graph_name.replace('"', "\\\""),
            )
        } else {
            format!(
                "SELECT ?id WHERE {{ \
                    ?charter a <{actions}Charter> . \
                    ?charter <{schema}name> \"{graph_name}\" . \
                    ?charter <{bfo}BFO_0000051> ?plan . \
                    ?plan <{actions}id> ?id \
                }}",
                actions = "https://clearhead.us/vocab/actions/v4#",
                schema = "http://schema.org/",
                bfo = "http://purl.obolibrary.org/obo/",
                graph_name = graph_name.replace('"', "\\\""),
            )
        };
        debug!(sparql = %query, recursive = recursive, "Querying plans by charter via SPARQL");
        clearhead_cli::run_workspace_sql_query(&ctx.data_dir, &query)?
    } else {
        debug!(data_dir = %ctx.data_dir.display(), "Reading workspace");

        if output_format == clearhead_cli::OutputFormat::Table {
            let model = clearhead_cli::load_workspace_domain_model(&ctx.data_dir)?;
            info!(plan_count = model.all_plans().len(), "Loaded workspace domain model for table");
            println!(
                "{}",
                clearhead_core::format_domain_as_table(&model, lib_table_opts.as_ref())?
            );
            return Ok(());
        } else {
            clearhead_cli::load_workspace_actions(&ctx.data_dir)?
        }
    };

    info!(action_count = actions.len(), "Loaded actions");
    if actions.is_empty() {
        if let Some(charter_query) = charter {
            println!(
                "No plans found for charter '{}'. \
                 Use `--format table` for a richer diagnostic, or \
                 `clearhead query --where \"?s a <https://clearhead.us/vocab/actions/v4#Charter> ; \
                 <http://schema.org/name> ?name\"` to inspect the graph.",
                charter_query
            );
            return Ok(());
        }
    }
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
    debug!(query = %query, "Executing Show Plan");

    let actions = if let Some(path) = file {
        let input_file = ctx.resolve_action_file(Some(path));
        debug!(input_file = %input_file.display(), "Searching file");
        load_file(&input_file)?
    } else {
        debug!(data_dir = %ctx.data_dir.display(), "Searching workspace");
        clearhead_cli::load_workspace_actions(&ctx.data_dir)?
    };

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
    charter: &Option<String>,
    fields: &argparser::ActionFields,
    dry_run: bool,
) -> Result<(), String> {
    use chrono::Local;
    use clearhead_cli::{Action, ActionState};
    use uuid::Uuid;

    let input_file = if let Some(charter_query) = charter {
        crate::commands::charter_to_file_path(&ctx.data_dir, charter_query)?
    } else {
        ctx.resolve_action_file(file.as_ref())
    };
    debug!(name = %name, input_file = %input_file.display(), dry_run = dry_run, "Executing Add Plan");

    if !dry_run && !input_file.exists() {
        info!(input_file = %input_file.display(), "Creating new actions file");
        if let Some(parent) = input_file.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
        }
        fs::write(&input_file, "").map_err(|e| format!("Failed to create file: {}", e))?;
    }

    let mut actions = load_file(&input_file)?;

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
        charter: None,
        alias: fields.alias.clone(),
        is_sequential: None,
    };

    actions.push(new_action.clone());

    if dry_run {
        let preview =
            clearhead_cli::format(&vec![new_action], clearhead_cli::OutputFormat::Actions, None, None)?;
        println!("{}", preview);
    } else {
        save_file(&input_file, &actions)?;

        try_emit(
            &new_id,
            TelemetryEvent::ActionCreated {
                name: name.to_string(),
                file_path: input_file.display().to_string(),
            },
        );

        info!(name = %name, id = %new_id, "Action added successfully");
        println!("{}", new_id);
    }
    Ok(())
}

pub fn update_plan(
    ctx: &CommandContext,
    query: &str,
    file: &Option<std::path::PathBuf>,
    name: &Option<String>,
    fields: &argparser::ActionFields,
    dry_run: bool,
) -> Result<(), String> {
    use clearhead_cli::{ActionUpdate, apply_updates, resolve_reference};

    let (input_file, mut actions) = if let Some(path) = file {
        let f = ctx.resolve_action_file(Some(path));
        let a = load_file(&f)?;
        (f, a)
    } else {
        crate::commands::find_plan_file(&ctx.data_dir, query)?
    };
    debug!(query = %query, input_file = %input_file.display(), dry_run = dry_run, "Executing Update Plan");

    let resolved = resolve_reference(&actions, query)
        .ok_or_else(|| format!("No action found matching '{}'", query))?;

    debug!(index = resolved.index, match_type = ?resolved.match_type, "Resolved action reference");

    let mut updates: ActionUpdate = fields.clone().into();
    updates.name = name.clone();

    let old_action = actions[resolved.index].clone();
    let action_id = old_action.id;

    apply_updates(&mut actions[resolved.index], updates);

    let new_action = actions[resolved.index].clone();

    if dry_run {
        let preview = clearhead_cli::format(
            &vec![new_action],
            clearhead_cli::OutputFormat::Actions,
            None,
            None,
        )?;
        println!("{}", preview);
    } else {
        save_file(&input_file, &actions)?;

        use clearhead_core::diff_actions;
        let changes = diff_actions(&vec![old_action], &vec![new_action.clone()]);
        if let Some(action_diff) = changes.modified.first() {
            for change in &action_diff.changes {
                if let Some(evt) = event_from_field_change(change) {
                    try_emit(&action_id, evt);
                }
            }
        }

        info!(name = %new_action.name, id = %action_id, "Action updated successfully");
    }
    Ok(())
}

pub fn complete_plan(
    ctx: &CommandContext,
    query: &str,
    file: &Option<std::path::PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (input_file, mut actions) = if let Some(path) = file {
        let f = ctx.resolve_action_file(Some(path));
        let a = load_file(&f)?;
        (f, a)
    } else {
        crate::commands::find_plan_file(&ctx.data_dir, query)?
    };

    let result = crate::commands::complete::complete_action(&mut actions, query)?;

    if dry_run {
        let completed = &actions[result.action_index];
        let preview = clearhead_cli::format(
            &vec![completed.clone()],
            clearhead_cli::OutputFormat::Actions,
            None,
            None,
        )?;
        println!("{}", preview);
    } else {
        save_file(&input_file, &actions)?;
        try_emit(&result.action_id, result.event);
        info!(name = %result.action_name, id = %result.action_id, recurring = result.is_recurring, "Action completed successfully");
    }
    Ok(())
}

pub fn delete_plan(
    ctx: &CommandContext,
    query: &str,
    file: &Option<std::path::PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (input_file, mut actions) = if let Some(path) = file {
        let f = ctx.resolve_action_file(Some(path));
        let a = load_file(&f)?;
        (f, a)
    } else {
        crate::commands::find_plan_file(&ctx.data_dir, query)?
    };
    debug!(query = %query, input_file = %input_file.display(), dry_run = dry_run, "Executing Delete Plan");

    let resolved = clearhead_cli::resolve_reference(&actions, query)
        .ok_or_else(|| format!("No plan found matching '{}'", query))?;

    let action = actions.remove(resolved.index);

    if dry_run {
        let preview = clearhead_cli::format(
            &vec![action],
            clearhead_cli::OutputFormat::Actions,
            None,
            None,
        )?;
        println!("{}", preview);
    } else {
        save_file(&input_file, &actions)?;

        try_emit(
            &action.id,
            TelemetryEvent::ActionDeleted {
                name: action.name.clone(),
            },
        );

        info!(name = %action.name, id = %action.id, "Action deleted successfully");
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
    let actions = clearhead_cli::parse_actions(&content)?;
    let model = clearhead_core::workspace::actions::convert::from_actions(&actions);
    let icalendar = clearhead_cli::format_as_icalendar(&model, open_only)?;

    if let Some(output_path) = output {
        info!(output_path = %output_path.display(), "Writing iCalendar export to file");
        fs::write(output_path, icalendar).map_err(|e| format!("Failed to write to file: {}", e))?;
    } else {
        println!("{}", icalendar);
    }
    Ok(())
}
