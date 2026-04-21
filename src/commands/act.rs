//! Handlers for act commands (expand, complete, cancel, update, read, archive).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::Local;
use tracing::{info, warn};

use clearhead_core::workspace::acts;
use clearhead_core::{Action, ActionList, ActionState};

use super::CommandContext;

// ============================================================================
// expand acts — ICS schedule → .actions file
// ============================================================================

/// Expand ICS schedule VEVENTs into planned acts in the charter's `.actions` file.
///
/// Acts are written to `<charter>.actions`. Expansion is idempotent: act UUIDs
/// are derived from `(VEVENT.UID, occurrence_rfc3339)` so re-running never
/// creates duplicates. Only occurrences within `now..now+days` are generated.
pub fn expand_acts(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    days: u32,
    dry_run: bool,
) -> Result<(), String> {
    use chrono::Duration;
    use clearhead_core::workspace::ics::{occurrence_act_id, parse_ics_file};
    use clearhead_core::workspace::plans::collect_plan_files;

    let data_root = clearhead_core::workspace_data_root(&ctx.data_dir);
    let now = Local::now();
    let horizon = now + Duration::days(days as i64);

    let all_entries = collect_plan_files(&data_root)
        .map_err(|e| format!("Failed to discover ICS files: {}", e))?;

    let entries: Vec<_> = if let Some(actions_path) = file {
        let relative = actions_path
            .strip_prefix(&data_root)
            .unwrap_or(actions_path.as_path());
        let charter_name = clearhead_core::infer_charter_name(relative)
            .ok_or_else(|| format!("Cannot infer charter name from '{}'", actions_path.display()))?;
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

    let mut total_added = 0usize;
    let mut charters_touched = 0usize;
    let mut parse_failures: Vec<PathBuf> = Vec::new();

    for entry in &entries {
        let plans = parse_ics_file(&entry.path)
            .map_err(|e| format!("Failed to parse {}: {}", entry.path.display(), e))?;

        if plans.is_empty() {
            continue;
        }

        let actions_path = entry.path.with_extension("actions");

        let mut action_list = match super::load_file_for_mutation(&actions_path, "expand acts") {
            Ok(actions) => actions,
            Err(err) => {
                warn!(path = %actions_path.display(), error = %err, "Skipping charter due to parse issues");
                parse_failures.push(actions_path.clone());
                continue;
            }
        };
        let existing_ids: HashSet<uuid::Uuid> = action_list.iter().map(|a| a.id).collect();

        let mut new_count = 0usize;

        for plan in &plans {
            let vevent_uid = match &plan.external_id {
                Some(uid) => uid.as_str(),
                None => continue,
            };
            let Some(dtstart) = plan.dtstart else { continue };

            if plan.recurrence.is_some() {
                let occurrences = plan.expand_occurrences(dtstart, 1000);
                for occ in occurrences {
                    let occ_local = occ.with_timezone(&Local);
                    if occ_local > horizon {
                        break;
                    }
                    if occ_local < now {
                        continue;
                    }
                    let occ_key = occ_local.to_rfc3339();
                    let act_id = occurrence_act_id(vevent_uid, &occ_key);
                    if existing_ids.contains(&act_id) {
                        continue;
                    }
                    action_list.push(Action {
                        id: act_id,
                        state: ActionState::NotStarted,
                        name: plan.name.clone(),
                        do_date_time: Some(occ_local),
                        created_date_time: Some(now),
                        ..Default::default()
                    });
                    new_count += 1;
                }
            } else if dtstart >= now && dtstart <= horizon {
                let occ_key = dtstart.to_rfc3339();
                let act_id = occurrence_act_id(vevent_uid, &occ_key);
                if !existing_ids.contains(&act_id) {
                    action_list.push(Action {
                        id: act_id,
                        state: ActionState::NotStarted,
                        name: plan.name.clone(),
                        do_date_time: Some(dtstart),
                        created_date_time: Some(now),
                        ..Default::default()
                    });
                    new_count += 1;
                }
            }
        }

        if new_count == 0 {
            continue;
        }

        if dry_run {
            println!("Would add {} act(s) to {}", new_count, actions_path.display());
        } else {
            super::save_file(&actions_path, &action_list)?;
            info!(count = new_count, path = %actions_path.display(), "Acts expanded");
            println!("Added {} act(s) to {}", new_count, actions_path.display());
            total_added += new_count;
            charters_touched += 1;
        }
    }

    if total_added == 0 && !dry_run {
        println!("Nothing to expand.");
    } else if charters_touched > 1 {
        println!("Expanded {} act(s) across {} charter(s).", total_added, charters_touched);
    }

    if !parse_failures.is_empty() {
        eprintln!(
            "expand acts failed for {} charter file(s) due to parse errors",
            parse_failures.len()
        );
        for path in &parse_failures {
            eprintln!("  - {}", path.display());
        }
        return Err(format!(
            "expand acts skipped {} file(s) due to parse errors",
            parse_failures.len()
        ));
    }

    Ok(())
}

// ============================================================================
// Act lifecycle — complete, cancel, update
// ============================================================================

/// Mark an open act as completed (moves to `.completed.actions`).
pub fn complete_act(
    ctx: &CommandContext,
    query: &str,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, query)?;

    let (act_id, completed_act) = {
        let act = find_act_mut(&mut open_acts, query)
            .ok_or_else(|| format!("No open act found matching '{}'", query))?;
        let id = act.id;
        if !dry_run {
            act.state = ActionState::Completed;
            act.completed_date_time = Some(Local::now());
        }
        (id, act.clone())
    };

    if dry_run {
        println!("Would complete act {}", &act_id.to_string()[..8]);
        return Ok(());
    }

    open_acts.retain(|a| a.id != act_id);
    super::save_file(&actions_path, &open_acts)?;

    let completed_path = acts::completed_acts_path(&actions_path);
    let mut closed = acts::read_acts(&completed_path).map_err(|e| e.to_string())?;
    closed.push(completed_act);
    acts::write_acts(&closed, &completed_path).map_err(|e| e.to_string())?;

    info!(%act_id, "Act marked completed");
    println!("Completed act {}", act_id);
    Ok(())
}

