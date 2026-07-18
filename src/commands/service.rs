use anyhow::Context;
use tracing::{debug, info, warn};

use crate::commands::{CommandContext, load_file_for_read};
use clearhead_cli::telemetry::{TelemetryEvent, TelemetryRecord, Tool, emit};
use clearhead_core::{Reconcile, SyncEntry};

/// Compatibility shim for the standalone `clearhead-lsp` process.
///
/// stdin/stdout/stderr are inherited so the child speaks LSP directly to the
/// editor. On Unix `exec` replaces the CLI process entirely; other platforms
/// wait for the external server and return its exit status.
pub fn start_lsp() -> anyhow::Result<()> {
    let executable = std::env::var_os("CLEARHEAD_LSP").unwrap_or_else(|| "clearhead-lsp".into());
    info!(server = ?executable, "Delegating to standalone Language Server");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = std::process::Command::new(&executable).exec();
        Err(error).with_context(|| format!("Failed to exec {:?}", executable))
    }

    #[cfg(not(unix))]
    {
        let status = std::process::Command::new(&executable)
            .status()
            .with_context(|| format!("Failed to start {:?}", executable))?;
        if !status.success() {
            anyhow::bail!("clearhead-lsp exited with {status}");
        }
        Ok(())
    }
}

pub fn sync_events(
    ctx: &CommandContext,
    file: &Option<std::path::PathBuf>,
    dry_run: bool,
) -> anyhow::Result<()> {
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

pub fn sync_calendar(
    ctx: &CommandContext,
    dry_run: bool,
    conflict: Option<crate::argparser::ConflictResolutionArg>,
) -> anyhow::Result<()> {
    let plans_root = ctx.plans_root();
    let model = ctx.load_model()?;
    let ics_dates = clearhead_core::read_ics_dates(&plans_root)?;
    let report = clearhead_core::plan_sync(&model, &ics_dates);
    let report = resolve_conflicts(report, conflict);

    if report.is_empty() {
        println!("Already in sync.");
        return Ok(());
    }

    for warning in &report.warnings {
        eprintln!("{}", warning);
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

    let applied =
        clearhead_core::apply_sync(&ctx.data_dir, ctx.plan_override().as_deref(), &report)?;
    info!(?applied, "Calendar sync complete");
    println!(
        "Sync complete. {} push, {} pull, {} converged, {} conflict.",
        applied.take_action, applied.take_calendar, applied.converged, applied.conflict
    );
    Ok(())
}

fn resolve_conflicts(
    mut report: clearhead_core::SyncReport,
    choice: Option<crate::argparser::ConflictResolutionArg>,
) -> clearhead_core::SyncReport {
    let Some(choice) = choice else {
        return report;
    };

    for entry in &mut report.entries {
        if let Reconcile::Conflict { action, calendar } = entry.outcome {
            entry.outcome = match choice {
                crate::argparser::ConflictResolutionArg::Action => Reconcile::TakeAction(action),
                crate::argparser::ConflictResolutionArg::Calendar => {
                    Reconcile::TakeCalendar(calendar)
                }
            };
        }
    }

    report
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
            format!(
                "push action removal → calendar: {} #{}",
                entry.name, entry.action_id
            )
        }
        Reconcile::TakeCalendar(Some(dt)) => format!(
            "pull calendar → action: {} #{} @{}",
            entry.name,
            entry.action_id,
            dt.format("%Y-%m-%dT%H:%M")
        ),
        Reconcile::TakeCalendar(None) => {
            format!(
                "pull calendar removal → action: {} #{}",
                entry.name, entry.action_id
            )
        }
        Reconcile::Converged(Some(dt)) => format!(
            "converged: {} #{} @{}",
            entry.name,
            entry.action_id,
            dt.format("%Y-%m-%dT%H:%M")
        ),
        Reconcile::Converged(None) => {
            format!("converged removal: {} #{}", entry.name, entry.action_id)
        }
        Reconcile::Conflict { action, calendar } => format!(
            "conflict: {} #{} action={:?} calendar={:?}",
            entry.name, entry.action_id, action, calendar
        ),
        Reconcile::NoOp => format!("noop: {} #{}", entry.name, entry.action_id),
    }
}
