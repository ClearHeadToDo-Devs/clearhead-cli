//! SQL module for storing and querying actions in SQLite
//!
//! This module implements the canonical SQL schema from the
//! [actions specification](https://github.com/ClearHeadToDo-Devs/tree-sitter-actions/blob/main/docs/action_specification.md#sql-storage-schema).
//!
//! # Architecture
//!
//! Actions are loaded into an in-memory SQLite database with the following structure:
//! - `actions` table: Core action data with flat structure (adjacency list via parent_id)
//! - `action_contexts` table: Normalized many-to-many relationship for context tags
//! - `action_recurrence` table: Recurrence rules (future use)
//!
//! # Usage
//!
//! ```ignore
//! use clearhead_cli::sql;
//!
//! // Create database and load actions
//! let conn = sql::create_database()?;
//! sql::load_actions(&conn, &actions)?;
//!
//! // Query for action IDs
//! let ids = sql::query_actions(&conn, "SELECT id FROM actions WHERE priority = 1")?;
//!
//! // Or get full Action structs for filtering/export
//! let filtered = sql::get_actions_from_sql(&conn,
//!     "SELECT * FROM actions WHERE priority = 1")?;
//! let output = clearhead_cli::format(&filtered, clearhead_cli::OutputFormat::Actions)?;
//! ```
//!
//! For CLI usage examples, see `docs/SQL_QUERIES.md`.

use rusqlite::{Connection, Result as SqlResult, params};
use crate::entities::{Action, ActionList, ActionState, Recurrence};
use chrono::{DateTime, Local};
use uuid::Uuid;

/// Create an in-memory SQLite database with the canonical schema
pub fn create_database() -> SqlResult<Connection> {
    let conn = Connection::open_in_memory()?;
    create_schema(&conn)?;
    Ok(conn)
}

/// Create the canonical SQL schema as defined in the specification
fn create_schema(conn: &Connection) -> SqlResult<()> {
    // Main actions table
    conn.execute(
        "CREATE TABLE actions (
            id TEXT PRIMARY KEY,
            parent_id TEXT REFERENCES actions(id) ON DELETE CASCADE,
            state TEXT NOT NULL CHECK(state IN ('not_started', 'completed', 'in_progress', 'blocked', 'cancelled')),
            name TEXT NOT NULL,
            description TEXT,
            priority INTEGER CHECK(priority >= 0),
            story TEXT,
            do_datetime TEXT,
            do_duration INTEGER,
            completed_datetime TEXT,
            depth INTEGER NOT NULL DEFAULT 0 CHECK(depth >= 0 AND depth <= 5)
        )",
        [],
    )?;

    // Indexes for common queries
    conn.execute("CREATE INDEX idx_actions_state ON actions(state)", [])?;
    conn.execute("CREATE INDEX idx_actions_priority ON actions(priority)", [])?;
    conn.execute("CREATE INDEX idx_actions_story ON actions(story)", [])?;
    conn.execute("CREATE INDEX idx_actions_do_datetime ON actions(do_datetime)", [])?;
    conn.execute("CREATE INDEX idx_actions_parent ON actions(parent_id)", [])?;
    conn.execute("CREATE INDEX idx_actions_depth ON actions(depth)", [])?;

    // Normalized contexts table
    conn.execute(
        "CREATE TABLE action_contexts (
            action_id TEXT NOT NULL REFERENCES actions(id) ON DELETE CASCADE,
            context TEXT NOT NULL,
            PRIMARY KEY (action_id, context)
        )",
        [],
    )?;

    conn.execute("CREATE INDEX idx_action_contexts_context ON action_contexts(context)", [])?;

    // Recurrence table per RFC 5545 RRULE syntax
    // Maps from R:FREQ=WEEKLY;BYDAY=MO,WE,FR to normalized fields
    conn.execute(
        "CREATE TABLE action_recurrence (
            action_id TEXT PRIMARY KEY REFERENCES actions(id) ON DELETE CASCADE,
            frequency TEXT NOT NULL CHECK(frequency IN ('secondly', 'minutely', 'hourly', 'daily', 'weekly', 'monthly', 'yearly')),
            interval INTEGER DEFAULT 1 CHECK(interval >= 1),
            count INTEGER CHECK(count >= 1),
            until_date TEXT,
            by_second TEXT,
            by_minute TEXT,
            by_hour TEXT,
            by_day TEXT,
            by_month_day TEXT,
            by_year_day TEXT,
            by_week_no TEXT,
            by_month TEXT,
            by_set_pos TEXT,
            week_start TEXT DEFAULT 'MO' CHECK(week_start IN ('MO', 'TU', 'WE', 'TH', 'FR', 'SA', 'SU'))
        )",
        [],
    )?;

    // Helper view for actions with contexts
    conn.execute(
        "CREATE VIEW actions_with_contexts AS
        SELECT
            a.*,
            GROUP_CONCAT(c.context, ',') as contexts
        FROM actions a
        LEFT JOIN action_contexts c ON a.id = c.action_id
        GROUP BY a.id",
        [],
    )?;

    Ok(())
}