/// Update an open act's scheduled time and/or duration.
pub fn update_act(
    ctx: &CommandContext,
    query: &str,
    scheduled_at: &Option<String>,
    duration: &Option<u32>,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, query)?;

    let new_scheduled = if let Some(dt_str) = scheduled_at {
        Some(
            chrono::DateTime::parse_from_rfc3339(dt_str)
                .map(|dt| dt.with_timezone(&Local))
                .map_err(|e| format!("Invalid --scheduled-at '{}': {}", dt_str, e))?,
        )
    } else {
        None
    };

    let act_id = {
        let act = find_act_mut(&mut open_acts, query)
            .ok_or_else(|| format!("No open act found matching '{}'", query))?;
        let id = act.id;
        if !dry_run {
            if let Some(dt) = new_scheduled {
                act.do_date_time = Some(dt);
            }
            if let Some(dur) = duration {
                act.do_duration = Some(*dur);
            }
        }
        id
    };

    if dry_run {
        println!("Would update act {}", &act_id.to_string()[..8]);
        return Ok(());
    }

    super::save_file(&actions_path, &open_acts)?;
    info!(%act_id, "Act updated");
    println!("Updated act {}", act_id);
    Ok(())
}

/// Cancel an open act (moves to `.completed.actions` with Cancelled state).
pub fn cancel_act(
    ctx: &CommandContext,
    query: &str,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (actions_path, mut open_acts) = find_and_load_open_acts(ctx, file, query)?;

    let (act_id, cancelled_act) = {
        let act = find_act_mut(&mut open_acts, query)
            .ok_or_else(|| format!("No open act found matching '{}'", query))?;
        let id = act.id;
        if !dry_run {
            act.state = ActionState::Cancelled;
        }
        (id, act.clone())
    };

    if dry_run {
        println!("Would cancel act {}", &act_id.to_string()[..8]);
        return Ok(());
    }

    open_acts.retain(|a| a.id != act_id);
    super::save_file(&actions_path, &open_acts)?;

    let completed_path = acts::completed_acts_path(&actions_path);
    let mut closed = acts::read_acts(&completed_path).map_err(|e| e.to_string())?;
    closed.push(cancelled_act);
    acts::write_acts(&closed, &completed_path).map_err(|e| e.to_string())?;

    info!(%act_id, "Act cancelled");
    println!("Cancelled act {}", act_id);
    Ok(())
}

