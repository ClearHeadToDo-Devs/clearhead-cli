//! Handlers for action commands (expand, complete, cancel, update, read, archive).

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use chrono::Local;
use tracing::{info, warn};

use clearhead_core::workspace::{action_files, read_actions, templates};
use clearhead_core::{Action, ActionList, ActionState};

use super::CommandContext;

// ============================================================================
// expand actions — ICS schedule → .actions file
// ============================================================================

/// Add a new standalone action to a charter's `.actions` file.
pub fn add_action(
    ctx: &CommandContext,
    name: &str,
    charter: &Option<String>,
    file: &Option<PathBuf>,
    parent: &Option<String>,
    priority: Option<u32>,
    state: Option<crate::argparser::ActionStateArg>,
    alias: &Option<String>,
    description: &Option<String>,
    scheduled_at: &Option<String>,
    duration: Option<u32>,
    dry_run: bool,
) -> Result<(), String> {
    let actions_path = resolve_acts_file(ctx, charter, file)?;

    let parent_id = parent
        .as_deref()
        .map(|q| {
            let mut list = action_files::read_actions(&actions_path).map_err(|e| e.to_string())?;
            find_act_mut(&mut list, q)
                .map(|a| a.id)
                .ok_or_else(|| format!("No action found matching parent '{}'", q))
        })
        .transpose()?;

    let new_scheduled = scheduled_at
        .as_deref()
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Local))
                .map_err(|e| format!("Invalid --scheduled-at '{}': {}", s, e))
        })
        .transpose()?;

    let act = Action {
        name: name.to_string(),
        parent_id,
        priority,
        state: state.map(Into::into).unwrap_or(ActionState::NotStarted),
        alias: alias.clone(),
        description: description.clone(),
        scheduled_at: new_scheduled,
        duration: duration,
        created_at: Some(Local::now()),
        ..Default::default()
    };

    if dry_run {
        println!("Would add action '{}' to {}", name, actions_path.display());
        return Ok(());
    }

    let mut list = action_files::read_actions(&actions_path).map_err(|e| e.to_string())?;
    list.push(act.clone());
    super::save_file(&actions_path, &list)?;

    info!(id = %act.id, name = %name, "Action added");
    println!("Added action {} ({})", &act.id.to_string()[..8], name);
    Ok(())
}

/// Resolve the `.actions` file path from a charter query or explicit file path.
fn resolve_acts_file(
    ctx: &CommandContext,
    charter: &Option<String>,
    file: &Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(path) = file {
        return Ok(path.clone());
    }
    if let Some(query) = charter {
        let (mc, ws_root) = resolve_charter_across_workspaces(ctx, query)?;
        let rel = mc
            .acts_file
            .as_ref()
            .ok_or_else(|| format!("Charter '{}' has no associated actions file", mc.title))?;
        let root = clearhead_core::charter_root(&ws_root);
        return Ok(root.join(rel));
    }
    Err("Specify --charter <name> or --file <path> to target a charter's actions file".to_string())
}

