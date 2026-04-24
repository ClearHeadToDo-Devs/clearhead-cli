use std::fs;
use tracing::{debug, info};

use crate::argparser;
use crate::commands::{
    CommandContext, find_plan_file_for_mutation, load_file, load_file_for_mutation,
    load_existing_file_for_read, load_file_for_read, parse_content_for_read, parse_format,
    read_input, save_file, try_emit,
};
use clearhead_cli::telemetry::{event_from_field_change, TelemetryEvent};

/// Find the index at which to insert a child action so it appears immediately
/// after the last existing descendant of `parent_id`.
///
/// Walks forward from the parent's position, collecting all actions whose
/// ancestor chain leads back to `parent_id`. Returns the index after the last
/// one, or `actions.len()` if the parent is not found.
fn insert_index_after_descendants(actions: &[clearhead_cli::Action], parent_id: uuid::Uuid) -> usize {
    let parent_idx = match actions.iter().position(|a| a.id == parent_id) {
        Some(idx) => idx,
        None => return actions.len(),
    };

    let mut descendant_ids: std::collections::HashSet<uuid::Uuid> =
        std::collections::HashSet::from([parent_id]);
    let mut last = parent_idx;

    for (offset, action) in actions[parent_idx + 1..].iter().enumerate() {
        if action.parent_id.map_or(false, |pid| descendant_ids.contains(&pid)) {
            descendant_ids.insert(action.id);
            last = parent_idx + 1 + offset;
        }
    }

    last + 1
}

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
        let action_list = load_existing_file_for_read(&resolved, "read plans")?;
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
        parse_content_for_read(&content, "stdin", "read plans")?
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
                    .filter(|c| charter_key(c).to_lowercase() == graph_name_lower)
                    .collect()
            };
            let combined_model = clearhead_core::DomainModel {
                objectives: vec![],
                charters: filtered_charters,
            };
            let plan_count = combined_model.all_plans().len();
            info!(plan_count, "Loaded domain model for table");
            if plan_count == 0 {
                if recursive {
                    println!("{}: no plans (searched sub-charters)", title);
                } else {
                    println!("{}: no plans", title);
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
                        ?charter <{rdfs}label> \"{title}\" . \
                        ?charter <{bfo}BFO_0000051> ?plan . \
                        ?plan <{actions}hasUUID> ?id \
                    }} UNION {{ \
                        ?root a <{actions}Charter> . \
                        ?root <{rdfs}label> \"{title}\" . \
                        ?root <{actions}hasSubCharter>+ ?charter . \
                        ?charter <{bfo}BFO_0000051> ?plan . \
                        ?plan <{actions}hasUUID> ?id \
                    }} \
                }}",
                actions = "https://clearhead.us/vocab/actions/v4#",
                rdfs = "http://www.w3.org/2000/01/rdf-schema#",
                bfo = "http://purl.obolibrary.org/obo/",
                title = title.replace('"', "\\\""),
            )
        } else {
            format!(
                "SELECT ?id WHERE {{ \
                    ?charter a <{actions}Charter> . \
                    ?charter <{rdfs}label> \"{title}\" . \
                    ?charter <{bfo}BFO_0000051> ?plan . \
                    ?plan <{actions}hasUUID> ?id \
                }}",
                actions = "https://clearhead.us/vocab/actions/v4#",
                rdfs = "http://www.w3.org/2000/01/rdf-schema#",
                bfo = "http://purl.obolibrary.org/obo/",
                title = title.replace('"', "\\\""),
            )
        };
        debug!(sparql = %query, recursive = recursive, "Querying plans by charter via SPARQL");
        clearhead_cli::run_workspace_sql_query(&ctx.data_dir, &query)?
    } else {
        debug!(data_dir = %ctx.data_dir.display(), "Reading workspace");

        if output_format == clearhead_cli::OutputFormat::Table {
            let model = clearhead_cli::load_workspace_domain_model(&ctx.data_dir)?;
            let plan_count = model.all_plans().len();
            info!(plan_count, "Loaded workspace domain model for table");
            if plan_count == 0 {
                println!("workspace: no plans");
                return Ok(());
            }
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
    parent: &Option<String>,
    fields: &argparser::ActionFields,
    dry_run: bool,
) -> Result<(), String> {
    use clearhead_cli::Action;
    use uuid::Uuid;

    let input_file = if let Some(charter_query) = charter {
        crate::commands::charter_to_file_path(&ctx.data_dir, charter_query)?
    } else {
        ctx.resolve_action_file(file.as_ref())
    };
    debug!(name = %name, input_file = %input_file.display(), dry_run = dry_run, "Executing Add Plan");

    if !dry_run && !input_file.exists() {
        info!(input_file = %input_file.display(), "Creating new actions file");
        if let Some(parent_dir) = input_file.parent() {
            fs::create_dir_all(parent_dir).map_err(|e| format!("Failed to create directory: {}", e))?;
        }
        fs::write(&input_file, "").map_err(|e| format!("Failed to create file: {}", e))?;
    }

    let mut actions = load_file_for_mutation(&input_file, "add plan")?;

    // Resolve parent reference if provided
    let parent_id: Option<Uuid> = if let Some(parent_ref) = parent {
        if parent_ref.contains('/') {
            // Cross-charter: charter/plan path
            use crate::commands::resolver::{resolve_domain_ref, ResolvedScope};
            let resolved_file = match resolve_domain_ref(&ctx.data_dir, parent_ref)? {
                ResolvedScope::Plan { file_path, plan_id } => {
                    // If --charter was also given, the resolved file must agree
                    if let Some(charter_query) = charter {
                        let charter_file = crate::commands::charter_to_file_path(&ctx.data_dir, charter_query)?;
                        if file_path != charter_file {
                            return Err(format!(
                                "--parent '{}' resolves to a different charter than --charter '{}'",
                                parent_ref, charter_query
                            ));
                        }
                    }
                    Some(plan_id)
                }
                ResolvedScope::Charter { .. } => {
                    return Err(format!("'{}' resolves to a charter, not a plan", parent_ref));
                }
            };
            resolved_file
        } else {
            // Same-file: alias, name, or short UUID
            let resolved = clearhead_cli::resolve_reference(&actions, parent_ref)
                .ok_or_else(|| format!("No plan found matching '{}'", parent_ref))?;
            Some(actions[resolved.index].id)
        }
    } else {
        None
    };

    let new_id = Uuid::now_v7();
    let new_action = Action {
        id: new_id,
        parent_id,
        state: fields.state.map(|s| s.into()).unwrap_or_default(),
        name: name.to_string(),
        description: fields.description.clone(),
        priority: fields.priority,
        context_list: (!fields.context.is_empty()).then(|| fields.context.clone()),
        alias: fields.alias.clone(),
        ..Default::default()
    };

    let insert_idx = parent_id
        .map(|pid| insert_index_after_descendants(&actions, pid))
        .unwrap_or(actions.len());
    actions.insert(insert_idx, new_action.clone());

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
        let a = load_file_for_mutation(&f, "update plan")?;
        (f, a)
    } else {
        find_plan_file_for_mutation(&ctx.data_dir, query, "update plan")?
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
        let a = load_file_for_mutation(&f, "complete plan")?;
        (f, a)
    } else {
        find_plan_file_for_mutation(&ctx.data_dir, query, "complete plan")?
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
        info!(name = %result.action_name, id = %result.action_id, "Action completed successfully");
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
        let a = load_file_for_mutation(&f, "delete plan")?;
        (f, a)
    } else {
        find_plan_file_for_mutation(&ctx.data_dir, query, "delete plan")?
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

        let all_actions = load_file_for_mutation(source, "archive plans")?;
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
            let mut dest_actions = load_file_for_mutation(&dest, "archive plans")?;
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
            let actions = parse_content_for_read(&content, "stdin", "export plans")?;
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
            let actions = load_file_for_read(&resolved, "export plans")?;
            let relative = resolved.strip_prefix(&ctx.data_dir).unwrap_or(&resolved);
            let charter_name = clearhead_core::infer_charter_name(relative)
                .unwrap_or_else(|| "unknown".to_string());
            let charter = clearhead_core::workspace::actions::convert::from_actions_with_charter(
                &actions,
                charter_name,
            );
            let model = clearhead_core::DomainModel {
                objectives: vec![],
                charters: vec![charter],
            };

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

#[cfg(test)]
mod tests {
    use super::*;
    use clearhead_cli::Action;
    use uuid::Uuid;

    fn action(id: Uuid, parent_id: Option<Uuid>) -> Action {
        Action { id, parent_id, name: id.to_string(), ..Default::default() }
    }

    #[test]
    fn insert_after_last_descendant_with_no_children() {
        let parent = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        // [parent, sibling] — sibling is not a descendant of parent
        let actions = vec![action(parent, None), action(sibling, None)];
        // child should go at index 1 (immediately after parent, before sibling)
        assert_eq!(insert_index_after_descendants(&actions, parent), 1);
    }

    #[test]
    fn insert_after_last_descendant_skips_existing_children() {
        let parent = Uuid::new_v4();
        let child = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        // [parent, child, sibling]
        let actions = vec![
            action(parent, None),
            action(child, Some(parent)),
            action(sibling, None),
        ];
        // new child should go at index 2 (after existing child, before sibling)
        assert_eq!(insert_index_after_descendants(&actions, parent), 2);
    }

    #[test]
    fn insert_after_last_descendant_handles_grandchildren() {
        let parent = Uuid::new_v4();
        let child = Uuid::new_v4();
        let grandchild = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        // [parent, child, grandchild, sibling]
        let actions = vec![
            action(parent, None),
            action(child, Some(parent)),
            action(grandchild, Some(child)),
            action(sibling, None),
        ];
        // new child should go at index 3 (after grandchild, before sibling)
        assert_eq!(insert_index_after_descendants(&actions, parent), 3);
    }

    #[test]
    fn insert_after_last_descendant_unknown_parent_appends() {
        let unknown = Uuid::new_v4();
        let actions = vec![action(Uuid::new_v4(), None)];
        assert_eq!(insert_index_after_descendants(&actions, unknown), actions.len());
    }
}