// ============================================================================
// read acts
// ============================================================================

/// List acts, optionally filtered by plan name and formatted.
pub fn read_acts_cmd(
    ctx: &CommandContext,
    format: Option<crate::argparser::ActFormat>,
    plan_filter: Option<&str>,
    open_only: bool,
    file: &Option<PathBuf>,
) -> Result<(), String> {
    let acts = collect_all_acts(ctx, file, open_only)?;

    let acts: Vec<&Action> = acts
        .iter()
        .filter(|a| {
            if let Some(filter) = plan_filter {
                let id_str = a.id.to_string();
                let short = &id_str[..8.min(id_str.len())];
                id_str == filter || short == filter || a.name.contains(filter)
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
// archive acts
// ============================================================================

/// Sweep completed/cancelled acts from `.actions` into `.completed.actions`.
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
                "Would archive {} act(s) from {}",
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
                "Acts archived"
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

fn find_and_load_open_acts(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    query: &str,
) -> Result<(PathBuf, ActionList), String> {
    if let Some(path) = file {
        let acts = super::load_file_for_mutation(path, "act lifecycle")?;
        return Ok((path.clone(), acts));
    }
    find_act_in_open_files(&ctx.data_dir, query)
}

/// Scan `.actions` files in the workspace for one containing an act matching `query`.
fn find_act_in_open_files(
    data_dir: &Path,
    query: &str,
) -> Result<(PathBuf, ActionList), String> {
    let action_files = clearhead_core::list_action_files(data_dir)
        .map_err(|e| format!("Failed to list workspace: {}", e))?;

    for actions_path in action_files {
        let action_list = acts::read_acts(&actions_path).map_err(|e| e.to_string())?;
        if action_list.iter().any(|a| act_matches(a, query)) {
            return Ok((actions_path, action_list));
        }
    }

    Err(format!("No open act found matching '{}'", query))
}

fn act_matches(act: &Action, query: &str) -> bool {
    let id_str = act.id.to_string();
    let short = &id_str[..8.min(id_str.len())];
    id_str == query || short == query
}

fn find_act_mut<'a>(acts: &'a mut ActionList, query: &str) -> Option<&'a mut Action> {
    acts.iter_mut().find(|a| act_matches(a, query))
}

fn collect_all_acts(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    open_only: bool,
) -> Result<Vec<Action>, String> {
    if let Some(path) = file {
        let mut result: Vec<Action> = acts::read_acts(path).map_err(|e| e.to_string())?;
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
        all.extend(acts::read_acts(&actions_path).map_err(|e| e.to_string())?);
        if !open_only {
            let completed_path = acts::completed_acts_path(&actions_path);
            all.extend(acts::read_acts(&completed_path).map_err(|e| e.to_string())?);
        }
    }
    Ok(all)
}

fn print_acts_table(acts: &[&Action]) {
    use comfy_table::{Cell, Table};

    let mut table = Table::new();
    table.set_header(vec!["id", "state", "name", "scheduled_at", "duration"]);

    for act in acts {
        let short_id = &act.id.to_string()[..8];
        let state = format!("{:?}", act.state);
        let scheduled = act
            .do_date_time
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "—".to_string());
        let duration = act
            .do_duration
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