/// Expand ICS schedule VEVENTs into actions in the charter's `.actions` and
/// `.upcoming.actions` files.
pub fn expand_actions(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    use clearhead_core::workspace::ics::parse_ics_file;
    use clearhead_core::workspace::plans::collect_plan_files;
    use clearhead_core::{ExpansionConfig, upcoming_actions_path};

    let data_root = clearhead_core::workspace_data_root(&ctx.data_dir);
    let now = Local::now();

    let expansion_config = ExpansionConfig {
        total_instances: ctx.config.expansion_total_instances,
        primary_instances: ctx.config.expansion_primary_instances,
    };

    let all_entries = collect_plan_files(&ctx.data_dir)
        .map_err(|e| format!("Failed to discover ICS files: {}", e))?;

    let entries: Vec<_> = if let Some(actions_path) = file {
        let relative = actions_path
            .strip_prefix(&data_root)
            .unwrap_or(actions_path.as_path());
        let charter_name = clearhead_core::infer_charter_name(relative).ok_or_else(|| {
            format!(
                "Cannot infer charter name from '{}'",
                actions_path.display()
            )
        })?;
        all_entries
            .into_iter()
            .filter(|e| e.charter_name == charter_name)
            .collect()
    } else {
        all_entries
    };

    if entries.is_empty() {
        println!("No ICS schedule files found.");
        return Ok(());
    }

    let mut by_charter: HashMap<String, Vec<_>> = HashMap::new();
    for entry in entries {
        by_charter
            .entry(entry.charter_name.clone())
            .or_default()
            .push(entry);
    }

    let mut total_added = 0usize;
    let mut charters_touched = 0usize;
    let mut parse_failures: Vec<PathBuf> = Vec::new();

    for (charter_name, charter_entries) in by_charter {
        let actions_path = resolve_acts_file(ctx, &Some(charter_name.clone()), &None)?;
        let upcoming_path = upcoming_actions_path(&actions_path);
        let charter_dir = actions_path.parent().unwrap_or(Path::new(""));

        let primary_list = match super::load_file_for_mutation(&actions_path, "expand actions") {
            Ok(actions) => actions,
            Err(err) => {
                warn!(path = %actions_path.display(), error = %err, "Skipping charter due to parse issues");
                parse_failures.push(actions_path.clone());
                continue;
            }
        };

        // Load existing upcoming file (empty list if file doesn't exist yet)
        let upcoming_list = if upcoming_path.exists() {
            match super::load_file_for_mutation(&upcoming_path, "expand actions (upcoming)") {
                Ok(actions) => actions,
                Err(err) => {
                    warn!(path = %upcoming_path.display(), error = %err, "Could not read upcoming file — treating as empty");
                    vec![]
                }
            }
        } else {
            vec![]
        };

        let mut all_plans = Vec::new();
        for entry in &charter_entries {
            match parse_ics_file(&entry.path) {
                Ok(plans) => all_plans.extend(plans.into_iter().map(|ip| ip.plan)),
                Err(e) => return Err(format!("Failed to parse {}: {}", entry.path.display(), e)),
            }
        }

        let expand_result = clearhead_core::expand_plans_into_acts(
            &all_plans,
            &primary_list,
            &upcoming_list,
            now,
            &expansion_config,
        );

        if expand_result.primary.is_empty() && expand_result.upcoming.is_empty() {
            continue;
        }

        // Resolve template expansions: when a plan has a template, the template
        // replaces the flat root action rather than being appended as its children.
        let primary_expanded = resolve_expanded_acts(
            expand_result.primary,
            &all_plans,
            charter_dir,
            &data_root,
        );
        let upcoming_expanded = resolve_expanded_acts(
            expand_result.upcoming,
            &all_plans,
            charter_dir,
            &data_root,
        );

        let new_primary_count = primary_expanded.len();
        let new_upcoming_count = upcoming_expanded.len();
        let new_count = new_primary_count + new_upcoming_count;

        if dry_run {
            if new_primary_count > 0 {
                println!("Would add {} action(s) to {}", new_primary_count, actions_path.display());
            }
            if new_upcoming_count > 0 {
                println!("Would add {} action(s) to {}", new_upcoming_count, upcoming_path.display());
            }
        } else {
            if new_primary_count > 0 {
                let mut updated_primary = primary_list;
                updated_primary.extend(primary_expanded);
                super::save_file(&actions_path, &updated_primary)?;
                info!(count = new_primary_count, path = %actions_path.display(), "Actions expanded (primary)");
                println!("Added {} action(s) to {}", new_primary_count, actions_path.display());
            }
            if new_upcoming_count > 0 {
                let mut updated_upcoming = upcoming_list;
                updated_upcoming.extend(upcoming_expanded);
                super::save_file(&upcoming_path, &updated_upcoming)?;
                info!(count = new_upcoming_count, path = %upcoming_path.display(), "Actions expanded (upcoming)");
                println!("Added {} action(s) to {}", new_upcoming_count, upcoming_path.display());
            }
            if new_count > 0 {
                total_added += new_count;
                charters_touched += 1;
            }
        }
    }

    if total_added == 0 && !dry_run {
        println!("Nothing to expand.");
    } else if charters_touched > 1 {
        println!("Expanded {} action(s) across {} charter(s).", total_added, charters_touched);
    }

    if !parse_failures.is_empty() {
        eprintln!(
            "expand actions failed for {} charter file(s) due to parse errors",
            parse_failures.len()
        );
        for path in &parse_failures {
            eprintln!("  - {}", path.display());
        }
        return Err(format!(
            "expand actions skipped {} file(s) due to parse errors",
            parse_failures.len()
        ));
    }

    Ok(())
}

// ============================================================================
// Action lifecycle — complete, cancel, update
// ============================================================================

