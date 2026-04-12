//! Handlers for PlannedAct commands (expand, complete, cancel, update, read, archive).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use tracing::info;

use clearhead_core::workspace::acts;
use clearhead_core::{ActPhase, PlannedAct};

use super::CommandContext;

// ============================================================================
// Gap 5 — expand acts
// ============================================================================

/// Expand recurring plans into open PlannedAct instances.
///
/// If `file` is given, only that file is processed. Otherwise the whole workspace is scanned.
pub fn expand_acts(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    days: u32,
    dry_run: bool,
) -> Result<(), String> {
    let charter_paths: Vec<PathBuf> = if let Some(f) = file {
        vec![f.clone()]
    } else {
        clearhead_core::list_action_files(&ctx.data_dir)
            .map_err(|e| format!("Failed to list workspace: {}", e))?
    };

    let mut total_written = 0usize;
    let mut charters_touched = 0usize;

    for path in &charter_paths {
        let content = read_actions_content(path)?;
        if content.is_empty() {
            continue;
        }
        let actions = clearhead_cli::parse_actions(&content)?;
        let data_root = clearhead_core::workspace_data_root(&ctx.data_dir);
        let relative = path.strip_prefix(&data_root).unwrap_or(path.as_path());
        let charter_name = clearhead_core::infer_charter_name(relative)
            .unwrap_or_else(|| "unknown".to_string());
        let charter = clearhead_core::workspace::actions::convert::from_actions_with_charter(&actions, charter_name);
        let mut model = clearhead_core::DomainModel { objectives: vec![], charters: vec![charter] };

        // Load existing open acts and merge them in
        let open_path = acts::open_acts_path(path);
        let existing_acts = acts::read_acts(&open_path).map_err(|e| e.to_string())?;
        if !existing_acts.is_empty() {
            acts::merge_acts_into_model(&mut model, existing_acts.clone());
        }

        let existing_acts_ids: HashSet<uuid::Uuid> = existing_acts.iter().map(|a| a.id).collect();

        // Collect plan IDs from this file
        let plan_ids: HashSet<uuid::Uuid> = model.all_plans().iter().map(|p| p.id).collect();

        if plan_ids.is_empty() {
            continue;
        }

        // Expand recurring plans
        model.expand_recurring_plans(days);

        let expanded_acts: Vec<PlannedAct> = model.all_acts().into_iter().cloned().collect();
        let expanded_act_ids: HashSet<uuid::Uuid> = expanded_acts.iter().map(|a| a.id).collect();

        if dry_run {
            println!(
                "Would write {} acts to {}",
                expanded_acts.len(),
                open_path.display()
            );
        } else if existing_acts_ids != expanded_act_ids {
            acts::write_acts_for_plans(&expanded_acts, &plan_ids, &open_path).map_err(|e| e.to_string())?;
            info!(count = expanded_acts.len(), path = %open_path.display(), "Acts written");
            println!(
                "Wrote {} acts to {}",
                expanded_acts.len(),
                open_path.display()
            );
            total_written += expanded_acts.len();
            charters_touched += 1;
        }
    }

    if charters_touched > 1 {
        if dry_run {
            println!("Would expand acts across {} charter(s).", charters_touched);
        } else {
            println!(
                "Expanded {} act(s) across {} charter(s).",
                total_written, charters_touched
            );
        }
    }

    Ok(())
}

// ============================================================================
// Gap 6 — act state management
// ============================================================================

/// Mark a planned act as completed (moves from open to closed file).
pub fn complete_act(
    ctx: &CommandContext,
    query: &str,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, query)?;
    let open_path = acts::open_acts_path(&actions_path);
    let closed_path = acts::closed_acts_path(&actions_path);

    // Scope the mutable borrow to extract the completed act
    let (act_id, completed_act) = {
        let act = find_act_mut(&mut open_acts, query)
            .ok_or_else(|| format!("No open act found matching '{}'", query))?;
        let id = act.id;
        if !dry_run {
            act.phase = ActPhase::Completed;
            act.completed_at = Some(Local::now());
        }
        (id, act.clone())
    };

    if dry_run {
        println!(
            "Would complete act {} ({})",
            &act_id.to_string()[..8],
            act_id
        );
        return Ok(());
    }

    // Remove from open
    open_acts.retain(|a| a.id != act_id);
    acts::write_acts(&open_acts, &open_path).map_err(|e| e.to_string())?;

    // Append to closed
    let mut closed_acts = acts::read_acts(&closed_path).map_err(|e| e.to_string())?;
    closed_acts.push(completed_act);
    acts::write_acts(&closed_acts, &closed_path).map_err(|e| e.to_string())?;

    info!(%act_id, "Act marked completed");
    println!("Completed act {}", act_id);
    Ok(())
}

