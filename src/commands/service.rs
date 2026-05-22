use tracing::{debug, info, warn};

use crate::commands::{CommandContext, load_file_for_read};
use clearhead_cli::telemetry::{TelemetryEvent, TelemetryRecord, Tool, emit};

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