/// Mark an open action as completed (moves to `.completed.actions`).
pub fn complete_action(
    ctx: &CommandContext,
    query: &str,
    charter: &Option<String>,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, charter, query)?;

    let act_id = find_act_mut(&mut open_acts, query)
        .ok_or_else(|| format!("No open action found matching '{}'", query))?
        .id;

    let subtree_ids = collect_subtree_ids(&open_acts, act_id);

    if dry_run {
        println!(
            "Would complete action {} and {} child(ren)",
            &act_id.to_string()[..8],
            subtree_ids.len() - 1,
        );
        return Ok(());
    }

    let now = Local::now();
    let mut to_close: Vec<Action> = open_acts
        .iter()
        .filter(|a| subtree_ids.contains(&a.id))
        .map(|a| {
            let mut closed = a.clone();
            closed.state = ActionState::Completed;
            closed.completed_at = Some(now);
            closed.parent_id = None;
            closed
        })
        .collect();

    open_acts.retain(|a| !subtree_ids.contains(&a.id));
    super::save_file(&actions_path, &open_acts)?;

    let completed_path = action_files::completed_actions_path(&actions_path);
    let mut closed = action_files::read_actions(&completed_path).map_err(|e| e.to_string())?;
    closed.append(&mut to_close);
    action_files::write_actions(&closed, &completed_path).map_err(|e| e.to_string())?;

    info!(%act_id, children = subtree_ids.len() - 1, "Action subtree completed");
    println!("Completed action {} (+{} children)", act_id, subtree_ids.len() - 1);
    Ok(())
}

/// Update an open action's fields.
pub fn update_action(
    ctx: &CommandContext,
    query: &str,
    name: &Option<String>,
    priority: Option<u32>,
    state: Option<crate::argparser::ActionStateArg>,
    scheduled_at: &Option<String>,
    duration: &Option<u32>,
    description: &Option<String>,
    charter: &Option<String>,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, charter, query)?;

    let new_scheduled = scheduled_at
        .as_deref()
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Local))
                .map_err(|e| format!("Invalid --scheduled-at '{}': {}", s, e))
        })
        .transpose()?;

    let act_id = {
        let act = find_act_mut(&mut open_acts, query)
            .ok_or_else(|| format!("No open action found matching '{}'", query))?;
        let id = act.id;
        if !dry_run {
            if let Some(n) = name {
                act.name = n.clone();
            }
            if let Some(p) = priority {
                act.priority = Some(p);
            }
            if let Some(s) = state {
                act.state = s.into();
            }
            if let Some(dt) = new_scheduled {
                act.scheduled_at = Some(dt);
            }
            if let Some(dur) = duration {
                act.duration = Some(*dur);
            }
            if let Some(d) = description {
                act.description = Some(d.clone());
            }
        }
        id
    };

    if dry_run {
        println!("Would update action {}", &act_id.to_string()[..8]);
        return Ok(());
    }

    super::save_file(&actions_path, &open_acts)?;
    info!(%act_id, "Action updated");
    println!("Updated action {}", act_id);
    Ok(())
}

/// Delete an action from the workspace (open or closed).
pub fn delete_action(
    ctx: &CommandContext,
    query: &str,
    charter: &Option<String>,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    // Try open actions first, then completed.
    let action_files: Vec<PathBuf> = if let Some(path) = file {
        vec![path.clone()]
    } else if let Some(charter_query) = charter {
        let (mc, ws_root) = resolve_charter_across_workspaces(ctx, charter_query)?;
        let rel = mc
            .acts_file
            .as_ref()
            .ok_or_else(|| format!("Charter '{}' has no associated actions file", mc.title))?;
        vec![clearhead_core::charter_root(&ws_root).join(rel)]
    } else {
        let mut all = Vec::new();
        for (_, ws_dir) in ctx.workspace_dirs() {
            let files = clearhead_core::list_action_files(&ws_dir)
                .map_err(|e| format!("Failed to list workspace '{}': {}", ws_dir.display(), e))?;
            all.extend(files);
        }
        all
    };

    for actions_path in &action_files {
        let mut open = action_files::read_actions(actions_path).map_err(|e| e.to_string())?;
        if let Some(act_id) = open.iter().find(|a| act_matches(a, query)).map(|a| a.id) {
            let subtree_ids = collect_subtree_ids(&open, act_id);
            if dry_run {
                println!(
                    "Would delete action {} (+{} children)",
                    &act_id.to_string()[..8],
                    subtree_ids.len() - 1,
                );
                return Ok(());
            }
            open.retain(|a| !subtree_ids.contains(&a.id));
            super::save_file(actions_path, &open)?;
            info!(%act_id, children = subtree_ids.len() - 1, "Action subtree deleted");
            println!("Deleted action {} (+{} children)", &act_id.to_string()[..8], subtree_ids.len() - 1);
            return Ok(());
        }

        // Check completed file — single action only (no tree context in closed file)
        let completed_path = action_files::completed_actions_path(actions_path);
        let mut closed = action_files::read_actions(&completed_path).map_err(|e| e.to_string())?;
        if let Some(pos) = closed.iter().position(|a| act_matches(a, query)) {
            let act_id = closed[pos].id;
            if dry_run {
                println!("Would delete action {}", &act_id.to_string()[..8]);
                return Ok(());
            }
            closed.remove(pos);
            action_files::write_actions(&closed, &completed_path).map_err(|e| e.to_string())?;
            info!(%act_id, "Action deleted from completed");
            println!("Deleted action {}", &act_id.to_string()[..8]);
            return Ok(());
        }
    }

    Err(format!("No action found matching '{}'", query))
}

