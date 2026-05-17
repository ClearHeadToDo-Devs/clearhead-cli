//! Handlers for action commands (expand, complete, cancel, update, read, archive).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Local;
use tracing::{info, warn};

use clearhead_core::workspace::{acts, read_acts, templates};
use clearhead_core::{Action, ActionList, ActionState, WorkspaceConfig};

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
    scheduled_at: &Option<String>,
    duration: Option<u32>,
    dry_run: bool,
) -> Result<(), String> {
    let actions_path = resolve_acts_file(ctx, charter, file)?;

    let parent_id = parent
        .as_deref()
        .map(|q| {
            let mut list = acts::read_acts(&actions_path).map_err(|e| e.to_string())?;
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
        scheduled_at: new_scheduled,
        duration: duration,
        created_at: Some(Local::now()),
        ..Default::default()
    };

    if dry_run {
        println!("Would add action '{}' to {}", name, actions_path.display());
        return Ok(());
    }

    let mut list = acts::read_acts(&actions_path).map_err(|e| e.to_string())?;
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
        let mcs =
            clearhead_core::load_workspace(&ctx.data_dir).map_err(|e| e.to_string())?;
        let mc = resolve_markdown_charter(&mcs, query)
            .ok_or_else(|| format!("No charter found matching '{}'", query))?;
        let rel = mc
            .acts_file
            .as_ref()
            .ok_or_else(|| format!("Charter '{}' has no associated actions file", mc.title))?;
        let root = clearhead_core::charter_root(&ctx.data_dir);
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
    use clearhead_core::{ExpansionConfig, upcoming_acts_path};

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
        let upcoming_path = upcoming_acts_path(&actions_path);
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
                Ok(plans) => all_plans.extend(plans),
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
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, query)?;

    let (act_id, completed_act) = {
        let act = find_act_mut(&mut open_acts, query)
            .ok_or_else(|| format!("No open action found matching '{}'", query))?;
        let id = act.id;
        if !dry_run {
            act.state = ActionState::Completed;
            act.completed_at = Some(Local::now());
        }
        (id, act.clone())
    };

    if dry_run {
        println!("Would complete action {}", &act_id.to_string()[..8]);
        return Ok(());
    }

    open_acts.retain(|a| a.id != act_id);
    super::save_file(&actions_path, &open_acts)?;

    let completed_path = acts::completed_acts_path(&actions_path);
    let mut closed = acts::read_acts(&completed_path).map_err(|e| e.to_string())?;
    closed.push(completed_act);
    acts::write_acts(&closed, &completed_path).map_err(|e| e.to_string())?;

    info!(%act_id, "Action marked completed");
    println!("Completed action {}", act_id);
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
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, query)?;

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
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    // Try open actions first, then completed.
    let action_files: Vec<PathBuf> = if let Some(path) = file {
        vec![path.clone()]
    } else {
        clearhead_core::list_action_files(&ctx.data_dir)
            .map_err(|e| format!("Failed to list workspace: {}", e))?
    };

    for actions_path in &action_files {
        let mut open = acts::read_acts(actions_path).map_err(|e| e.to_string())?;
        if let Some(pos) = open.iter().position(|a| act_matches(a, query)) {
            let act = &open[pos];
            let act_id = act.id;
            let act_name = act.name.clone();
            if dry_run {
                println!("Would delete action {} ({})", &act_id.to_string()[..8], act_name);
                return Ok(());
            }
            open.remove(pos);
            super::save_file(actions_path, &open)?;
            info!(%act_id, "Action deleted");
            println!("Deleted action {} ({})", &act_id.to_string()[..8], act_name);
            return Ok(());
        }

        // Check completed file too
        let completed_path = acts::completed_acts_path(actions_path);
        let mut closed = acts::read_acts(&completed_path).map_err(|e| e.to_string())?;
        if let Some(pos) = closed.iter().position(|a| act_matches(a, query)) {
            let act = &closed[pos];
            let act_id = act.id;
            let act_name = act.name.clone();
            if dry_run {
                println!("Would delete action {} ({})", &act_id.to_string()[..8], act_name);
                return Ok(());
            }
            closed.remove(pos);
            acts::write_acts(&closed, &completed_path).map_err(|e| e.to_string())?;
            info!(%act_id, "Action deleted from completed");
            println!("Deleted action {} ({})", &act_id.to_string()[..8], act_name);
            return Ok(());
        }
    }

    Err(format!("No action found matching '{}'", query))
}

/// Cancel an open action (moves to `.completed.actions` with Cancelled state).
pub fn cancel_action(
    ctx: &CommandContext,
    query: &str,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, query)?;

    let (act_id, cancelled_act) = {
        let act = find_act_mut(&mut open_acts, query)
            .ok_or_else(|| format!("No open action found matching '{}'", query))?;
        let id = act.id;
        if !dry_run {
            act.state = ActionState::Cancelled;
        }
        (id, act.clone())
    };

    if dry_run {
        println!("Would cancel action {}", &act_id.to_string()[..8]);
        return Ok(());
    }

    open_acts.retain(|a| a.id != act_id);
    super::save_file(&actions_path, &open_acts)?;

    let completed_path = acts::completed_acts_path(&actions_path);
    let mut closed = acts::read_acts(&completed_path).map_err(|e| e.to_string())?;
    closed.push(cancelled_act);
    acts::write_acts(&closed, &completed_path).map_err(|e| e.to_string())?;

    info!(%act_id, "Action cancelled");
    println!("Cancelled action {}", act_id);
    Ok(())
}