/// Update a planned act's scheduled time and/or duration (open acts only).
pub fn update_act(
    ctx: &CommandContext,
    query: &str,
    scheduled_at: &Option<String>,
    duration: &Option<u32>,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, query)?;
    let open_path = acts::open_acts_path(&actions_path);

    // Parse the new scheduled_at before mutating (avoids borrow issues)
    let new_scheduled = if let Some(dt_str) = scheduled_at {
        Some(
            chrono::DateTime::parse_from_rfc3339(dt_str)
                .map(|dt| dt.with_timezone(&Local))
                .map_err(|e| format!("Invalid --scheduled-at '{}': {}", dt_str, e))?,
        )
    } else {
        None
    };

    // Scope the mutable borrow
    let act_id = {
        let act = find_act_mut(&mut open_acts, query)
            .ok_or_else(|| format!("No open act found matching '{}'", query))?;
        let id = act.id;
        if !dry_run {
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
        println!("Would update act {}", &act_id.to_string()[..8]);
        return Ok(());
    }

    acts::write_acts(&open_acts, &open_path).map_err(|e| e.to_string())?;
    info!(%act_id, "Act updated");
    println!("Updated act {}", act_id);
    Ok(())
}

/// Cancel a planned act (moves from open to closed file with Cancelled phase).
pub fn cancel_act(
    ctx: &CommandContext,
    query: &str,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, query)?;
    let open_path = acts::open_acts_path(&actions_path);
    let closed_path = acts::closed_acts_path(&actions_path);

    let (act_id, cancelled_act) = {
        let act = find_act_mut(&mut open_acts, query)
            .ok_or_else(|| format!("No open act found matching '{}'", query))?;
        let id = act.id;
        if !dry_run {
            act.phase = ActPhase::Cancelled;
        }
        (id, act.clone())
    };

    if dry_run {
        println!("Would cancel act {} ({})", &act_id.to_string()[..8], act_id);
        return Ok(());
    }

    // Remove from open
    open_acts.retain(|a| a.id != act_id);
    acts::write_acts(&open_acts, &open_path).map_err(|e| e.to_string())?;

    // Append to closed
    let mut closed_acts = acts::read_acts(&closed_path).map_err(|e| e.to_string())?;
    closed_acts.push(cancelled_act);
    acts::write_acts(&closed_acts, &closed_path).map_err(|e| e.to_string())?;

    info!(%act_id, "Act cancelled");
    println!("Cancelled act {}", act_id);
    Ok(())
}

// ============================================================================
// Gap 7 — read acts
// ============================================================================

/// List planned acts, optionally filtered and formatted.
pub fn read_acts_cmd(
    ctx: &CommandContext,
    format: Option<crate::argparser::ActFormat>,
    plan_filter: Option<&str>,
    open_only: bool,
    file: &Option<PathBuf>,
) -> Result<(), String> {
    let acts = collect_all_acts(ctx, file, open_only)?;

    // Filter by plan
    let acts: Vec<&PlannedAct> = acts
        .iter()
        .filter(|a| {
            if let Some(filter) = plan_filter {
                let id_str = a.plan_id.to_string();
                let short = &id_str[..8.min(id_str.len())];
                id_str == filter || short == filter
            } else {
                true
            }
        })
        .collect();

    match format {
        Some(crate::argparser::ActFormat::Json) => {
            let json = serde_json::to_string_pretty(&acts)
                .map_err(|e| format!("JSON serialization failed: {}", e))?;
            println!("{}", json);
        }
        _ => {
            print_acts_table(&acts);
        }
    }

    Ok(())
}

// ============================================================================
// Gap 8 — archive acts
// ============================================================================