/// Cancel an open action and all its descendants (moves to `.completed.actions` with Cancelled state).
pub fn cancel_action(
    ctx: &CommandContext,
    query: &str,
    charter: &Option<String>,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, charter, query)?;

    let act_id = find_act_mut(&mut open_acts, query)
        .ok_or_else(|| format!("No open action found matching '{}'", query))?
        .id;

    let subtree_ids = collect_subtree_ids(&open_acts, act_id);

    if dry_run {
        println!(
            "Would cancel action {} and {} child(ren)",
            &act_id.to_string()[..8],
            subtree_ids.len() - 1,
        );
        return Ok(());
    }

    let now = Local::now();
    let mut to_close: Vec<Action> = open_acts
        .iter()
        .filter(|a| subtree_ids.contains(&a.id))
        .map(|a| {
            let mut closed = a.clone();
            closed.state = ActionState::Cancelled;
            closed.completed_at = Some(now);
            closed.parent_id = None;
            closed
        })
        .collect();

    open_acts.retain(|a| !subtree_ids.contains(&a.id));
    super::save_file(&actions_path, &open_acts)?;

    let completed_path = action_files::completed_actions_path(&actions_path);
    let mut closed = action_files::read_actions(&completed_path).map_err(|e| e.to_string())?;
    closed.append(&mut to_close);
    action_files::write_actions(&closed, &completed_path).map_err(|e| e.to_string())?;

    info!(%act_id, children = subtree_ids.len() - 1, "Action subtree cancelled");
    println!("Cancelled action {} (+{} children)", act_id, subtree_ids.len() - 1);
    Ok(())
}

// ============================================================================
// read actions
// ============================================================================