/// Load an ActionList into the database
///
/// Inserts all actions and their contexts into the database.
/// Uses a transaction for atomicity - either all actions load or none do.
///
/// # Arguments
/// * `conn` - The SQLite connection (must have schema created via `create_database`)
/// * `actions` - The flat list of actions to load
///
/// # Returns
/// Ok(()) on success, or a SQLite error
pub fn load_actions(conn: &Connection, actions: &ActionList) -> SqlResult<()> {
    let tx = conn.unchecked_transaction()?;

    for action in actions {
        // Insert main action
        tx.execute(
            "INSERT INTO actions (
                id, parent_id, state, name, description, priority, story,
                do_datetime, do_duration, completed_datetime, depth
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                action.id.to_string(),
                action.parent_id.map(|id| id.to_string()),
                state_to_sql(&action.state),
                action.name,
                action.description,
                action.priority,
                action.story,
                action.do_date_time.map(|dt| dt.to_rfc3339()),
                action.do_duration,
                action.completed_date_time.map(|dt| dt.to_rfc3339()),
                action.depth(actions),
            ],
        )?;

        // Insert contexts
        if let Some(ref contexts) = action.context_list {
            for context in contexts {
                tx.execute(
                    "INSERT INTO action_contexts (action_id, context) VALUES (?1, ?2)",
                    params![action.id.to_string(), context],
                )?;
            }
        }

        // Insert recurrence
        if let Some(ref r) = action.recurrence {
            fn join_vec<T: std::fmt::Display>(v: &[T]) -> String {
                v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
            }

            tx.execute(
                "INSERT INTO action_recurrence (
                    action_id, frequency, interval, count, until_date,
                    by_second, by_minute, by_hour, by_day, by_month_day,
                    by_year_day, by_week_no, by_month, by_set_pos, week_start
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    action.id.to_string(),
                    r.frequency,
                    r.interval,
                    r.count,
                    r.until,
                    r.by_second.as_ref().map(|v| join_vec(v)),
                    r.by_minute.as_ref().map(|v| join_vec(v)),
                    r.by_hour.as_ref().map(|v| join_vec(v)),
                    r.by_day.as_ref().map(|v| v.join(",")),
                    r.by_month_day.as_ref().map(|v| join_vec(v)),
                    r.by_year_day.as_ref().map(|v| join_vec(v)),
                    r.by_week_no.as_ref().map(|v| join_vec(v)),
                    r.by_month.as_ref().map(|v| join_vec(v)),
                    r.by_set_pos.as_ref().map(|v| join_vec(v)),
                    r.week_start,
                ],
            )?;
        }
    }

    tx.commit()?;
    Ok(())
}

