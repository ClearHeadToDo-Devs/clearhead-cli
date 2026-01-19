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
use crate::environment_reader::Config;
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
            depth INTEGER NOT NULL DEFAULT 0 CHECK(depth >= 0 AND depth <= 5),
            file_path TEXT,
            project TEXT
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
    conn.execute("CREATE INDEX idx_actions_file_path ON actions(file_path)", [])?;
    conn.execute("CREATE INDEX idx_actions_project ON actions(project)", [])?;

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

    // Tag hierarchies table for transitive inheritance
    // When an action has tag "neovim", queries for "terminal" or "computer" should match
    conn.execute(
        "CREATE TABLE tag_hierarchies (
            parent_tag TEXT NOT NULL,
            child_tag TEXT NOT NULL,
            PRIMARY KEY (parent_tag, child_tag)
        )",
        [],
    )?;

    conn.execute("CREATE INDEX idx_tag_hierarchies_child ON tag_hierarchies(child_tag)", [])?;

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

    // Helper view for actions with expanded contexts (including ancestor tags)
    // This view joins action_contexts with tag_hierarchies to include implicit contexts
    conn.execute(
        "CREATE VIEW actions_with_expanded_contexts AS
        SELECT
            a.*,
            GROUP_CONCAT(DISTINCT ec.context, ',') as contexts
        FROM actions a
        LEFT JOIN (
            -- Direct contexts
            SELECT action_id, context FROM action_contexts
            UNION
            -- Ancestor contexts via hierarchy
            SELECT ac.action_id, th.parent_tag as context
            FROM action_contexts ac
            JOIN tag_hierarchies th ON LOWER(ac.context) = LOWER(th.child_tag)
        ) ec ON a.id = ec.action_id
        GROUP BY a.id",
        [],
    )?;

    // Helper view with "effective_project" - story if explicit, else file-inferred project
    // Per naming_conventions.md: actions inherit project from file structure unless explicitly set
    conn.execute(
        "CREATE VIEW actions_with_effective_project AS
        SELECT
            a.*,
            COALESCE(a.story, a.project) as effective_project
        FROM actions a",
        [],
    )?;

    Ok(())
}

/// Load tag hierarchies from Config into the database
///
/// Inserts all parent-child tag relationships into the tag_hierarchies table.
/// Computes full transitive closure so that if A->B->C, stores (A,B), (A,C), and (B,C).
/// This enables queries like "find all actions with context=terminal" to match
/// actions tagged with "neovim" (since neovim is a child of terminal).
///
/// # Arguments
/// * `conn` - The SQLite connection (must have schema created via `create_database`)
/// * `config` - The configuration containing tag_hierarchies
///
/// # Returns
/// Ok(()) on success, or a SQLite error
pub fn load_tag_hierarchies(conn: &Connection, config: &Config) -> SqlResult<()> {
    use std::collections::{HashMap, HashSet};

    // Build direct parent->children mapping (lowercase)
    let mut direct_children: HashMap<String, Vec<String>> = HashMap::new();
    for (parent, children) in &config.tag_hierarchies {
        let parent_lower = parent.to_lowercase();
        let children_lower: Vec<String> = children.iter().map(|c| c.to_lowercase()).collect();
        direct_children.insert(parent_lower, children_lower);
    }

    // Compute all descendants for each parent (transitive closure)
    fn get_all_descendants(
        tag: &str,
        direct_children: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
    ) -> Vec<String> {
        let mut descendants = Vec::new();
        if let Some(children) = direct_children.get(tag) {
            for child in children {
                if visited.insert(child.clone()) {
                    descendants.push(child.clone());
                    descendants.extend(get_all_descendants(child, direct_children, visited));
                }
            }
        }
        descendants
    }

    // Insert all relationships (direct and transitive)
    let mut all_relationships: HashSet<(String, String)> = HashSet::new();
    for parent in direct_children.keys() {
        let mut visited = HashSet::new();
        let descendants = get_all_descendants(parent, &direct_children, &mut visited);
        for descendant in descendants {
            all_relationships.insert((parent.clone(), descendant));
        }
    }

    for (parent, child) in all_relationships {
        conn.execute(
            "INSERT OR IGNORE INTO tag_hierarchies (parent_tag, child_tag) VALUES (?1, ?2)",
            params![parent, child],
        )?;
    }

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
    load_actions_with_source(conn, actions, None, None)
}