/// List actions, optionally filtered by charter, plan name, and/or context tags.
pub fn read_actions_cmd(
    ctx: &CommandContext,
    format: Option<crate::argparser::OutputMode>,
    plan_filter: Option<&str>,
    charter_filter: Option<&str>,
    context_filter: &[String],
    open_only: bool,
    states: &[crate::argparser::ActionStateArg],
    file: &Option<PathBuf>,
) -> Result<(), String> {
    let charter_acts_file: Option<PathBuf> = if let Some(query) = charter_filter {
        let (mc, ws_root) = resolve_charter_across_workspaces(ctx, query)?;
        let rel = mc
            .acts_file
            .as_ref()
            .ok_or_else(|| format!("Charter '{}' has no associated actions file", mc.title))?;
        let root = clearhead_core::charter_root(&ws_root);
        Some(root.join(rel))
    } else {
        None
    };
    let effective_file = charter_acts_file.as_ref().or(file.as_ref()).cloned();

    let wc = ctx.workspace_config();
    let multi_ws = effective_file.is_none() && !wc.additional_workspaces.is_empty();

    // Pre-expand context filter tags downward (general → specific) so ActionFilter::matches
    // can do a simple set-membership check. Filtering by "computer" will match actions
    // tagged "terminal" or "neovim" because those are descendants of "computer".
    let expanded_context_tags: Vec<String> = context_filter
        .iter()
        .flat_map(|t| wc.descendants_and_self(t))
        .collect();
    let action_filter = clearhead_core::ActionFilter {
        open_only,
        states: states.iter().map(|s| (*s).into()).collect(),
        context_tags: expanded_context_tags,
        plan_ref: plan_filter.map(String::from),
    };

    // ws_acts drives non-TTY output (DSL, JSON, table). collect open_only early as a
    // performance hint; action_filter.matches enforces all remaining criteria.
    let ws_acts: Vec<(Option<String>, Action)> = if multi_ws {
        collect_workspace_actions(ctx, open_only)?
    } else {
        collect_all_actions(ctx, &effective_file, open_only)?
            .into_iter()
            .map(|a| (None, a))
            .collect()
    };

    let filtered: Vec<(Option<&str>, &Action)> = ws_acts
        .iter()
        .filter(|(_, a)| action_filter.matches(a))
        .map(|(ws, a)| (ws.as_deref(), a))
        .collect();

    match format {
        Some(crate::argparser::OutputMode::JsonLd) => {
            let model = clearhead_core::load_domain_model(&ctx.data_dir)
                .map_err(|e| e.to_string())?;
            let jsonld = clearhead_core::graph::serialize_domain_to_jsonld(&model)
                .map_err(|e| format!("Failed to serialize JSON-LD: {}", e))?;
            println!("{}", jsonld);
        }
        Some(crate::argparser::OutputMode::Ids) => {
            for (_, action) in &filtered {
                println!("{}", action.id);
            }
        }
        Some(crate::argparser::OutputMode::Table) => print_acts_table(&filtered, multi_ws),
        None => {
            if !std::io::stdout().is_terminal() {
                // Pipe/redirect: emit .actions DSL so output can be saved or piped.
                let acts: Vec<&Action> = filtered.iter().map(|(_, a)| *a).collect();
                let list: clearhead_core::ActionList = acts.into_iter().cloned().collect();
                let text = clearhead_core::format(&list, clearhead_core::OutputFormat::Actions, None, None)
                    .map_err(|e| format!("Failed to format actions: {}", e))?;
                print!("{}", text);
            } else {
                // TTY: always render the domain hierarchy tree, filtered if needed.
                let primary = clearhead_core::load_domain_model(&ctx.data_dir)
                    .map_err(|e| e.to_string())?;

                let mut model = if let Some(query) = charter_filter {
                    let charter = super::charter::resolve_charter(&primary.charters, query)
                        .ok_or_else(|| format!("No charter found matching '{}'", query))?
                        .clone();
                    clearhead_core::DomainModel { objectives: vec![], charters: vec![charter] }
                } else {
                    primary
                };
                clearhead_core::apply_filter(&mut model, &action_filter);

                if multi_ws {
                    let ws_name = ctx.config.workspace_name.clone()
                        .unwrap_or_else(|| "primary".to_string());
                    println!("▸ {}", ws_name);
                    print!("{}", crate::display::render_domain_tree(&model));
                    for path_str in &wc.additional_workspaces {
                        let path = std::path::Path::new(path_str);
                        match clearhead_core::load_domain_model(path) {
                            Ok(mut ws_model) => {
                                clearhead_core::apply_filter(&mut ws_model, &action_filter);
                                println!("▸ {}", super::workspace_name_from_path(path));
                                print!("{}", crate::display::render_domain_tree(&ws_model));
                            }
                            Err(e) => tracing::warn!("Skipping workspace '{}': {}", path_str, e),
                        }
                    }
                } else {
                    print!("{}", crate::display::render_domain_tree(&model));
                }
            }
        }
    }

    Ok(())
}

/// Collect actions from the primary workspace and all configured additional workspaces.
/// Each action is paired with its workspace name (`None` when not in multi-workspace context).
fn collect_workspace_actions(
    ctx: &CommandContext,
    open_only: bool,
) -> Result<Vec<(Option<String>, Action)>, String> {
    let multi_ws = ctx.workspace_dirs().len() > 1;
    let mut result = Vec::new();

    for (ws_name, ws_path) in ctx.workspace_dirs() {
        let is_primary = ws_path == ctx.data_dir;
        let label = if multi_ws { Some(ws_name) } else { None };

        let files = match clearhead_core::list_action_files(&ws_path) {
            Ok(f) => f,
            Err(e) if is_primary => return Err(e.to_string()),
            Err(e) => { warn!("Skipping workspace '{}': {}", ws_path.display(), e); continue; }
        };

        for actions_path in files {
            let mut open = match action_files::read_actions(&actions_path) {
                Ok(a) => a,
                Err(e) => { warn!("Skipping {}: {}", actions_path.display(), e); continue; }
            };
            if open_only { open.retain(is_open_act); }
            for act in open { result.push((label.clone(), act)); }
            if !open_only {
                let completed_path = action_files::completed_actions_path(&actions_path);
                if let Ok(completed) = action_files::read_actions(&completed_path) {
                    for act in completed { result.push((label.clone(), act)); }
                }
            }
        }
    }

    Ok(result)
}