// ============================================================================
// read actions
// ============================================================================

/// List actions, optionally filtered by charter, plan name, and/or context tags.
pub fn read_actions_cmd(
    ctx: &CommandContext,
    format: Option<crate::argparser::ActFormat>,
    plan_filter: Option<&str>,
    charter_filter: Option<&str>,
    context_filter: &[String],
    open_only: bool,
    file: &Option<PathBuf>,
) -> Result<(), String> {
    let charter_acts_file: Option<PathBuf> = if let Some(query) = charter_filter {
        let mcs =
            clearhead_core::load_workspace(&ctx.data_dir).map_err(|e| e.to_string())?;
        let mc = resolve_markdown_charter(&mcs, query)
            .ok_or_else(|| format!("No charter found matching '{}'", query))?;
        let rel = mc
            .acts_file
            .as_ref()
            .ok_or_else(|| format!("Charter '{}' has no associated actions file", mc.title))?;
        let root = clearhead_core::charter_root(&ctx.data_dir);
        Some(root.join(rel))
    } else {
        None
    };
    let effective_file = charter_acts_file.as_ref().or(file.as_ref()).cloned();
    let acts = collect_all_actions(ctx, &effective_file, open_only)?;

    // Build the filter set: lowercase raw filter tags. Each action's tags are expanded upward
    // (tag + all ancestors) and checked for intersection — so filtering by +computer matches
    // actions tagged +neovim because neovim → terminal → computer.
    let wc = WorkspaceConfig {
        tag_hierarchies: ctx.config.tag_hierarchies.clone(),
        ..Default::default()
    };
    let filter_set: std::collections::HashSet<String> =
        context_filter.iter().map(|t| t.to_lowercase()).collect();

    let acts: Vec<&Action> = acts
        .iter()
        .filter(|a| {
            if let Some(filter) = plan_filter {
                let id_str = a.id.to_string();
                let short = &id_str[..8.min(id_str.len())];
                if !(id_str == filter || short == filter || a.name.contains(filter)) {
                    return false;
                }
            }
            if !filter_set.is_empty() {
                return a.contexts.as_ref().map_or(false, |cs| {
                    cs.iter().any(|c| {
                        wc.expand_tag(c).iter().any(|t| filter_set.contains(t))
                    })
                });
            }
            true
        })
        .collect();

    match format {
        Some(crate::argparser::ActFormat::Json) => {
            let json = serde_json::to_string_pretty(&acts)
                .map_err(|e| format!("JSON serialization failed: {}", e))?;
            println!("{}", json);
        }
        Some(crate::argparser::ActFormat::Table) => print_acts_table(&acts),
        None => print_acts_tree(&acts),
    }

    Ok(())
}

/// Show details for one action from open and completed action stores.
pub fn show_action(ctx: &CommandContext, query: &str, file: &Option<PathBuf>) -> Result<(), String> {
    let acts = collect_all_actions(ctx, file, false)?;
    let act = acts
        .iter()
        .find(|act| act_matches(act, query))
        .ok_or_else(|| format!("No action found matching '{}'", query))?;

    println!("{}", act.name);
    println!("{}", "=".repeat(act.name.len()));
    println!("id:          {}", act.id);
    println!("state:       {:?}", act.state);
    if let Some(alias) = &act.alias {
        println!("alias:       {}", alias);
    }
    if let Some(parent_id) = act.parent_id {
        println!("parent:      {}", parent_id);
    }
    if let Some(dt) = act.scheduled_at {
        println!("scheduled:   {}", dt.format("%Y-%m-%d %H:%M"));
    }
    if let Some(duration) = act.duration {
        println!("duration:    {}m", duration);
    }
    if let Some(priority) = act.priority {
        println!("priority:    {}", priority);
    }
    if let Some(contexts) = &act.contexts {
        println!("contexts:    {}", contexts.join(", "));
    }
    if let Some(created) = act.created_at {
        println!("created:     {}", created.format("%Y-%m-%d %H:%M"));
    }
    if let Some(completed) = act.completed_at {
        println!("completed:   {}", completed.format("%Y-%m-%d %H:%M"));
    }
    if let Some(description) = &act.description {
        println!("description:\n  {}", description.replace('\n', "\n  "));
    }

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
        match resolve_domain_ref(&ctx.data_dir, s)? {
            ResolvedScope::Charter { file_path } | ResolvedScope::Plan { file_path, .. } => {
                vec![file_path]
            }
        }
    } else {
        clearhead_core::list_action_files(&ctx.data_dir)
            .map_err(|e| format!("Failed to list workspace: {}", e))?
    };

    let mut total_archived = 0usize;
    let mut charters_touched = 0usize;

    for actions_path in &charter_paths {
        let open_acts = acts::read_acts(actions_path).map_err(|e| e.to_string())?;

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

            let completed_path = acts::completed_acts_path(actions_path);
            let mut existing_closed =
                acts::read_acts(&completed_path).map_err(|e| e.to_string())?;
            existing_closed.extend(to_close.iter().cloned());
            acts::write_acts(&existing_closed, &completed_path).map_err(|e| e.to_string())?;

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
    query: &str,
) -> Result<(PathBuf, ActionList), String> {
    if let Some(path) = file {
        let acts = super::load_file_for_mutation(path, "action lifecycle")?;
        return Ok((path.clone(), acts));
    }
    find_act_in_open_files(&ctx.data_dir, query)
}

