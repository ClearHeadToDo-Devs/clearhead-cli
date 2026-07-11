//! Handlers for action commands (expand, complete, cancel, update, read, archive).

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use anyhow::Context;
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
) -> anyhow::Result<()> {
    let actions_path = resolve_acts_file(ctx, charter, file)?;

    let parent_id = parent
        .as_deref()
        .map(|q| -> anyhow::Result<_> {
            let mut list = action_files::read_actions(&actions_path)?;
            find_action_mut(&mut list, q)
                .map(|a| a.id)
                .ok_or_else(|| anyhow::anyhow!("No action found matching parent '{}'", q))
        })
        .transpose()?;

    let new_scheduled = scheduled_at
        .as_deref()
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Local))
                .map_err(|e| anyhow::anyhow!("Invalid --scheduled-at '{}': {}", s, e))
        })
        .transpose()?;

    let action = Action {
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

    let mut list = action_files::read_actions(&actions_path)?;
    list.push(action.clone());
    super::save_file(&actions_path, &list)?;

    info!(id = %action.id, name = %name, "Action added");
    println!("Added action {} ({})", &action.id.to_string()[..8], name);
    Ok(())
}

/// Resolve the `.actions` file path from a charter query or explicit file path.
fn resolve_acts_file(
    ctx: &CommandContext,
    charter: &Option<String>,
    file: &Option<PathBuf>,
) -> anyhow::Result<PathBuf> {
    if let Some(path) = file {
        return Ok(path.clone());
    }
    if let Some(query) = charter {
        let (mc, ws_root) = resolve_charter_across_workspaces(ctx, query)?;
        let rel = mc
            .actions_file
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Charter '{}' has no associated actions file", mc.title))?;
        let root = clearhead_core::charter_root(&ws_root);
        return Ok(root.join(rel));
    }
    anyhow::bail!("Specify --charter <name> or --file <path> to target a charter's actions file")
}

/// Expand ICS schedule VEVENTs into actions in the charter's `.actions` and
/// `.upcoming.actions` files.
pub fn expand_actions(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> anyhow::Result<()> {
    use clearhead_core::workspace::calendar::ics::parse_ics_file;
    use clearhead_core::{ExpansionConfig, upcoming_actions_path};

    let data_root = clearhead_core::workspace_data_root(&ctx.data_dir);
    let now = Local::now();

    let expansion_config = ExpansionConfig {
        total_instances: ctx.config.expansion_total_instances,
        primary_instances: ctx.config.expansion_primary_instances,
    };

    let all_entries = ctx.collect_plan_files()
        .context("Failed to discover ICS files")?;

    let entries: Vec<_> = if let Some(actions_path) = file {
        let relative = actions_path
            .strip_prefix(&data_root)
            .unwrap_or(actions_path.as_path());
        let charter_name = clearhead_core::infer_charter_name(relative).with_context(|| {
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
                Err(e) => anyhow::bail!("Failed to parse {}: {}", entry.path.display(), e),
            }
        }

        let expand_result = clearhead_core::expand_plans_into_actions(
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
        anyhow::bail!(
            "expand actions skipped {} file(s) due to parse errors",
            parse_failures.len()
        );
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
) -> anyhow::Result<()> {
    close_action_subtree(ctx, query, charter, file, dry_run, ActionState::Completed, "complete", "Completed")
}

/// Shared body of `complete`/`cancel`: resolve the action, close its subtree via
/// core's `close_subtree`, save the open list, and append the closed subtree to
/// `.completed.actions`. Only the target state and message wording differ.
fn close_action_subtree(
    ctx: &CommandContext,
    query: &str,
    charter: &Option<String>,
    file: &Option<PathBuf>,
    dry_run: bool,
    closing_state: ActionState,
    verb_present: &str,
    verb_past: &str,
) -> anyhow::Result<()> {
    let (actions_path, mut open_actions) = find_and_load_open_actions(ctx, file, charter, query)?;

    let action_id = find_action_mut(&mut open_actions, query)
        .ok_or_else(|| anyhow::anyhow!("No open action found matching '{}'", query))?
        .id;

    let subtree_ids = clearhead_core::collect_subtree_ids(&open_actions, action_id);

    if dry_run {
        println!(
            "Would {} action {} and {} child(ren)",
            verb_present,
            &action_id.to_string()[..8],
            subtree_ids.len() - 1,
        );
        return Ok(());
    }

    let now = Local::now();
    let mut to_close = clearhead_core::close_subtree(&open_actions, action_id, closing_state, now);

    open_actions.retain(|a| !subtree_ids.contains(&a.id));
    super::save_file(&actions_path, &open_actions)?;

    let completed_path = action_files::completed_actions_path(&actions_path);
    let mut closed = action_files::read_actions(&completed_path)?;
    closed.append(&mut to_close);
    action_files::write_actions(&closed, &completed_path)?;

    info!(%action_id, children = subtree_ids.len() - 1, "Action subtree {}", verb_past.to_lowercase());
    println!("{} action {} (+{} children)", verb_past, action_id, subtree_ids.len() - 1);
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
) -> anyhow::Result<()> {
    let (actions_path, mut open_actions) = find_and_load_open_actions(ctx, file, charter, query)?;

    let new_scheduled = scheduled_at
        .as_deref()
        .map(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Local))
                .map_err(|e| anyhow::anyhow!("Invalid --scheduled-at '{}': {}", s, e))
        })
        .transpose()?;

    let action_id = {
        let action = find_action_mut(&mut open_actions, query)
            .ok_or_else(|| anyhow::anyhow!("No open action found matching '{}'", query))?;
        let id = action.id;
        if !dry_run {
            clearhead_cli::mutations::apply_updates(action, clearhead_cli::mutations::ActionUpdate {
                name: name.clone(),
                description: description.clone(),
                priority,
                state: state.map(Into::into),
                scheduled_at: new_scheduled,
                duration: *duration,
                ..Default::default()
            });
        }
        id
    };

    if dry_run {
        println!("Would update action {}", &action_id.to_string()[..8]);
        return Ok(());
    }

    super::save_file(&actions_path, &open_actions)?;
    info!(%action_id, "Action updated");
    println!("Updated action {}", action_id);
    Ok(())
}