/// Show details for one action from open and completed action stores.
pub fn show_action(ctx: &CommandContext, query: &str, file: &Option<PathBuf>) -> Result<(), String> {
    let acts: Vec<Action> = if file.is_none() && !ctx.workspace_config().additional_workspaces.is_empty() {
        collect_workspace_actions(ctx, false)?.into_iter().map(|(_, a)| a).collect()
    } else {
        collect_all_actions(ctx, file, false)?
    };

    let act = acts
        .iter()
        .find(|act| act_matches(act, query))
        .ok_or_else(|| format!("No action found matching '{}'", query))?;

    println!("{}", crate::display::render_action_detail(act));
    Ok(())
}

// ============================================================================
// archive actions
// ============================================================================

/// Sweep completed/cancelled actions from `.actions` into `.completed.actions`.
pub fn archive_actions(
    ctx: &CommandContext,
    scope: &Option<String>,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let charter_paths: Vec<PathBuf> = if let Some(f) = file {
        vec![f.clone()]
    } else if let Some(s) = scope {
        use crate::commands::resolver::{ResolvedScope, resolve_domain_ref};
        match resolve_domain_ref(ctx, s)? {
            ResolvedScope::Charter { file_path }
            | ResolvedScope::Plan { file_path, .. }
            | ResolvedScope::Action { file_path, .. } => vec![file_path],
        }
    } else {
        clearhead_core::list_action_files(&ctx.data_dir)
            .map_err(|e| format!("Failed to list workspace: {}", e))?
    };

    let mut total_archived = 0usize;
    let mut charters_touched = 0usize;

    for actions_path in &charter_paths {
        let open_acts = action_files::read_actions(actions_path).map_err(|e| e.to_string())?;

        let (to_close, to_keep): (Vec<Action>, Vec<Action>) = open_acts
            .into_iter()
            .partition(|a| matches!(a.state, ActionState::Completed | ActionState::Cancelled));

        if to_close.is_empty() {
            continue;
        }

        if dry_run {
            println!(
                "Would archive {} action(s) from {}",
                to_close.len(),
                actions_path.display()
            );
        } else {
            super::save_file(actions_path, &to_keep)?;

            let completed_path = action_files::completed_actions_path(actions_path);
            let mut existing_closed =
                action_files::read_actions(&completed_path).map_err(|e| e.to_string())?;
            // Clear parent_id — the completed file is a flat list with no parent context.
            existing_closed.extend(to_close.iter().cloned().map(|mut a| { a.parent_id = None; a }));
            action_files::write_actions(&existing_closed, &completed_path).map_err(|e| e.to_string())?;

            info!(
                count = to_close.len(),
                charter = %actions_path.display(),
                "Actions archived"
            );
        }

        total_archived += to_close.len();
        charters_touched += 1;
    }

    if total_archived == 0 {
        println!("Nothing to archive.");
    } else if dry_run {
        println!(
            "Would archive {} action(s) across {} charter(s).",
            total_archived, charters_touched
        );
    } else {
        println!(
            "Archived {} action(s) across {} charter(s).",
            total_archived, charters_touched
        );
    }

    Ok(())
}

// ============================================================================
// Private helpers
// ============================================================================

fn find_and_load_open_acts(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    charter: &Option<String>,
    query: &str,
) -> Result<(PathBuf, ActionList), String> {
    if let Some(path) = file {
        let acts = super::load_file_for_mutation(path, "action lifecycle")?;
        return Ok((path.clone(), acts));
    }
    if let Some(charter_query) = charter {
        let (mc, ws_root) = resolve_charter_across_workspaces(ctx, charter_query)?;
        let rel = mc
            .acts_file
            .as_ref()
            .ok_or_else(|| format!("Charter '{}' has no associated actions file", mc.title))?;
        let path = clearhead_core::charter_root(&ws_root).join(rel);
        let acts = super::load_file_for_mutation(&path, "action lifecycle")?;
        return Ok((path, acts));
    }
    find_act_in_open_files(&ctx.data_dir, query)
}

/// Scan `.actions` files in the workspace for one containing an action matching `query`.
fn find_act_in_open_files(data_dir: &Path, query: &str) -> Result<(PathBuf, ActionList), String> {
    let action_files = clearhead_core::list_action_files(data_dir)
        .map_err(|e| format!("Failed to list workspace: {}", e))?;

    for actions_path in action_files {
        let action_list = action_files::read_actions(&actions_path).map_err(|e| e.to_string())?;
        if action_list
            .iter()
            .any(|a| is_open_act(a) && act_matches(a, query))
        {
            return Ok((actions_path, action_list));
        }
    }

    Err(format!("No open action found matching '{}'", query))
}