/// Load an ActionList into the database with source metadata
///
/// Like `load_actions` but also stores file_path and project for cross-file queries.
///
/// # Arguments
/// * `conn` - The SQLite connection
/// * `actions` - The flat list of actions to load
/// * `file_path` - Optional source file path
/// * `project` - Optional project name (inferred from file structure)
pub fn load_actions_with_source(
    conn: &Connection,
    actions: &ActionList,
    file_path: Option<&str>,
    project: Option<&str>,
) -> SqlResult<()> {
    let tx = conn.unchecked_transaction()?;

    for action in actions {
        // Insert main action
        tx.execute(
            "INSERT INTO actions (
                id, parent_id, state, name, description, priority, story,
                do_datetime, do_duration, completed_datetime, depth, file_path, project
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
                file_path,
                project,
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
        let priority: Option<u32> = row.get(5)?;
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
            created_date_time: None, // TODO: Update SQL schema to support created_date
            predecessors: None,
            story,
            alias: None, // TODO: Update SQL schema to support alias
            is_sequential: None, // TODO: Update SQL schema to support is_sequential
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

/// Query actions by context with hierarchy expansion
///
/// Finds actions that have the given context tag OR any descendant tag.
/// For example, if "neovim" is a child of "terminal" which is a child of "computer",
/// querying for "computer" will match actions tagged with "neovim".
///
/// # Arguments
/// * `conn` - The SQLite connection (with tag hierarchies loaded)
/// * `context` - The context tag to search for
///
/// # Returns
/// A vector of action ID strings that match
///
/// # Example
/// ```ignore
/// // Config has: computer -> [terminal], terminal -> [neovim]
/// // Action tagged with +neovim
/// let ids = query_actions_by_context(&conn, "computer")?; // matches!
/// ```
pub fn query_actions_by_context(conn: &Connection, context: &str) -> SqlResult<Vec<String>> {
    let context_lower = context.to_lowercase();

    // Query for actions that have this context directly OR have a descendant context
    let sql = "
        SELECT DISTINCT a.id FROM actions a
        JOIN action_contexts ac ON a.id = ac.action_id
        WHERE LOWER(ac.context) = ?1
           OR LOWER(ac.context) IN (
               SELECT child_tag FROM tag_hierarchies WHERE parent_tag = ?1
           )
    ";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![context_lower], |row| row.get::<_, String>(0))?;

    let mut ids = Vec::new();
    for id_result in rows {
        ids.push(id_result?);
    }

    Ok(ids)
}

/// Query actions by effective project
///
/// Finds actions where the "effective project" matches the given value.
/// Effective project = explicit story if set, else file-inferred project.
///
/// Per naming_conventions.md: actions inherit project from file structure
/// unless explicitly specified via *story.
///
/// # Arguments
/// * `conn` - The SQLite connection
/// * `project` - The project name to search for
///
/// # Returns
/// A vector of action ID strings that match
pub fn query_actions_by_project(conn: &Connection, project: &str) -> SqlResult<Vec<String>> {
    let sql = "
        SELECT id FROM actions_with_effective_project
        WHERE effective_project = ?1
    ";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![project], |row| row.get::<_, String>(0))?;

    let mut ids = Vec::new();
    for id_result in rows {
        ids.push(id_result?);
    }

    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Action;

    #[test]
    fn test_create_database() {
        let conn = create_database().expect("Failed to create database");

        // Verify tables exist (actions, action_contexts, action_recurrence, tag_hierarchies)
        let table_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('actions', 'action_contexts', 'action_recurrence', 'tag_hierarchies')",
                [],
                |row| row.get(0),
            )
            .expect("Failed to count tables");

        assert_eq!(table_count, 4, "Expected 4 tables to be created");

        // Verify views exist
        let view_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='view' AND name IN ('actions_with_contexts', 'actions_with_expanded_contexts', 'actions_with_effective_project')",
                [],
                |row| row.get(0),
            )
            .expect("Failed to count views");

        assert_eq!(view_count, 3, "Expected 3 views to be created");
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
            created_date_time: None,
            predecessors: None,
            story: Some("Test Story".to_string()),
            alias: None,
            is_sequential: None,
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
            created_date_time: None,
            predecessors: None,
            story: Some("Epic Story".to_string()),
            alias: None,
            is_sequential: None,
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
            created_date_time: None,
            predecessors: None,
            story: None,
            alias: None,
            is_sequential: None,
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
            created_date_time: None,
            predecessors: None,
            story: None,
            alias: None,
            is_sequential: None,
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
            created_date_time: None,
            predecessors: None,
            story: None,
            alias: None,
            is_sequential: None,
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
            created_date_time: None,
            predecessors: None,
            story: None,
            alias: None,
            is_sequential: None,
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

    #[test]
    fn test_tag_hierarchy_expansion() {
        use crate::entities::ActionState;
        use std::collections::HashMap;
        use uuid::Uuid;

        let conn = create_database().expect("Failed to create database");

        // Create tag hierarchies: computer -> terminal -> neovim
        let mut tag_hierarchies = HashMap::new();
        tag_hierarchies.insert("computer".to_string(), vec!["terminal".to_string(), "browser".to_string()]);
        tag_hierarchies.insert("terminal".to_string(), vec!["neovim".to_string(), "tmux".to_string()]);

        let config = Config {
            data_dir: String::new(),
            config_dir: String::new(),
            default_file: String::new(),
            tag_hierarchies,
            cli_format: String::new(),
            cli_indent_style: String::new(),
            cli_indent_width: 4,
        };

        load_tag_hierarchies(&conn, &config).expect("Failed to load hierarchies");

        // Verify transitive closure was computed
        // computer should have: terminal, browser, neovim, tmux (all descendants)
        let descendant_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM tag_hierarchies WHERE parent_tag = 'computer'",
                [],
                |row| row.get(0),
            )
            .expect("Failed to count");
        assert_eq!(descendant_count, 4, "computer should have 4 descendants");

        // Create an action tagged with neovim
        let mut actions = ActionList::new();
        actions.push(Action {
            id: Uuid::new_v4(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Edit config".to_string(),
            description: None,
            priority: None,
            context_list: Some(vec!["neovim".to_string()]),
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            created_date_time: None,
            predecessors: None,
            story: None,
            alias: None,
            is_sequential: None,
        });

        load_actions(&conn, &actions).expect("Failed to load actions");

        // Query by "computer" should find the action tagged with "neovim"
        let ids = query_actions_by_context(&conn, "computer").expect("Failed to query");
        assert_eq!(ids.len(), 1, "Should find action tagged with neovim when querying for computer");

        // Query by "terminal" should also find it
        let ids = query_actions_by_context(&conn, "terminal").expect("Failed to query");
        assert_eq!(ids.len(), 1, "Should find action tagged with neovim when querying for terminal");

        // Query by "neovim" directly should find it
        let ids = query_actions_by_context(&conn, "neovim").expect("Failed to query");
        assert_eq!(ids.len(), 1, "Should find action tagged with neovim when querying directly");

        // Query by "browser" should NOT find it (different branch)
        let ids = query_actions_by_context(&conn, "browser").expect("Failed to query");
        assert_eq!(ids.len(), 0, "Should NOT find action when querying unrelated tag");
    }

    #[test]
    fn test_effective_project() {
        use crate::entities::ActionState;
        use uuid::Uuid;

        let conn = create_database().expect("Failed to create database");

        // Create actions with different project/story combinations
        let mut actions = ActionList::new();

        // Action with explicit story (should take precedence)
        let action1_id = Uuid::new_v4();
        actions.push(Action {
            id: action1_id,
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Explicit story task".to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            created_date_time: None,
            predecessors: None,
            story: Some("my-explicit-story".to_string()),
            alias: None,
            is_sequential: None,
        });

        // Action without explicit story (should use file-inferred project)
        let action2_id = Uuid::new_v4();
        actions.push(Action {
            id: action2_id,
            parent_id: None,
            state: ActionState::NotStarted,
            name: "No story task".to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            created_date_time: None,
            predecessors: None,
            story: None,
            alias: None,
            is_sequential: None,
        });

        // Load with file-inferred project "work"
        load_actions_with_source(&conn, &actions, Some("work.actions"), Some("work"))
            .expect("Failed to load actions");

        // Query by explicit story
        let ids = query_actions_by_project(&conn, "my-explicit-story").expect("Failed to query");
        assert_eq!(ids.len(), 1, "Should find action with explicit story");
        assert_eq!(ids[0], action1_id.to_string());

        // Query by file-inferred project
        let ids = query_actions_by_project(&conn, "work").expect("Failed to query");
        assert_eq!(ids.len(), 1, "Should find action with file-inferred project");
        assert_eq!(ids[0], action2_id.to_string());

        // Verify view shows correct effective_project
        let effective_projects: Vec<(String, Option<String>)> = conn
            .prepare("SELECT id, effective_project FROM actions_with_effective_project ORDER BY name")
            .unwrap()
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert_eq!(effective_projects.len(), 2);

        // "Explicit story task" should have effective_project = "my-explicit-story"
        let explicit = effective_projects.iter().find(|(id, _)| *id == action1_id.to_string()).unwrap();
        assert_eq!(explicit.1, Some("my-explicit-story".to_string()));

        // "No story task" should have effective_project = "work" (from file)
        let inferred = effective_projects.iter().find(|(id, _)| *id == action2_id.to_string()).unwrap();
        assert_eq!(inferred.1, Some("work".to_string()));
    }
}
