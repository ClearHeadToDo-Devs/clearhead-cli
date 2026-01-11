/// Event logging module for persistent action history tracking.
///
/// This module implements event sourcing for action files:
/// - Events are the source of truth for "what happened when"
/// - Stored in SQLite database at ~/.local/state/clearhead/events.db
/// - Enables time-series analytics and historical queries

use rusqlite::{params, Connection};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};

/// Get the path to the events database.
///
/// Default: $XDG_STATE_HOME/clearhead/events.db (~/.local/state/clearhead/events.db)
/// Can be overridden via config or environment variable.
pub fn events_db_path() -> Result<PathBuf, String> {
    events_db_path_inner(None)
}

/// Internal function to get events DB path with optional override for testing
fn events_db_path_inner(override_path: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(path) = override_path {
        let db_dir = path.parent().ok_or("Invalid override path")?;
        std::fs::create_dir_all(db_dir)
            .map_err(|e| format!("Failed to create events database directory: {}", e))?;
        return Ok(path.to_path_buf());
    }

    // TODO: Read from config/env var
    // For now, use XDG_STATE_HOME default

    let state_dir = if let Ok(xdg_state) = std::env::var("XDG_STATE_HOME") {
        PathBuf::from(xdg_state)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".local").join("state")
    } else {
        return Err("Could not determine state directory (no HOME or XDG_STATE_HOME)".to_string());
    };

    let db_dir = state_dir.join("clearhead");

    // Create directory if it doesn't exist
    std::fs::create_dir_all(&db_dir)
        .map_err(|e| format!("Failed to create events database directory: {}", e))?;

    Ok(db_dir.join("events.db"))
}

/// Initialize the events database with schema.
///
/// Creates the events table with indexes if it doesn't exist.
/// Enables WAL mode for concurrent writes.
pub fn init_events_db() -> Result<Connection, String> {
    init_events_db_inner(None)
}