/// Convert ActionState to SQL string representation
fn state_to_sql(state: &ActionState) -> &'static str {
    match state {
        ActionState::NotStarted => "not_started",
        ActionState::Completed => "completed",
        ActionState::InProgress => "in_progress",
        ActionState::BlockedorAwaiting => "blocked",
        ActionState::Cancelled => "cancelled",
    }
}

/// Convert SQL string representation back to ActionState
fn state_from_sql(state_str: &str) -> Result<ActionState, String> {
    match state_str {
        "not_started" => Ok(ActionState::NotStarted),
        "completed" => Ok(ActionState::Completed),
        "in_progress" => Ok(ActionState::InProgress),
        "blocked" => Ok(ActionState::BlockedorAwaiting),
        "cancelled" => Ok(ActionState::Cancelled),
        _ => Err(format!("Invalid state string: {}", state_str)),
    }
}

/// Execute a SQL query and return matching action IDs
///
/// Executes arbitrary SQL and extracts the first column as action IDs.
/// The query should SELECT the `id` column (or alias the desired column as the first column).
///
/// # Arguments
/// * `conn` - The SQLite connection
/// * `sql` - The SQL query to execute (must return IDs in first column)
///
/// # Returns
/// A vector of action ID strings, or a SQLite error
///
/// # Example
/// ```ignore
/// let ids = query_actions(&conn, "SELECT id FROM actions WHERE priority = 1")?;
/// ```
pub fn query_actions(conn: &Connection, sql: &str) -> SqlResult<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

    let mut ids = Vec::new();
    for id_result in rows {
        ids.push(id_result?);
    }

    Ok(ids)
}