/// Scan `.actions` files in the workspace for one containing an action matching `query`.
fn find_act_in_open_files(data_dir: &Path, query: &str) -> Result<(PathBuf, ActionList), String> {
    let action_files = clearhead_core::list_action_files(data_dir)
        .map_err(|e| format!("Failed to list workspace: {}", e))?;

    for actions_path in action_files {
        let action_list = acts::read_acts(&actions_path).map_err(|e| e.to_string())?;
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

    let tpl_acts = match read_acts(&tpl_path) {
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
    let query_lower = query.to_lowercase();
    let id_str = act.id.to_string();
    let short = &id_str[..8.min(id_str.len())];
    id_str == query
        || short == query
        || act
            .alias
            .as_deref()
            .map(|alias| alias.eq_ignore_ascii_case(query))
            .unwrap_or(false)
        || act.name.to_lowercase().contains(&query_lower)
}

fn find_act_mut<'a>(acts: &'a mut ActionList, query: &str) -> Option<&'a mut Action> {
    acts.iter_mut()
        .find(|a| is_open_act(a) && act_matches(a, query))
}

fn resolve_markdown_charter<'a>(
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

fn collect_all_actions(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    open_only: bool,
) -> Result<Vec<Action>, String> {
    if let Some(path) = file {
        let mut result: Vec<Action> = acts::read_acts(path).map_err(|e| e.to_string())?;
        if open_only {
            result.retain(is_open_act);
        }
        if !open_only {
            let completed_path = acts::completed_acts_path(path);
            result.extend(acts::read_acts(&completed_path).map_err(|e| e.to_string())?);
        }
        return Ok(result);
    }

    let action_files = clearhead_core::list_action_files(&ctx.data_dir)
        .map_err(|e| format!("Failed to list workspace: {}", e))?;

    let mut all = Vec::new();
    for actions_path in action_files {
        let mut open = acts::read_acts(&actions_path).map_err(|e| e.to_string())?;
        if open_only {
            open.retain(is_open_act);
        }
        all.extend(open);
        if !open_only {
            let completed_path = acts::completed_acts_path(&actions_path);
            all.extend(acts::read_acts(&completed_path).map_err(|e| e.to_string())?);
        }
    }
    Ok(all)
}

fn is_open_act(act: &Action) -> bool {
    !matches!(act.state, ActionState::Completed | ActionState::Cancelled)
}

fn print_acts_tree(acts: &[&Action]) {
    let mut by_parent: HashMap<uuid::Uuid, Vec<&Action>> = HashMap::new();
    let mut roots: Vec<&Action> = Vec::new();

    for &act in acts {
        match act.parent_id {
            Some(pid) => by_parent.entry(pid).or_default().push(act),
            None => roots.push(act),
        }
    }

    for (i, root) in roots.iter().enumerate() {
        print_act_node(root, &by_parent, "", true, i == roots.len() - 1);
    }
}

fn print_act_node(
    act: &Action,
    by_parent: &HashMap<uuid::Uuid, Vec<&Action>>,
    prefix: &str,
    is_root: bool,
    is_last: bool,
) {
    let connector = if is_root { "" } else if is_last { "└── " } else { "├── " };
    println!("{}{}{} {}", prefix, connector, state_sigil(&act.state), act.name);

    let child_prefix = if is_root {
        String::new()
    } else {
        format!("{}{}", prefix, if is_last { "    " } else { "│   " })
    };

    if let Some(kids) = by_parent.get(&act.id) {
        for (i, kid) in kids.iter().enumerate() {
            print_act_node(kid, by_parent, &child_prefix, false, i == kids.len() - 1);
        }
    }
}

fn state_sigil(state: &ActionState) -> &'static str {
    match state {
        ActionState::NotStarted => "[ ]",
        ActionState::Completed => "[x]",
        ActionState::InProgress => "[-]",
        ActionState::BlockedOrAwaiting => "[=]",
        ActionState::Cancelled => "[_]",
    }
}

fn print_acts_table(acts: &[&Action]) {
    use comfy_table::{Cell, Table};

    let mut table = Table::new();
    table.set_header(vec!["id", "state", "name", "scheduled_at", "duration"]);

    for act in acts {
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

        table.add_row(vec![
            Cell::new(short_id),
            Cell::new(state),
            Cell::new(&act.name),
            Cell::new(scheduled),
            Cell::new(duration),
        ]);
    }

    println!("{}", table);
}
