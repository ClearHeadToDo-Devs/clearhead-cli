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
//! use cliche::sql;
//!
//! // Create database and load actions
//! let conn = sql::create_database()?;
//! sql::load_actions(&conn, &actions)?;
//!
//! // Query actions
//! let ids = sql::query_actions(&conn, "SELECT id FROM actions WHERE priority = 1")?;
//! ```
//!
//! For CLI usage examples, see `docs/SQL_QUERIES.md`.

use rusqlite::{Connection, Result as SqlResult, params};
use crate::entities::{ActionList, ActionState};

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

    // Recurrence table (for future use)
    conn.execute(
        "CREATE TABLE action_recurrence (
            action_id TEXT PRIMARY KEY REFERENCES actions(id) ON DELETE CASCADE,
            frequency TEXT NOT NULL CHECK(frequency IN ('minutely', 'hourly', 'daily', 'weekly', 'monthly', 'yearly')),
            interval INTEGER DEFAULT 1 CHECK(interval >= 1),
            count INTEGER CHECK(count >= 1),
            until_date TEXT,
            by_minute TEXT,
            by_hour TEXT,
            by_day TEXT,
            by_month_day TEXT,
            by_month TEXT
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
                None::<i32>, // duration - not yet implemented in Action struct
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
/// # use cliche::sql::build_where_query;
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
}