/// Get full Action structs from a SQL query
///
/// Executes a SQL query and reconstructs complete Action objects.
/// The query should SELECT from the actions table (e.g., `SELECT * FROM actions WHERE ...`).
///
/// # Arguments
/// * `conn` - The SQLite connection
/// * `sql` - The SQL query to execute (should SELECT from actions table)
///
/// # Returns
/// A vector of Action structs with all fields populated, including contexts
///
/// # Example
/// ```ignore
/// let actions = get_actions_from_sql(&conn, "SELECT * FROM actions WHERE priority = 1")?;
/// let formatted = clearhead_cli::format(&actions, OutputFormat::Actions)?;
/// ```
pub fn get_actions_from_sql(conn: &Connection, sql: &str) -> Result<ActionList, String> {
    let mut stmt = conn.prepare(sql)
        .map_err(|e| format!("Failed to prepare query: {}", e))?;

    let action_iter = stmt.query_map([], |row| {
        // Extract all fields from the row
        let id_str: String = row.get(0)?;
        let parent_id_str: Option<String> = row.get(1)?;
        let state_str: String = row.get(2)?;
        let name: String = row.get(3)?;
        let description: Option<String> = row.get(4)?;
        let priority: Option<usize> = row.get(5)?;
        let story: Option<String> = row.get(6)?;
        let do_datetime_str: Option<String> = row.get(7)?;
        let do_duration: Option<i32> = row.get(8)?;
        let completed_datetime_str: Option<String> = row.get(9)?;
        let _depth: i32 = row.get(10)?; // Calculated dynamically, not stored in Action

        Ok((id_str, parent_id_str, state_str, name, description, priority,
            story, do_datetime_str, do_duration, completed_datetime_str))
    }).map_err(|e| format!("Failed to execute query: {}", e))?;

    let mut actions = ActionList::new();

    for row_result in action_iter {
        let (id_str, parent_id_str, state_str, name, description, priority,
             story, do_datetime_str, do_duration_int, completed_datetime_str) =
            row_result.map_err(|e| format!("Failed to read row: {}", e))?;

        // Parse UUID
        let id = Uuid::parse_str(&id_str)
            .map_err(|e| format!("Invalid UUID '{}': {}", id_str, e))?;

        let parent_id = if let Some(pid_str) = parent_id_str {
            Some(Uuid::parse_str(&pid_str)
                .map_err(|e| format!("Invalid parent UUID '{}': {}", pid_str, e))?)
        } else {
            None
        };

        // Parse state
        let state = state_from_sql(&state_str)?;

        // Parse datetimes
        let do_date_time = if let Some(dt_str) = do_datetime_str {
            Some(DateTime::parse_from_rfc3339(&dt_str)
                .map_err(|e| format!("Invalid do_datetime '{}': {}", dt_str, e))?
                .with_timezone(&Local))
        } else {
            None
        };

        let completed_date_time = if let Some(dt_str) = completed_datetime_str {
            Some(DateTime::parse_from_rfc3339(&dt_str)
                .map_err(|e| format!("Invalid completed_datetime '{}': {}", dt_str, e))?
                .with_timezone(&Local))
        } else {
            None
        };
        
        // Convert duration
        let do_duration = do_duration_int.map(|d| d as u32);

        // Query contexts for this action
        let mut context_stmt = conn.prepare("SELECT context FROM action_contexts WHERE action_id = ?1")
            .map_err(|e| format!("Failed to prepare context query: {}", e))?;

        let contexts_iter = context_stmt.query_map(params![id_str], |row| {
            row.get::<_, String>(0)
        }).map_err(|e| format!("Failed to query contexts: {}", e))?;

        let mut contexts = Vec::new();
        for context_result in contexts_iter {
            contexts.push(context_result
                .map_err(|e| format!("Failed to read context: {}", e))?);
        }

        let context_list = if contexts.is_empty() {
            None
        } else {
            Some(contexts)
        };

        // Query recurrence
        let mut recurrence = None;
        let mut r_stmt = conn.prepare(
            "SELECT frequency, interval, count, until_date, by_second, by_minute, by_hour, by_day, by_month_day, by_year_day, by_week_no, by_month, by_set_pos, week_start 
             FROM action_recurrence WHERE action_id = ?1"
        ).map_err(|e| format!("Failed to prepare recurrence query: {}", e))?;
        
        let mut r_rows = r_stmt.query(params![id_str])
            .map_err(|e| format!("Failed to query recurrence: {}", e))?;

        if let Some(row) = r_rows.next().map_err(|e| format!("Failed to read recurrence row: {}", e))? {
             fn parse_vec<T: std::str::FromStr>(s: Option<String>) -> Option<Vec<T>> {
                s.and_then(|str_val| {
                    let v: Vec<T> = str_val.split(',')
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    if v.is_empty() { None } else { Some(v) }
                })
             }
             
             // For strings (BYDAY), we don't need FromStr, just split
             fn parse_string_vec(s: Option<String>) -> Option<Vec<String>> {
                 s.and_then(|str_val| {
                     let v: Vec<String> = str_val.split(',')
                         .map(|x| x.to_string())
                         .collect();
                     if v.is_empty() { None } else { Some(v) }
                 })
             }

             recurrence = Some(Recurrence {
                 frequency: row.get(0).unwrap_or_default(),
                 interval: row.get(1).ok(),
                 count: row.get(2).ok(),
                 until: row.get(3).ok(),
                 by_second: parse_vec(row.get(4).ok()),
                 by_minute: parse_vec(row.get(5).ok()),
                 by_hour: parse_vec(row.get(6).ok()),
                 by_day: parse_string_vec(row.get(7).ok()),
                 by_month_day: parse_vec(row.get(8).ok()),
                 by_year_day: parse_vec(row.get(9).ok()),
                 by_week_no: parse_vec(row.get(10).ok()),
                 by_month: parse_vec(row.get(11).ok()),
                 by_set_pos: parse_vec(row.get(12).ok()),
                 week_start: row.get(13).ok(),
             });
        }

        // Construct the Action
        actions.push(Action {
            id,
            parent_id,
            state,
            name,
            description,
            priority,
            context_list,
            do_date_time,
            do_duration,
            recurrence,
            completed_date_time,
            story,
        });
    }

    Ok(actions)
}