/// For each act in `acts`, if its plan has a template, replace it with the instantiated
/// template (template root gets the occurrence UUID + scheduled_at for idempotency).
/// Acts with no template are passed through unchanged.
fn resolve_expanded_acts(
    acts: Vec<Action>,
    all_plans: &[clearhead_core::domain::Plan],
    charter_dir: &Path,
    data_root: &Path,
) -> Vec<Action> {
    let mut out: Vec<Action> = Vec::new();
    for act in acts {
        let occ_key = act.scheduled_at.map(|dt| dt.to_rfc3339()).unwrap_or_default();
        let matching_plan = all_plans.iter().find(|p| {
            p.external_id
                .as_deref()
                .map(|uid| clearhead_core::occurrence_act_id(uid, &occ_key) == act.id)
                .unwrap_or(false)
        });

        let template_applied = matching_plan.and_then(|plan| {
            plan.external_id.as_deref().and_then(|uid| {
                apply_template_in_place(plan, uid, &occ_key, act.id, act.scheduled_at, charter_dir, data_root)
            })
        });

        match template_applied {
            Some(instantiated) => out.extend(instantiated),
            None => out.push(act),
        }
    }
    out
}

/// Load and instantiate a template for one occurrence. Returns `None` when the plan has no
/// template or the template file can't be found/read (caller falls back to the flat action).
///
/// The first template root action receives `root_id` (the deterministic occurrence UUID) so
/// that idempotency checks in future expansion runs find it correctly.
fn apply_template_in_place(
    plan: &clearhead_core::domain::Plan,
    vevent_uid: &str,
    occ_key: &str,
    root_id: uuid::Uuid,
    scheduled_at: Option<chrono::DateTime<Local>>,
    charter_dir: &Path,
    data_root: &Path,
) -> Option<Vec<Action>> {
    use clearhead_core::workspace::ics::occurrence_act_id;

    let tpl_name = plan.template_name.as_deref()?;

    let tpl_path = match templates::resolve_template(charter_dir, data_root, tpl_name) {
        Ok(Some(p)) => p,
        Ok(None) => {
            warn!(template = %tpl_name, "Template not found, expanding flat action only");
            return None;
        }
        Err(e) => {
            warn!(template = %tpl_name, error = %e, "Failed to resolve template");
            return None;
        }
    };

    let tpl_acts = match read_actions(&tpl_path) {
        Ok(acts) => acts,
        Err(e) => {
            warn!(template = %tpl_name, path = %tpl_path.display(), error = %e, "Failed to read template");
            return None;
        }
    };

    // First template root gets the occurrence UUID so idempotency works on re-runs.
    let first_root_tpl_id = tpl_acts.iter().find(|a| a.parent_id.is_none()).map(|a| a.id);
    let uid = vevent_uid.to_string();
    let key = occ_key.to_string();

    let mut instantiated = templates::instantiate_template(
        &tpl_acts,
        |tid| {
            if Some(tid) == first_root_tpl_id {
                root_id
            } else {
                occurrence_act_id(&format!("{}:tpl:{}", uid, tid), &key)
            }
        },
        None,
    );

    // Stamp scheduled_at onto root-level actions (parent_id == None after instantiation).
    for action in &mut instantiated {
        if action.parent_id.is_none() {
            action.scheduled_at = scheduled_at;
        }
    }

    Some(instantiated)
}

fn act_matches(act: &Action, query: &str) -> bool {
    let q = query.trim_start_matches('/');
    let query_lower = q.to_lowercase();
    let id_str = act.id.to_string();
    let short = &id_str[..8.min(id_str.len())];
    id_str == q
        || short == q
        || act
            .alias
            .as_deref()
            .map(|alias| alias.eq_ignore_ascii_case(q))
            .unwrap_or(false)
        || act.name.to_lowercase().contains(&query_lower)
}

/// Collect the IDs of `root_id` and all its recursive descendants in `acts`.
fn collect_subtree_ids(acts: &ActionList, root_id: uuid::Uuid) -> Vec<uuid::Uuid> {
    let mut ids = vec![root_id];
    let mut i = 0;
    while i < ids.len() {
        let parent = ids[i];
        for a in acts.iter() {
            if a.parent_id == Some(parent) && !ids.contains(&a.id) {
                ids.push(a.id);
            }
        }
        i += 1;
    }
    ids
}

