use std::fs;
use tracing::{debug, info};

use crate::argparser;
use crate::commands::{load_file, parse_format, read_input, save_file, try_emit, CommandContext};
use clearhead_cli::telemetry::{event_from_field_change, TelemetryEvent};

/// The canonical machine key for a charter — alias if present, otherwise title.
///
/// `charter.parent` always stores a machine key, so this is the right value to
/// use for any identity comparison or graph edge.
fn charter_key(charter: &clearhead_core::Charter) -> &str {
    charter.alias.as_deref().unwrap_or(&charter.title)
}

/// Returns the key name used in the workspace graph for a charter (owned).
fn charter_graph_name(charter: &clearhead_core::Charter) -> String {
    charter_key(charter).to_string()
}

/// All charters whose `parent` field matches `parent_key` (case-insensitive).
fn direct_children<'a>(
    charters: &'a [clearhead_core::Charter],
    parent_key: &str,
) -> Vec<&'a clearhead_core::Charter> {
    charters
        .iter()
        .filter(|c| {
            c.parent
                .as_deref()
                .map(|p| p.eq_ignore_ascii_case(parent_key))
                .unwrap_or(false)
        })
        .collect()
}

/// Collect the charter matching `root_key` plus all descendants (transitively).
///
/// Uses machine keys throughout — `charter.parent` always stores a machine key,
/// never a display title. A `visited` set guards against cyclic parent data.
fn collect_charter_tree(
    charters: &[clearhead_core::Charter],
    root_key: &str,
) -> Vec<clearhead_core::Charter> {
    let mut result = Vec::new();
    let mut queue = vec![root_key.to_string()];
    let mut visited = std::collections::HashSet::new();

    while let Some(current) = queue.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        if let Some(node) = charters
            .iter()
            .find(|c| charter_key(c).eq_ignore_ascii_case(&current))
        {
            result.push(node.clone());
        }
        for child in direct_children(charters, &current) {
            queue.push(charter_key(child).to_string());
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
            let relative = resolved.strip_prefix(&ctx.data_dir).unwrap_or(&resolved);
            let charter_name = clearhead_core::infer_charter_name(relative)
                .unwrap_or_else(|| "unknown".to_string());
            let charter = clearhead_core::workspace::actions::convert::from_actions_with_charter(
                &action_list,
                charter_name,
            );
            let model = clearhead_core::DomainModel {
                objectives: vec![],
                charters: vec![charter],
            };
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
        debug!(charter = %charter_query, recursive = recursive, "Filtering by charter");
        let model = clearhead_core::load_domain_model(&ctx.data_dir).map_err(|e| e.to_string())?;

        let (title, graph_name) = {
            let found = crate::commands::charter::resolve_charter(&model.charters, charter_query)
                .ok_or_else(|| format!("No charter found matching '{}'", charter_query))?;
            (found.title.clone(), charter_graph_name(found))
        };
        debug!(charter_title = %title, graph_name = %graph_name, "Resolved charter");

        // Table: filter the already-loaded DomainModel in memory
        if output_format == clearhead_cli::OutputFormat::Table {
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
                println!(
                    "Charter '{}' (graph name: '{}') resolved but contains no plans.",
                    title, graph_name
                );
                if recursive {
                    println!("(searched recursively through sub-charters)");
                } else {
                    println!(
                        "Tip: try --recursive to include sub-charters, or \
                         `clearhead query --where \"?s a <https://clearhead.us/vocab/actions/v4#Charter> ; \
                         <http://www.w3.org/2000/01/rdf-schema#label> ?name\"` to see what the graph sees."
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
                        ?charter <{rdfs}label> \"{graph_name}\" . \
                        ?charter <{bfo}BFO_0000051> ?plan . \
                        ?plan <{actions}hasUUID> ?id \
                    }} UNION {{ \
                        ?root a <{actions}Charter> . \
                        ?root <{rdfs}label> \"{graph_name}\" . \
                        ?root <{actions}hasSubCharter>+ ?charter . \
                        ?charter <{bfo}BFO_0000051> ?plan . \
                        ?plan <{actions}hasUUID> ?id \
                    }} \
                }}",
                actions = "https://clearhead.us/vocab/actions/v4#",
                rdfs = "http://www.w3.org/2000/01/rdf-schema#",
                bfo = "http://purl.obolibrary.org/obo/",
                graph_name = graph_name.replace('"', "\\\""),
            )
        } else {
            format!(
                "SELECT ?id WHERE {{ \
                    ?charter a <{actions}Charter> . \
                    ?charter <{rdfs}label> \"{graph_name}\" . \
                    ?charter <{bfo}BFO_0000051> ?plan . \
                    ?plan <{actions}hasUUID> ?id \
                }}",
                actions = "https://clearhead.us/vocab/actions/v4#",
                rdfs = "http://www.w3.org/2000/01/rdf-schema#",
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
            info!(
                plan_count = model.all_plans().len(),
                "Loaded workspace domain model for table"
            );
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
                 <http://www.w3.org/2000/01/rdf-schema#label> ?name\"` to inspect the graph.",
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
        due_date_time: None,
        due_recurrence: None,
        completed_date_time: None,
        created_date_time: Some(Local::now()),
        predecessors: None,
        charter: None,
        alias: fields.alias.clone(),
        is_sequential: None,
    };

    actions.push(new_action.clone());

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
    use clearhead_cli::{apply_updates, resolve_reference, ActionUpdate};

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
    scope: &Option<String>,
    file: &Option<std::path::PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    use std::path::PathBuf;

    let charter_files: Vec<PathBuf> = if let Some(f) = file {
        vec![f.clone()]
    } else if let Some(s) = scope {
        use crate::commands::resolver::{resolve_domain_ref, ResolvedScope};
        // TODO: filter to matching plan tree only when scope is Plan variant
        match resolve_domain_ref(&ctx.data_dir, s)? {
            ResolvedScope::Charter { file_path } | ResolvedScope::Plan { file_path, .. } => {
                vec![file_path]
            }
        }
    } else {
        clearhead_core::list_action_files(&ctx.data_dir)
            .map_err(|e| format!("Failed to list workspace: {}", e))?
    };

    debug!(dry_run = dry_run, "Executing Archive Plans");

    let mut total_archived = 0usize;
    let mut charters_touched = 0usize;

    for source in &charter_files {
        if !source.exists() {
            continue;
        }

        let content = fs::read_to_string(source)
            .map_err(|e| format!("Failed to read '{}': {}", source.display(), e))?;

        let all_actions = clearhead_cli::parse_actions(&content)?;
        let (active_actions, archived_actions) =
            clearhead_cli::archive::partition_actions_for_archive(&all_actions);

        if archived_actions.is_empty() {
            continue;
        }

        let dest = completed_archive_path(source);

        if dry_run {
            println!(
                "Would archive {} action(s) from {} to {}",
                archived_actions.len(),
                source.display(),
                dest.display()
            );
            for action in &archived_actions {
                if action.parent_id.is_none() {
                    println!("  - {} (tree)", action.name);
                }
            }
        } else {
            // Append archived actions to destination
            let mut dest_actions = load_file(&dest)?;
            dest_actions.extend(archived_actions.iter().cloned());
            save_file(&dest, &dest_actions)?;

            // Write active actions back to source
            save_file(source, &active_actions)?;

            info!(
                count = archived_actions.len(),
                source = %source.display(),
                dest = %dest.display(),
                "Plans archived"
            );
            println!(
                "Archived {} plan(s) to {}",
                archived_actions.len(),
                dest.display()
            );
        }

        total_archived += archived_actions.len();
        charters_touched += 1;
    }

    if total_archived == 0 {
        println!("Nothing to archive.");
    } else if dry_run && charters_touched > 1 {
        println!(
            "Would archive {} plan(s) across {} charter(s).",
            total_archived, charters_touched
        );
    } else if !dry_run && charters_touched > 1 {
        println!(
            "Archived {} plan(s) across {} charter(s).",
            total_archived, charters_touched
        );
    }

    Ok(())
}

fn completed_archive_path(source: &std::path::Path) -> std::path::PathBuf {
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    source
        .parent()
        .unwrap_or(std::path::Path::new(""))
        .join(format!("{}.completed.actions", stem))
}

pub fn export_plans(
    ctx: &CommandContext,
    reference: &Option<String>,
    output: &Option<std::path::PathBuf>,
    open_only: bool,
    recursive: bool,
) -> Result<(), String> {
    use crate::environment_reader::resolve_file_path;
    use clearhead_core::reference::{
        filter_model_for_act, filter_model_for_charter, filter_model_for_plan, resolve_reference,
        ReferenceOptions, ReferenceTarget,
    };

    debug!(reference = ?reference, output = ?output, open_only = open_only, recursive = recursive, "Executing Export Plans");

    let model = if let Some(reference) = reference {
        if reference == "-" {
            let content = read_input(None)?;
            let actions = clearhead_cli::parse_actions(&content)?;
            let charter = clearhead_core::workspace::actions::convert::from_actions_with_charter(
                &actions,
                "stdin".to_string(),
            );
            clearhead_core::DomainModel {
                objectives: vec![],
                charters: vec![charter],
            }
        } else if reference.ends_with(".actions") {
            let resolved = resolve_file_path(reference, &ctx.data_dir);
            let content = read_input(Some(&resolved))?;
            let actions = clearhead_cli::parse_actions(&content)?;
            let relative = resolved.strip_prefix(&ctx.data_dir).unwrap_or(&resolved);
            let charter_name = clearhead_core::infer_charter_name(relative)
                .unwrap_or_else(|| "unknown".to_string());
            let charter = clearhead_core::workspace::actions::convert::from_actions_with_charter(
                &actions,
                charter_name,
            );
            let mut model = clearhead_core::DomainModel {
                objectives: vec![],
                charters: vec![charter],
            };

            let open_path = clearhead_core::open_acts_path(&resolved);
            let closed_path = clearhead_core::closed_acts_path(&resolved);
            let mut loaded_acts = clearhead_core::read_acts(&open_path)?;
            let closed_acts = clearhead_core::read_acts(&closed_path)?;
            loaded_acts.extend(closed_acts);
            if !loaded_acts.is_empty() {
                clearhead_core::merge_acts_into_model(&mut model, loaded_acts);
            }
            model
        } else {
            let model = clearhead_cli::load_workspace_domain_model(&ctx.data_dir)?;
            let target = resolve_reference(&model, reference, &ReferenceOptions::default())
                .map_err(|e| e.to_string())?;
            match target {
                ReferenceTarget::Charter(id) => filter_model_for_charter(&model, id, recursive),
                ReferenceTarget::Plan(id) => filter_model_for_plan(&model, id),
                ReferenceTarget::Act(id) => filter_model_for_act(&model, id),
            }
        }
    } else {
        clearhead_cli::load_workspace_domain_model(&ctx.data_dir)?
    };

    let icalendar = clearhead_cli::format_as_icalendar(&model, open_only)?;

    if let Some(output_path) = output {
        info!(output_path = %output_path.display(), "Writing iCalendar export to file");
        fs::write(output_path, icalendar).map_err(|e| format!("Failed to write to file: {}", e))?;
    } else {
        println!("{}", icalendar);
    }
    Ok(())
}