/// Build a WHERE clause query from components
///
/// Constructs a SELECT...FROM...WHERE query from individual parts.
/// Useful for the CLI where users provide just the WHERE clause.
///
/// # Arguments
/// * `where_clause` - The WHERE condition (without "WHERE" keyword)
/// * `select` - Optional SELECT clause (defaults to "id")
/// * `from` - Optional FROM clause (defaults to "actions")
///
/// # Returns
/// A complete SQL query string
///
/// # Example
/// ```
/// # use clearhead_cli::sql::build_where_query;
/// let query = build_where_query("priority = 1", None, None);
/// assert_eq!(query, "SELECT id FROM actions WHERE priority = 1");
///
/// let query = build_where_query("state = 'completed'", Some("name, priority"), Some("actions"));
/// assert_eq!(query, "SELECT name, priority FROM actions WHERE state = 'completed'");
/// ```
pub fn build_where_query(
    where_clause: &str,
    select: Option<&str>,
    from: Option<&str>,
) -> String {
    let select_clause = select.unwrap_or("id");
    let from_clause = from.unwrap_or("actions");

    format!("SELECT {} FROM {} WHERE {}", select_clause, from_clause, where_clause)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Action;

    #[test]
    fn test_create_database() {
        let conn = create_database().expect("Failed to create database");

        // Verify tables exist
        let table_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('actions', 'action_contexts', 'action_recurrence')",
                [],
                |row| row.get(0),
            )
            .expect("Failed to count tables");

        assert_eq!(table_count, 3, "Expected 3 tables to be created");
    }

    #[test]
    fn test_load_and_query_actions() {
        use crate::entities::ActionState;
        use uuid::Uuid;

        let conn = create_database().expect("Failed to create database");

        // Create test actions
        let mut actions = ActionList::new();
        actions.push(Action {
            id: Uuid::new_v4(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Test Action".to_string(),
            description: Some("Test description".to_string()),
            priority: Some(1),
            context_list: Some(vec!["work".to_string()]),
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            story: Some("Test Story".to_string()),
        });

        // Load actions
        load_actions(&conn, &actions).expect("Failed to load actions");

        // Query actions
        let ids = query_actions(&conn, "SELECT id FROM actions WHERE priority = 1")
            .expect("Failed to query actions");

        assert_eq!(ids.len(), 1, "Expected to find 1 action");
    }

    #[test]
    fn test_build_where_query() {
        let query = build_where_query("priority = 1", None, None);
        assert_eq!(query, "SELECT id FROM actions WHERE priority = 1");

        let query = build_where_query("priority = 1", Some("name, priority"), Some("actions"));
        assert_eq!(query, "SELECT name, priority FROM actions WHERE priority = 1");
    }

    #[test]
    fn test_sql_to_ir_roundtrip() {
        use crate::entities::ActionState;
        use uuid::Uuid;

        let conn = create_database().expect("Failed to create database");

        // Create test actions with various fields
        let mut original_actions = ActionList::new();

        let parent_id = Uuid::new_v4();
        let child_id = Uuid::new_v4();

        // Parent action with all fields
        original_actions.push(Action {
            id: parent_id,
            parent_id: None,
            state: ActionState::InProgress,
            name: "Parent Task".to_string(),
            description: Some("This is a parent task".to_string()),
            priority: Some(2),
            context_list: Some(vec!["work".to_string(), "urgent".to_string()]),
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            story: Some("Epic Story".to_string()),
        });

        // Child action
        original_actions.push(Action {
            id: child_id,
            parent_id: Some(parent_id),
            state: ActionState::NotStarted,
            name: "Child Task".to_string(),
            description: None,
            priority: Some(1),
            context_list: Some(vec!["work".to_string()]),
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            story: None,
        });

        // Load into SQL
        load_actions(&conn, &original_actions).expect("Failed to load actions");

        // Retrieve back from SQL
        let retrieved_actions = get_actions_from_sql(&conn, "SELECT * FROM actions")
            .expect("Failed to get actions from SQL");

        // Verify we got the same number of actions
        assert_eq!(retrieved_actions.len(), original_actions.len());

        // Find parent and child by ID (order may vary)
        let parent = retrieved_actions.iter().find(|a| a.id == parent_id).expect("Parent not found");
        let child = retrieved_actions.iter().find(|a| a.id == child_id).expect("Child not found");

        // Verify parent action
        assert_eq!(parent.id, parent_id);
        assert_eq!(parent.parent_id, None);
        assert_eq!(parent.state, ActionState::InProgress);
        assert_eq!(parent.name, "Parent Task");
        assert_eq!(parent.description, Some("This is a parent task".to_string()));
        assert_eq!(parent.priority, Some(2));
        assert_eq!(parent.story, Some("Epic Story".to_string()));

        // Verify contexts (may be in different order)
        let contexts = parent.context_list.as_ref().unwrap();
        assert_eq!(contexts.len(), 2);
        assert!(contexts.contains(&"work".to_string()));
        assert!(contexts.contains(&"urgent".to_string()));

        // Verify child action
        assert_eq!(child.id, child_id);
        assert_eq!(child.parent_id, Some(parent_id));
        assert_eq!(child.state, ActionState::NotStarted);
        assert_eq!(child.name, "Child Task");
        assert_eq!(child.description, None);
        assert_eq!(child.priority, Some(1));
        assert_eq!(child.context_list, Some(vec!["work".to_string()]));
    }

    #[test]
    fn test_get_actions_with_filter() {
        use crate::entities::ActionState;
        use uuid::Uuid;

        let conn = create_database().expect("Failed to create database");

        let mut actions = ActionList::new();

        // Add multiple actions with different priorities
        actions.push(Action {
            id: Uuid::new_v4(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "High Priority".to_string(),
            description: None,
            priority: Some(1),
            context_list: None,
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            story: None,
        });

        actions.push(Action {
            id: Uuid::new_v4(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Low Priority".to_string(),
            description: None,
            priority: Some(3),
            context_list: None,
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            story: None,
        });

        load_actions(&conn, &actions).expect("Failed to load actions");

        // Query only high priority actions
        let high_priority = get_actions_from_sql(&conn, "SELECT * FROM actions WHERE priority = 1")
            .expect("Failed to query");

        assert_eq!(high_priority.len(), 1);
        assert_eq!(high_priority[0].name, "High Priority");
    }

    #[test]
    fn test_recurrence_and_duration_persistence() {
        use crate::entities::{ActionState, Recurrence};
        use uuid::Uuid;

        let conn = create_database().expect("Failed to create database");
        let mut actions = ActionList::new();
        
        let recurrence = Recurrence {
            frequency: "weekly".to_string(),
            interval: Some(2),
            count: None,
            until: None,
            by_second: None,
            by_minute: None,
            by_hour: None,
            by_day: Some(vec!["MO".to_string(), "FR".to_string()]),
            by_month_day: None,
            by_year_day: None,
            by_week_no: None,
            by_month: None,
            by_set_pos: None,
            week_start: Some("MO".to_string()),
        };

        actions.push(Action {
            id: Uuid::new_v4(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Recurring Task".to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: None,
            do_duration: Some(60),
            recurrence: Some(recurrence.clone()),
            completed_date_time: None,
            story: None,
        });

        load_actions(&conn, &actions).expect("Failed to load actions");

        let retrieved = get_actions_from_sql(&conn, "SELECT * FROM actions").expect("Failed to query");
        
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].do_duration, Some(60));
        assert!(retrieved[0].recurrence.is_some());
        
        let ret_recurrence = retrieved[0].recurrence.as_ref().unwrap();
        assert_eq!(ret_recurrence.frequency, "weekly");
        assert_eq!(ret_recurrence.interval, Some(2));
        assert_eq!(ret_recurrence.by_day, Some(vec!["MO".to_string(), "FR".to_string()]));
    }
}