/// Delete an action from the workspace (open or closed).
pub fn delete_action(
    ctx: &CommandContext,
    query: &str,
    charter: &Option<String>,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> anyhow::Result<()> {
    // Try open actions first, then completed.
    let action_files: Vec<PathBuf> = if let Some(path) = file {
        vec![path.clone()]
    } else if let Some(charter_query) = charter {
        let (mc, ws_root) = resolve_charter_across_workspaces(ctx, charter_query)?;
        let rel = mc
            .actions_file
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Charter '{}' has no associated actions file", mc.title))?;
        vec![clearhead_core::charter_root(&ws_root).join(rel)]
    } else {
        let mut all = Vec::new();
        for (_, ws_dir) in ctx.workspace_dirs() {
            let files = clearhead_core::list_action_files(&ws_dir)
                .with_context(|| format!("Failed to list workspace '{}'", ws_dir.display()))?;
            all.extend(files);
        }
        all
    };

    for actions_path in &action_files {
        let mut open = action_files::read_actions(actions_path)?;
        if let Some(action_id) = find_best_match(&open, query, |_| true).map(|a| a.id) {
            let subtree_ids = clearhead_core::collect_subtree_ids(&open, action_id);
            if dry_run {
                println!(
                    "Would delete action {} (+{} children)",
                    &action_id.to_string()[..8],
                    subtree_ids.len() - 1,
                );
                return Ok(());
            }
            open.retain(|a| !subtree_ids.contains(&a.id));
            super::save_file(actions_path, &open)?;
            info!(%action_id, children = subtree_ids.len() - 1, "Action subtree deleted");
            println!("Deleted action {} (+{} children)", &action_id.to_string()[..8], subtree_ids.len() - 1);
            return Ok(());
        }

        // Check completed file — single action only (no tree context in closed file)
        let completed_path = action_files::completed_actions_path(actions_path);
        let mut closed = action_files::read_actions(&completed_path)?;
        if let Some(pos) = find_best_match_pos(&closed, query, |_| true) {
            let action_id = closed[pos].id;
            if dry_run {
                println!("Would delete action {}", &action_id.to_string()[..8]);
                return Ok(());
            }
            closed.remove(pos);
            action_files::write_actions(&closed, &completed_path)?;
            info!(%action_id, "Action deleted from completed");
            println!("Deleted action {}", &action_id.to_string()[..8]);
            return Ok(());
        }
    }

    anyhow::bail!("No action found matching '{}'", query)
}

/// Cancel an open action and all its descendants (moves to `.completed.actions` with Cancelled state).
pub fn cancel_action(
    ctx: &CommandContext,
    query: &str,
    charter: &Option<String>,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> anyhow::Result<()> {
    close_action_subtree(ctx, query, charter, file, dry_run, ActionState::Cancelled, "cancel", "Cancelled")
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
) -> anyhow::Result<()> {
    let charter_acts_file: Option<PathBuf> = if let Some(query) = charter_filter {
        let (mc, ws_root) = resolve_charter_across_workspaces(ctx, query)?;
        let rel = mc
            .actions_file
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Charter '{}' has no associated actions file", mc.title))?;
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

    // ws_actions drives non-TTY output (DSL, JSON, table). collect open_only early as a
    // performance hint; action_filter.matches enforces all remaining criteria.
    let ws_actions: Vec<(Option<String>, Action)> = if multi_ws {
        collect_workspace_actions(ctx, open_only)?
    } else {
        collect_all_actions(ctx, &effective_file, open_only)?
            .into_iter()
            .map(|a| (None, a))
            .collect()
    };

    let filtered: Vec<(Option<&str>, &Action)> = ws_actions
        .iter()
        .filter(|(_, a)| action_filter.matches(a))
        .map(|(ws, a)| (ws.as_deref(), a))
        .collect();

    match format {
        Some(crate::argparser::OutputMode::JsonLd) => {
            // Serialize the *filtered* model — --charter/--context/--open-only/--state
            // must narrow JSON-LD output just as they narrow the table and tree.
            let model = filtered_primary_model(ctx, charter_filter, &action_filter)?;
            let jsonld = clearhead_core::graph::serialize_domain_to_jsonld(&model)
                .context("Failed to serialize JSON-LD")?;
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
                let actions: Vec<&Action> = filtered.iter().map(|(_, a)| *a).collect();
                let list: clearhead_core::ActionList = actions.into_iter().cloned().collect();
                let text = clearhead_core::format(&list, clearhead_core::OutputFormat::Actions, None, None)
                    .map_err(|e| anyhow::anyhow!("Failed to format actions: {}", e))?;
                print!("{}", text);
            } else {
                // TTY: always render the domain hierarchy tree, filtered if needed.
                let model = filtered_primary_model(ctx, charter_filter, &action_filter)?;

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

/// Load the primary domain model, optionally narrowed to a single charter, with
/// the action filter applied. Shared by the JSON-LD and TTY branches so both
/// honor --charter/--context/--open-only/--state identically — the JSON path
/// used to skip this and serialize the whole workspace unfiltered.
fn filtered_primary_model(
    ctx: &CommandContext,
    charter_filter: Option<&str>,
    action_filter: &clearhead_core::ActionFilter,
) -> anyhow::Result<clearhead_core::DomainModel> {
    let primary = ctx.load_model()?;
    let mut model = if let Some(query) = charter_filter {
        let charter = super::charter::resolve_charter(&primary.charters, query)
            .ok_or_else(|| anyhow::anyhow!("No charter found matching '{}'", query))?
            .clone();
        clearhead_core::DomainModel { objectives: vec![], charters: vec![charter] }
    } else {
        primary
    };
    clearhead_core::apply_filter(&mut model, action_filter);
    Ok(model)
}

/// Collect actions from the primary workspace and all configured additional workspaces.
/// Each action is paired with its workspace name (`None` when not in multi-workspace context).
/// Same convergence as `collect_all_actions`, fanned out across every configured
/// workspace: the loader gives journal recovery, sidecar hydration, and its own
/// load-finding warnings per workspace, instead of the CLI re-deriving a coarser
/// per-file warn! from a raw parse.
fn collect_workspace_actions(
    ctx: &CommandContext,
    open_only: bool,
) -> anyhow::Result<Vec<(Option<String>, Action)>> {
    let multi_ws = ctx.workspace_dirs().len() > 1;
    let mut result = Vec::new();

    for (ws_name, ws_path) in ctx.workspace_dirs() {
        let is_primary = ws_path == ctx.data_dir;
        let label = if multi_ws { Some(ws_name) } else { None };

        let charters = match clearhead_core::load_workspace(&ws_path) {
            Ok(c) => c,
            Err(e) if is_primary => return Err(e.into()),
            Err(e) => { warn!("Skipping workspace '{}': {}", ws_path.display(), e); continue; }
        };
        let charter_root = clearhead_core::charter_root(&ws_path);

        for mc in &charters {
            let mut open: Vec<Action> = mc.actions.iter().map(|sourced| sourced.action.clone()).collect();
            if open_only { open.retain(is_open_action); }
            for action in open { result.push((label.clone(), action)); }
            if !open_only {
                if let Some(actions_file) = &mc.actions_file {
                    let completed_path = action_files::completed_actions_path(&charter_root.join(actions_file));
                    if let Ok(completed) = action_files::read_actions(&completed_path) {
                        for action in completed { result.push((label.clone(), action)); }
                    }
                }
            }
        }
    }

    Ok(result)
}

/// Show details for one action from open and completed action stores.
pub fn show_action(ctx: &CommandContext, query: &str, file: &Option<PathBuf>) -> anyhow::Result<()> {
    let actions: Vec<Action> = if file.is_none() && !ctx.workspace_config().additional_workspaces.is_empty() {
        collect_workspace_actions(ctx, false)?.into_iter().map(|(_, a)| a).collect()
    } else {
        collect_all_actions(ctx, file, false)?
    };

    let action = find_best_match(&actions, query, |_| true)
        .ok_or_else(|| anyhow::anyhow!("No action found matching '{}'", query))?;

    println!("{}", crate::display::render_action_detail(action));
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
) -> anyhow::Result<()> {
    let charter_paths: Vec<PathBuf> = if let Some(f) = file {
        vec![f.clone()]
    } else if let Some(s) = scope {
        use crate::commands::resolver::{ResolvedScope, resolve_domain_ref};
        match resolve_domain_ref(ctx, s)? {
            ResolvedScope::Charter { file_path }
            | ResolvedScope::Plan { file_path }
            | ResolvedScope::Action { file_path } => vec![file_path],
        }
    } else {
        clearhead_core::list_action_files(&ctx.data_dir)
            .context("Failed to list workspace")?
    };

    let mut total_archived = 0usize;
    let mut charters_touched = 0usize;

    for actions_path in &charter_paths {
        let open_actions = action_files::read_actions(actions_path)?;

        let (to_close, to_keep): (Vec<Action>, Vec<Action>) = open_actions
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
                action_files::read_actions(&completed_path)?;
            // Clear parent_id — the completed file is a flat list with no parent context.
            existing_closed.extend(to_close.iter().cloned().map(|mut a| { a.parent_id = None; a }));
            action_files::write_actions(&existing_closed, &completed_path)?;

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

fn find_and_load_open_actions(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    charter: &Option<String>,
    query: &str,
) -> anyhow::Result<(PathBuf, ActionList)> {
    if let Some(path) = file {
        let actions = super::load_file_for_mutation(path, "action lifecycle")?;
        return Ok((path.clone(), actions));
    }
    if let Some(charter_query) = charter {
        let (mc, ws_root) = resolve_charter_across_workspaces(ctx, charter_query)?;
        let rel = mc
            .actions_file
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Charter '{}' has no associated actions file", mc.title))?;
        let path = clearhead_core::charter_root(&ws_root).join(rel);
        let actions = super::load_file_for_mutation(&path, "action lifecycle")?;
        return Ok((path, actions));
    }
    find_act_in_open_files(&ctx.data_dir, query)
}

/// Scan `.actions` files in the workspace for one containing an action matching `query`.
fn find_act_in_open_files(data_dir: &Path, query: &str) -> anyhow::Result<(PathBuf, ActionList)> {
    let action_files = clearhead_core::list_action_files(data_dir)
        .context("Failed to list workspace")?;

    for actions_path in action_files {
        let action_list = action_files::read_actions(&actions_path)?;
        if action_list
            .iter()
            .any(|a| is_open_action(a) && action_matches(a, query))
        {
            return Ok((actions_path, action_list));
        }
    }

    anyhow::bail!("No open action found matching '{}'", query)
}

/// For each action in `actions`, if its plan has a template, replace it with the instantiated
/// template (template root gets the occurrence UUID + scheduled_at for idempotency).
/// Actions with no template are passed through unchanged.
fn resolve_expanded_acts(
    actions: Vec<Action>,
    all_plans: &[clearhead_core::domain::Plan],
    charter_dir: &Path,
    data_root: &Path,
) -> Vec<Action> {
    let mut out: Vec<Action> = Vec::new();
    for action in actions {
        let occ_key = action.scheduled_at.map(|dt| dt.to_rfc3339()).unwrap_or_default();
        let matching_plan = all_plans.iter().find(|p| {
            p.external_id
                .as_deref()
                .map(|uid| clearhead_core::occurrence_action_id(uid, &occ_key) == action.id)
                .unwrap_or(false)
        });

        let template_applied = matching_plan.and_then(|plan| {
            plan.external_id.as_deref().and_then(|uid| {
                apply_template_in_place(plan, uid, &occ_key, action.id, action.scheduled_at, charter_dir, data_root)
            })
        });

        match template_applied {
            Some(instantiated) => out.extend(instantiated),
            None => out.push(action),
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
    use clearhead_core::workspace::calendar::ics::occurrence_action_id;

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
        Ok(actions) => actions,
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
                occurrence_action_id(&format!("{}:tpl:{}", uid, tid), &key)
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

/// Precedence tier for `query` against `action`: 0 = full UUID, 1 = short UUID,
/// 2 = alias, 3 = name-contains, `None` = no match. Lower wins.
fn action_match_tier(action: &Action, query: &str) -> Option<u8> {
    let q = query.trim_start_matches('/');
    let id_str = action.id.to_string();
    let short = &id_str[..8.min(id_str.len())];
    if id_str == q {
        Some(0)
    } else if short == q {
        Some(1)
    } else if action.alias.as_deref().map(|alias| alias.eq_ignore_ascii_case(q)).unwrap_or(false) {
        Some(2)
    } else if action.name.to_lowercase().contains(&q.to_lowercase()) {
        Some(3)
    } else {
        None
    }
}

/// True if `query` matches `action` under any tier. Existence-only — does not
/// resolve precedence across a list, so never use this to pick *which* action
/// when more than one might match. Use `find_best_match`/`_mut`/`_pos` for that.
fn action_matches(action: &Action, query: &str) -> bool {
    action_match_tier(action, query).is_some()
}

/// Resolve `query` against `actions` honoring precedence — full UUID, then short
/// UUID, then alias, then name-contains — across the whole list, rather than the
/// first list-order action that matches any criterion. An earlier name-contains
/// match must never shadow a later exact alias or UUID match.
fn find_best_match_pos(actions: &[Action], query: &str, filter: impl Fn(&Action) -> bool) -> Option<usize> {
    (0..=3u8).find_map(|tier| {
        actions
            .iter()
            .position(|a| filter(a) && action_match_tier(a, query) == Some(tier))
    })
}

fn find_best_match<'a>(actions: &'a [Action], query: &str, filter: impl Fn(&Action) -> bool) -> Option<&'a Action> {
    find_best_match_pos(actions, query, filter).map(|i| &actions[i])
}

fn find_action_mut<'a>(actions: &'a mut ActionList, query: &str) -> Option<&'a mut Action> {
    let idx = find_best_match_pos(actions, query, is_open_action)?;
    actions.get_mut(idx)
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
) -> anyhow::Result<(clearhead_core::MarkdownCharter, PathBuf)> {
    for (_, ws_root) in ctx.workspace_dirs() {
        let is_primary = ws_root == ctx.data_dir;
        let mcs = match clearhead_core::load_workspace(&ws_root) {
            Ok(m) => m,
            Err(e) if is_primary => return Err(e.into()),
            Err(e) => { warn!("Skipping workspace '{}': {}", ws_root.display(), e); continue; }
        };
        if let Some(mc) = resolve_markdown_charter(&mcs, query) {
            return Ok((mc.clone(), ws_root));
        }
    }
    anyhow::bail!("No charter found matching '{}'", query)
}

/// True if `actions_file` (relative to the charter root) resolves to the same
/// file as `target` (an absolute or CWD-relative path from the caller).
fn same_actions_file(charter_root: &Path, actions_file: &Path, target: &Path) -> bool {
    let candidate = charter_root.join(actions_file);
    let candidate = std::fs::canonicalize(&candidate).unwrap_or(candidate);
    let target = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    candidate == target
}

/// Collect actions for the `read` command via the workspace loader — the same
/// journal recovery, sidecar hydration, and load-finding warnings every other
/// command gets. `.completed.actions` files fall outside the loader's domain
/// model by design (they're a closed-action archive, not live workspace state,
/// see `discover_action_files`), so those are still read directly per matching
/// file when `open_only` is false.
fn collect_all_actions(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    open_only: bool,
) -> anyhow::Result<Vec<Action>> {
    let charter_root = clearhead_core::charter_root(&ctx.data_dir);
    let charters = clearhead_core::load_workspace(&ctx.data_dir)?;

    let matches = |mc: &clearhead_core::MarkdownCharter| match (file, &mc.actions_file) {
        (Some(target), Some(actions_file)) => same_actions_file(&charter_root, actions_file, target),
        (Some(_), None) => false,
        (None, _) => true,
    };

    let matching: Vec<&clearhead_core::MarkdownCharter> = charters.iter().filter(|mc| matches(mc)).collect();

    let mut result: Vec<Action> = matching
        .iter()
        .flat_map(|mc| mc.actions.iter().map(|sourced| sourced.action.clone()))
        .collect();
    if open_only {
        result.retain(is_open_action);
    } else {
        for mc in &matching {
            let Some(actions_file) = &mc.actions_file else { continue };
            let completed_path = action_files::completed_actions_path(&charter_root.join(actions_file));
            result.extend(action_files::read_actions(&completed_path)?);
        }
    }
    Ok(result)
}

fn is_open_action(action: &Action) -> bool {
    !matches!(action.state, ActionState::Completed | ActionState::Cancelled)
}


fn print_acts_table(ws_actions: &[(Option<&str>, &Action)], multi_ws: bool) {
    use comfy_table::{Cell, Table};

    let mut table = Table::new();
    let mut headers: Vec<&str> = vec!["id", "state", "name", "scheduled_at", "duration"];
    if multi_ws { headers.insert(0, "workspace"); }
    table.set_header(headers);

    for (ws, action) in ws_actions {
        let short_id = &action.id.to_string()[..8];
        let state = format!("{:?}", action.state);
        let scheduled = action
            .scheduled_at
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "—".to_string());
        let duration = action
            .duration
            .map(|d| format!("{}m", d))
            .unwrap_or_else(|| "—".to_string());

        let mut row = vec![
            Cell::new(short_id),
            Cell::new(state),
            Cell::new(&action.name),
            Cell::new(scheduled),
            Cell::new(duration),
        ];
        if multi_ws { row.insert(0, Cell::new(ws.unwrap_or("—"))); }
        table.add_row(row);
    }

    println!("{}", table);
}

#[cfg(test)]
mod resolution_tests {
    use super::*;
    use uuid::Uuid;

    fn make_action(name: &str, alias: Option<&str>) -> Action {
        Action {
            id: Uuid::now_v7(),
            name: name.to_string(),
            alias: alias.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn alias_beats_earlier_name_contains_match() {
        // An earlier name-contains match must not shadow a later exact alias —
        // the bug that let `complete`/`update`/`cancel`/`delete` act on the
        // wrong action when a query happened to substring-match an earlier name.
        let actions = vec![
            make_action("Fix staging server", None),
            make_action("Deploy", Some("staging")),
        ];

        let found = find_best_match(&actions, "staging", |_| true).unwrap();
        assert_eq!(found.name, "Deploy");
        assert_eq!(found.alias.as_deref(), Some("staging"));
    }

    #[test]
    fn short_uuid_beats_alias_and_name() {
        let mut target = make_action("Target action", None);
        target.id = Uuid::parse_str("aaaaaaaa-0000-7000-8000-000000000000").unwrap();
        let short = &target.id.to_string()[..8];
        let actions = vec![
            make_action("Alias holder", Some(short.to_owned()).as_deref()),
            target.clone(),
        ];

        let found = find_best_match(&actions, short, |_| true).unwrap();
        assert_eq!(found.id, target.id);
    }

    #[test]
    fn full_uuid_beats_everything() {
        let target = make_action("Target action", None);
        let decoy = make_action(&target.id.to_string(), None);
        let actions = vec![decoy, target.clone()];

        let found = find_best_match(&actions, &target.id.to_string(), |_| true).unwrap();
        assert_eq!(found.id, target.id);
    }
}