fn find_act_mut<'a>(acts: &'a mut ActionList, query: &str) -> Option<&'a mut Action> {
    acts.iter_mut()
        .find(|a| is_open_act(a) && act_matches(a, query))
}

pub(super) fn resolve_markdown_charter<'a>(
    charters: &'a [clearhead_core::MarkdownCharter],
    query: &str,
) -> Option<&'a clearhead_core::MarkdownCharter> {
    let query_lower = query.to_lowercase();
    if query.len() == 8 && query.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Some(c) = charters
            .iter()
            .find(|c| c.id.to_string().starts_with(query))
        {
            return Some(c);
        }
    }
    if let Ok(uuid) = uuid::Uuid::parse_str(query) {
        if let Some(c) = charters.iter().find(|c| c.id == uuid) {
            return Some(c);
        }
    }
    if let Some(c) = charters.iter().find(|c| {
        c.alias
            .as_deref()
            .map(|a| a.to_lowercase() == query_lower)
            .unwrap_or(false)
    }) {
        return Some(c);
    }
    charters
        .iter()
        .find(|c| c.title.to_lowercase().contains(&query_lower))
}

/// Search configured workspaces (respecting `--workspace` filter) for a charter matching `query`.
///
/// Returns the matched charter (owned) and the workspace root it came from.
pub(super) fn resolve_charter_across_workspaces(
    ctx: &CommandContext,
    query: &str,
) -> Result<(clearhead_core::MarkdownCharter, PathBuf), String> {
    for (_, ws_root) in ctx.workspace_dirs() {
        let is_primary = ws_root == ctx.data_dir;
        let mcs = match clearhead_core::load_workspace(&ws_root) {
            Ok(m) => m,
            Err(e) if is_primary => return Err(e.to_string()),
            Err(e) => { warn!("Skipping workspace '{}': {}", ws_root.display(), e); continue; }
        };
        if let Some(mc) = resolve_markdown_charter(&mcs, query) {
            return Ok((mc.clone(), ws_root));
        }
    }
    Err(format!("No charter found matching '{}'", query))
}

fn collect_all_actions(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    open_only: bool,
) -> Result<Vec<Action>, String> {
    if let Some(path) = file {
        let mut result: Vec<Action> = action_files::read_actions(path).map_err(|e| e.to_string())?;
        if open_only {
            result.retain(is_open_act);
        }
        if !open_only {
            let completed_path = action_files::completed_actions_path(path);
            result.extend(action_files::read_actions(&completed_path).map_err(|e| e.to_string())?);
        }
        return Ok(result);
    }

    let action_files = clearhead_core::list_action_files(&ctx.data_dir)
        .map_err(|e| format!("Failed to list workspace: {}", e))?;

    let mut all = Vec::new();
    for actions_path in action_files {
        let mut open = action_files::read_actions(&actions_path).map_err(|e| e.to_string())?;
        if open_only {
            open.retain(is_open_act);
        }
        all.extend(open);
        if !open_only {
            let completed_path = action_files::completed_actions_path(&actions_path);
            all.extend(action_files::read_actions(&completed_path).map_err(|e| e.to_string())?);
        }
    }
    Ok(all)
}

fn is_open_act(act: &Action) -> bool {
    !matches!(act.state, ActionState::Completed | ActionState::Cancelled)
}


fn print_acts_table(ws_acts: &[(Option<&str>, &Action)], multi_ws: bool) {
    use comfy_table::{Cell, Table};

    let mut table = Table::new();
    let mut headers: Vec<&str> = vec!["id", "state", "name", "scheduled_at", "duration"];
    if multi_ws { headers.insert(0, "workspace"); }
    table.set_header(headers);

    for (ws, act) in ws_acts {
        let short_id = &act.id.to_string()[..8];
        let state = format!("{:?}", act.state);
        let scheduled = act
            .scheduled_at
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "—".to_string());
        let duration = act
            .duration
            .map(|d| format!("{}m", d))
            .unwrap_or_else(|| "—".to_string());

        let mut row = vec![
            Cell::new(short_id),
            Cell::new(state),
            Cell::new(&act.name),
            Cell::new(scheduled),
            Cell::new(duration),
        ];
        if multi_ws { row.insert(0, Cell::new(ws.unwrap_or("—"))); }
        table.add_row(row);
    }

    println!("{}", table);
}