/// Internal function with optional path override for testing
fn init_events_db_inner(db_path: Option<&Path>) -> Result<Connection, String> {
    let path = events_db_path_inner(db_path)?;
    let conn = Connection::open(&path)
        .map_err(|e| format!("Failed to open events database: {}", e))?;

    // Enable WAL mode for concurrent writes (PRAGMA returns results, so use pragma_update)
    conn.pragma_update(None, "journal_mode", "WAL")
        .map_err(|e| format!("Failed to enable WAL mode: {}", e))?;

    // Create events table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            event_id TEXT UNIQUE NOT NULL,
            timestamp TEXT NOT NULL,
            event_type TEXT NOT NULL,
            action_uuid TEXT NOT NULL,
            file_path TEXT,
            metadata TEXT NOT NULL DEFAULT '{}'
        )",
        [],
    )
    .map_err(|e| format!("Failed to create events table: {}", e))?;

    // Create indexes
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp)",
        [],
    )
    .map_err(|e| format!("Failed to create timestamp index: {}", e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_events_action_uuid ON events(action_uuid)",
        [],
    )
    .map_err(|e| format!("Failed to create action_uuid index: {}", e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type)",
        [],
    )
    .map_err(|e| format!("Failed to create event_type index: {}", e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_events_type_timestamp ON events(event_type, timestamp)",
        [],
    )
    .map_err(|e| format!("Failed to create composite index: {}", e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_events_file_path ON events(file_path)",
        [],
    )
    .map_err(|e| format!("Failed to create file_path index: {}", e))?;

    Ok(conn)
}

/// Emit a single event to the events database.
///
/// # Arguments
/// * `event_type` - Type of event (e.g., "action_completed", "priority_changed")
/// * `action_uuid` - UUID of the action this event relates to
/// * `file_path` - Path to the .actions file (optional)
/// * `metadata` - JSON metadata for this event
pub fn emit_event(
    event_type: &str,
    action_uuid: &str,
    file_path: Option<&str>,
    metadata: JsonValue,
) -> Result<(), String> {
    emit_event_with_db(event_type, action_uuid, file_path, metadata, None)
}

/// Emit a single event to the events database with a custom timestamp.
pub fn emit_event_with_timestamp(
    event_type: &str,
    action_uuid: &str,
    file_path: Option<&str>,
    metadata: JsonValue,
    timestamp: &str,
) -> Result<(), String> {
    emit_event_with_db_timestamp(event_type, action_uuid, file_path, metadata, None, Some(timestamp))
}

/// Internal function with optional DB path override and optional timestamp
pub fn emit_event_with_db(
    event_type: &str,
    action_uuid: &str,
    file_path: Option<&str>,
    metadata: JsonValue,
    db_path: Option<&Path>,
) -> Result<(), String> {
    emit_event_with_db_timestamp(event_type, action_uuid, file_path, metadata, db_path, None)
}

/// Full internal emission function
fn emit_event_with_db_timestamp(
    event_type: &str,
    action_uuid: &str,
    file_path: Option<&str>,
    metadata: JsonValue,
    db_path: Option<&Path>,
    timestamp: Option<&str>,
) -> Result<(), String> {
    let conn = init_events_db_inner(db_path)?;

    // Generate UUIDv7 for event
    let event_id = uuid::Uuid::now_v7().to_string();

    // Use provided timestamp or current time
    let ts = match timestamp {
        Some(t) => t.to_string(),
        None => chrono::Utc::now().to_rfc3339(),
    };

    conn.execute(
        "INSERT INTO events (event_id, timestamp, event_type, action_uuid, file_path, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            event_id,
            ts,
            event_type,
            action_uuid,
            file_path,
            metadata.to_string()
        ],
    )
    .map_err(|e| format!("Failed to insert event: {}", e))?;

    Ok(())
}

/// Check if an action already has any events logged.
pub fn action_has_events(action_uuid: &str) -> Result<bool, String> {
    action_has_events_inner(action_uuid, None)
}

/// Internal function with optional DB path override
fn action_has_events_inner(action_uuid: &str, db_path: Option<&Path>) -> Result<bool, String> {
    let conn = init_events_db_inner(db_path)?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM events WHERE action_uuid = ?1",
            params![action_uuid],
            |row| row.get(0),
        )
        .map_err(|e| format!("Failed to query events: {}", e))?;

    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    /// Create a temp database path for testing
    fn temp_db_path() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_events.db");
        (temp_dir, db_path)
    }

    #[test]
    fn test_init_events_db_creates_schema() {
        let (_temp, db_path) = temp_db_path();
        let conn = init_events_db_inner(Some(&db_path)).unwrap();

        // Check that events table exists
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='events'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(count, 1, "events table should exist");

        // Check that indexes exist
        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND tbl_name='events'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(index_count >= 5, "should have at least 5 indexes");
    }

    #[test]
    fn test_emit_event_inserts_event() {
        let (_temp, db_path) = temp_db_path();

        let metadata = json!({
            "name": "Test action",
            "priority": 1
        });

        emit_event_with_db(
            "action_completed",
            "test-uuid-123",
            Some("inbox.actions"),
            metadata.clone(),
            Some(&db_path),
        )
        .unwrap();

        // Verify event was inserted
        let conn = init_events_db_inner(Some(&db_path)).unwrap();
        let mut stmt = conn
            .prepare("SELECT event_type, action_uuid, file_path, metadata FROM events")
            .unwrap();

        let mut rows = stmt.query([]).unwrap();
        let row = rows.next().unwrap().unwrap();

        assert_eq!(row.get::<_, String>(0).unwrap(), "action_completed");
        assert_eq!(row.get::<_, String>(1).unwrap(), "test-uuid-123");
        assert_eq!(row.get::<_, String>(2).unwrap(), "inbox.actions");

        let stored_metadata: String = row.get(3).unwrap();
        let parsed: JsonValue = serde_json::from_str(&stored_metadata).unwrap();
        assert_eq!(parsed["name"], "Test action");
        assert_eq!(parsed["priority"], 1);
    }

    #[test]
    fn test_emit_event_with_no_file_path() {
        let (_temp, db_path) = temp_db_path();

        let metadata = json!({"test": true});

        emit_event_with_db("action_created", "uuid-456", None, metadata, Some(&db_path)).unwrap();

        let conn = init_events_db_inner(Some(&db_path)).unwrap();
        let file_path: Option<String> = conn
            .query_row(
                "SELECT file_path FROM events WHERE action_uuid = 'uuid-456'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(file_path.is_none(), "file_path should be NULL");
    }

    #[test]
    fn test_multiple_events_sequential_ids() {
        let (_temp, db_path) = temp_db_path();

        emit_event_with_db("action_created", "uuid-1", None, json!({}), Some(&db_path)).unwrap();
        emit_event_with_db("action_completed", "uuid-1", None, json!({}), Some(&db_path)).unwrap();
        emit_event_with_db("action_deleted", "uuid-1", None, json!({}), Some(&db_path)).unwrap();

        let conn = init_events_db_inner(Some(&db_path)).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 3, "should have 3 events");

        // Check IDs are sequential
        let mut stmt = conn.prepare("SELECT id FROM events ORDER BY id").unwrap();
        let ids: Vec<i64> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(ids, vec![1, 2, 3], "IDs should be sequential");
    }

    #[test]
    fn test_action_has_events() {
        let (_temp, db_path) = temp_db_path();
        let uuid = "uuid-789";

        // Initially false
        assert!(!action_has_events_inner(uuid, Some(&db_path)).unwrap());

        // After emitting, true
        emit_event_with_db("action_created", uuid, None, json!({}), Some(&db_path)).unwrap();
        assert!(action_has_events_inner(uuid, Some(&db_path)).unwrap());
    }
}
