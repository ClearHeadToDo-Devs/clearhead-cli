//! CLI telemetry: NDJSON implementation of `clearhead_core::TelemetryEmitter`.
//!
//! The domain types (`Tool`, `TelemetryEvent`, `TelemetryRecord`) and the
//! `TelemetryEmitter` trait all live in `clearhead_core::telemetry`. This
//! module provides:
//!
//! - `NdjsonEmitter` — writes records to rotating monthly NDJSON files
//! - Module-level `emit` / `emit_event` wrappers for call sites that don't
//!   yet inject an emitter via context (kept for backward compatibility)
//! - Re-exports of all core types so existing `use clearhead_cli::telemetry::*`
//!   imports continue to work without change

// Re-export everything from core so existing call sites need no changes
pub use clearhead_core::telemetry::{
    event_from_field_change, event_from_state_change, noop_emitter, NoopEmitter, TelemetryEmitter,
    TelemetryEvent, TelemetryRecord, Tool,
};

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use tracing::info;

// =============================================================================
// NDJSON file paths
// =============================================================================

/// XDG state directory for ClearHead telemetry files.
pub fn get_telemetry_dir() -> PathBuf {
    let state_dir = dirs::state_dir()
        .or_else(|| dirs::data_local_dir().map(|p| p.join("state")))
        .unwrap_or_else(|| {
            dirs::home_dir()
                .expect("Could not determine home directory")
                .join(".local")
                .join("state")
        });
    state_dir.join("clearhead").join("telemetry")
}

fn get_current_file() -> PathBuf {
    let now = chrono::Utc::now();
    let filename = format!("events-{}.ndjson", now.format("%Y-%m"));
    get_telemetry_dir().join(filename)
}

fn ensure_telemetry_dir() -> Result<(), String> {
    fs::create_dir_all(get_telemetry_dir())
        .map_err(|e| format!("Failed to create telemetry directory: {}", e))
}

// =============================================================================
// NdjsonEmitter
// =============================================================================

/// Writes `TelemetryRecord`s as newline-delimited JSON to monthly rotating files.
///
/// Files land in `get_telemetry_dir()` as `events-YYYY-MM.ndjson`.
pub struct NdjsonEmitter;

impl TelemetryEmitter for NdjsonEmitter {
    fn emit(&self, record: TelemetryRecord) -> Result<(), String> {
        ensure_telemetry_dir()?;

        let json = serde_json::to_string(&record)
            .map_err(|e| format!("Failed to serialize telemetry event: {}", e))?;

        let file_path = get_current_file();
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .map_err(|e| format!("Failed to open telemetry file: {}", e))?;

        writeln!(file, "{}", json)
            .map_err(|e| format!("Failed to write telemetry event: {}", e))?;

        info!(
            event = record.event.name(),
            action_uuid = ?record.action_uuid,
            tool = ?record.tool,
            "telemetry"
        );

        Ok(())
    }
}

// =============================================================================
// Module-level convenience wrappers (backward compatibility)
//
// Call sites that haven't been migrated to injected emitters still work.
// These are thin delegations to NdjsonEmitter.
// =============================================================================

/// Emit a pre-built `TelemetryRecord` via the CLI's NDJSON emitter.
pub fn emit(record: TelemetryRecord) -> Result<(), String> {
    NdjsonEmitter.emit(record)
}

/// Build and emit a record from parts via the CLI's NDJSON emitter.
pub fn emit_event(
    tool: Tool,
    action_uuid: Option<String>,
    event: TelemetryEvent,
) -> Result<(), String> {
    NdjsonEmitter.emit_event(tool, action_uuid, event)
}