/// Move completed/cancelled acts from `<charter>.open.ttl` to `<charter>.closed.ttl`.
///
/// If `file` is given, only that charter is processed. If `scope` is given, resolves a
/// domain reference (e.g. `"health"` or `"health/exercise"`) to a charter file. Otherwise
/// the whole workspace is scanned.
pub fn archive_acts(
    ctx: &CommandContext,
    scope: &Option<String>,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let charter_paths: Vec<PathBuf> = if let Some(f) = file {
        vec![f.clone()]
    } else if let Some(s) = scope {
        use crate::commands::resolver::{ResolvedScope, resolve_domain_ref};
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

    let mut total_archived = 0usize;
    let mut charters_touched = 0usize;

    for actions_path in &charter_paths {
        let open_path = acts::open_acts_path(actions_path);
        let all_open = acts::read_acts(&open_path).map_err(|e| e.to_string())?;

        let (to_close, to_keep): (Vec<PlannedAct>, Vec<PlannedAct>) = all_open
            .into_iter()
            .partition(|a| matches!(a.phase, ActPhase::Completed | ActPhase::Cancelled));

        if to_close.is_empty() {
            continue;
        }

        if dry_run {
            println!(
                "Would archive {} act(s) from {}",
                to_close.len(),
                open_path.display()
            );
        } else {
            acts::write_acts(&to_keep, &open_path).map_err(|e| e.to_string())?;

            let closed_path = acts::closed_acts_path(actions_path);
            let mut existing_closed = acts::read_acts(&closed_path).map_err(|e| e.to_string())?;
            existing_closed.extend(to_close.iter().cloned());
            acts::write_acts(&existing_closed, &closed_path).map_err(|e| e.to_string())?;

            info!(
                count = to_close.len(),
                charter = %open_path.display(),
                "Acts archived to closed file"
            );
        }

        total_archived += to_close.len();
        charters_touched += 1;
    }

    if total_archived == 0 {
        println!("Nothing to archive.");
    } else if dry_run {
        println!(
            "Would archive {} act(s) across {} charter(s).",
            total_archived, charters_touched
        );
    } else {
        println!(
            "Archived {} act(s) across {} charter(s).",
            total_archived, charters_touched
        );
    }

    Ok(())
}

// ============================================================================
// Private helpers
// ============================================================================

fn read_actions_content(path: &Path) -> Result<String, String> {
    if path.exists() {
        fs::read_to_string(path).map_err(|e| format!("Failed to read '{}': {}", path.display(), e))
    } else {
        Ok(String::new())
    }
}

/// Find the actions file for a given query (by scanning open acts), and load open acts.
///
/// Returns `(actions_path, Vec<PlannedAct>)` where the acts are open only.
fn find_and_load_open_acts(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    query: &str,
) -> Result<(PathBuf, Vec<PlannedAct>), String> {
    if let Some(path) = file {
        let open_path = acts::open_acts_path(path);
        let acts = acts::read_acts(&open_path).map_err(|e| e.to_string())?;
        return Ok((path.clone(), acts));
    }

    // Workspace search: scan all open.ttl files
    find_act_in_open_files(&ctx.data_dir, query)
}

/// Scan all `*.open.ttl` files in the workspace for an act matching `query`.
fn find_act_in_open_files(
    data_dir: &Path,
    query: &str,
) -> Result<(PathBuf, Vec<PlannedAct>), String> {
    let action_files = clearhead_core::list_action_files(data_dir)
        .map_err(|e| format!("Failed to list workspace: {}", e))?;

    for actions_path in action_files {
        let open_path = acts::open_acts_path(&actions_path);
        if !open_path.exists() {
            continue;
        }
        let acts = acts::read_acts(&open_path).map_err(|e| e.to_string())?;
        if acts.iter().any(|a| act_matches(a, query)) {
            return Ok((actions_path, acts));
        }
    }

    Err(format!("No open act found matching '{}'", query))
}

fn act_matches(act: &PlannedAct, query: &str) -> bool {
    let id_str = act.id.to_string();
    let short = &id_str[..8.min(id_str.len())];
    id_str == query || short == query
}

fn find_act_mut<'a>(acts: &'a mut Vec<PlannedAct>, query: &str) -> Option<&'a mut PlannedAct> {
    acts.iter_mut().find(|a| act_matches(a, query))
}

/// Collect acts from either a single charter or the whole workspace.
///
/// If `open_only` is true, only open acts are loaded; otherwise open + closed are combined.
fn collect_all_acts(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    open_only: bool,
) -> Result<Vec<PlannedAct>, String> {
    if let Some(path) = file {
        let mut result = acts::read_acts(&acts::open_acts_path(path)).map_err(|e| e.to_string())?;
        if !open_only {
            result.extend(acts::read_acts(&acts::closed_acts_path(path)).map_err(|e| e.to_string())?);
        }
        return Ok(result);
    }

    let action_files = clearhead_core::list_action_files(&ctx.data_dir)
        .map_err(|e| format!("Failed to list workspace: {}", e))?;

    let mut all = Vec::new();
    for actions_path in action_files {
        all.extend(acts::read_acts(&acts::open_acts_path(&actions_path)).map_err(|e| e.to_string())?);
        if !open_only {
            all.extend(acts::read_acts(&acts::closed_acts_path(&actions_path)).map_err(|e| e.to_string())?);
        }
    }
    Ok(all)
}

fn print_acts_table(acts: &[&PlannedAct]) {
    use comfy_table::{Cell, Table};

    let mut table = Table::new();
    table.set_header(vec!["id", "plan_id", "phase", "scheduled_at", "duration"]);

    for act in acts {
        let short_id = &act.id.to_string()[..8];
        let short_plan = &act.plan_id.to_string()[..8];
        let phase = format!("{:?}", act.phase);
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
            Cell::new(short_plan),
            Cell::new(phase),
            Cell::new(scheduled),
            Cell::new(duration),
        ]);
    }

    println!("{}", table);
}
