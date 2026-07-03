use tracing::{debug, info, warn};

use crate::commands::{CommandContext, load_file_for_read};
use clearhead_cli::telemetry::{TelemetryEvent, TelemetryRecord, Tool, emit};
use clearhead_core::{Reconcile, SyncEntry};

#[cfg(feature = "lsp")]
pub fn start_lsp() -> Result<(), String> {
    info!("Starting Language Server");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to start async runtime: {}", e))?;

    rt.block_on(crate::lsp::start_lsp());
    Ok(())
}

pub fn sync_events(
    ctx: &CommandContext,
    file: &Option<std::path::PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let input_file = ctx.resolve_action_file(file.as_ref());
    debug!(input_file = %input_file.display(), dry_run = dry_run, "Executing Sync Events");

    let actions = load_file_for_read(&input_file, "sync events")?;
    let mut sync_count = 0;
    let skip_count = 0; // TODO: track which events already exist

    for action in &actions {
        let uuid_str = action.id.to_string();

        if dry_run {
            println!("Would sync: {} #{}", action.name, uuid_str);
        } else {
            let timestamp = action
                .created_at
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

    if dry_run {
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

pub fn sync_calendar(ctx: &CommandContext, dry_run: bool) -> Result<(), String> {
    let plans_root = ctx.plans_root();
    let model = ctx.load_model()?;
    let ics_dates = clearhead_core::read_ics_dates(&plans_root).map_err(|e| e.to_string())?;
    let report = clearhead_core::plan_sync(&model, &ics_dates);

    if report.is_empty() {
        println!("Already in sync.");
        return Ok(());
    }

    for entry in &report.entries {
        println!("{}", render_sync_entry(entry));
    }

    let tally = report.tally();
    if dry_run {
        info!(?tally, "Calendar sync dry run complete");
        println!(
            "Dry run complete. {} push, {} pull, {} converged, {} conflict.",
            tally.take_action, tally.take_calendar, tally.converged, tally.conflict
        );
        return Ok(());
    }

    let applied = clearhead_core::apply_sync(&ctx.data_dir, ctx.plan_override().as_deref(), &report)
        .map_err(|e| e.to_string())?;
    info!(?applied, "Calendar sync complete");
    println!(
        "Sync complete. {} push, {} pull, {} converged, {} conflict.",
        applied.take_action, applied.take_calendar, applied.converged, applied.conflict
    );
    Ok(())
}

fn render_sync_entry(entry: &SyncEntry) -> String {
    match entry.outcome {
        Reconcile::TakeAction(Some(dt)) => format!(
            "push action → calendar: {} #{} @{}",
            entry.name,
            entry.action_id,
            dt.format("%Y-%m-%dT%H:%M")
        ),
        Reconcile::TakeAction(None) => {
            format!("push action removal → calendar: {} #{}", entry.name, entry.action_id)
        }
        Reconcile::TakeCalendar(Some(dt)) => format!(
            "pull calendar → action: {} #{} @{}",
            entry.name,
            entry.action_id,
            dt.format("%Y-%m-%dT%H:%M")
        ),
        Reconcile::TakeCalendar(None) => {
            format!("pull calendar removal → action: {} #{}", entry.name, entry.action_id)
        }
        Reconcile::Converged(Some(dt)) => format!(
            "converged: {} #{} @{}",
            entry.name,
            entry.action_id,
            dt.format("%Y-%m-%dT%H:%M")
        ),
        Reconcile::Converged(None) => format!("converged removal: {} #{}", entry.name, entry.action_id),
        Reconcile::Conflict { action, calendar } => format!(
            "conflict: {} #{} action={:?} calendar={:?}",
            entry.name, entry.action_id, action, calendar
        ),
        Reconcile::NoOp => format!("noop: {} #{}", entry.name, entry.action_id),
    }
}
