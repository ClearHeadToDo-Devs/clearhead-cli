use serde_json::{Map, Value};
use tree_sitter::Tree;

pub mod treesitter;

pub mod entities;
use entities::ActionList;

pub mod format;
pub use format::{format, OutputFormat};

pub mod sql;

/// Merge two JSON hashmaps (right overwrites left on key conflicts)
///
/// This is a simple shallow merge utility for combining configuration
/// or options from multiple sources.
///
/// # Arguments
/// * `left` - The base hashmap
/// * `right` - The hashmap to merge in (takes precedence)
///
/// # Returns
/// A merged JSON Value object
///
/// # Example
/// ```ignore
/// let base = json!({"a": 1, "b": 2});
/// let override = json!({"b": 3, "c": 4});
/// let merged = merge_hashmaps(&base.as_object().unwrap(), &override.as_object().unwrap());
/// // Result: {"a": 1, "b": 3, "c": 4}
/// ```
pub fn merge_hashmaps(
    left: &Map<String, Value>,
    right: &Map<String, Value>,
) -> Result<Value, String> {
    let mut merged = left.clone();
    for (key, value) in right {
        merged.insert(key.clone(), value.clone());
    }
    Ok(Value::Object(merged))
}

/// Parse a .actions file into a structured ActionList
///
/// # Arguments
/// * `_opts` - Configuration options (currently unused)
/// * `actions` - The .actions file content as a string
///
/// # Returns
/// A `Vec<Action>` representing the flat list of parsed actions
pub fn get_action_list_struct(_opts: &Value, actions: &str) -> Result<ActionList, String> {
    let tree = get_action_list_tree(actions)?;
    let tree_wrapper = treesitter::TreeWrapper {
        tree,
        source: actions.to_string(),
    };
    let action_list: ActionList = tree_wrapper.try_into().map_err(|e: &str| e.to_string())?;
    Ok(action_list)
}

/// Parse a .actions file and return as JSON Value
///
/// This is a convenience wrapper around get_action_list_struct that
/// serializes the result to JSON for data-centric workflows.
///
/// # Arguments
/// * `opts` - Configuration options (passed through)
/// * `actions` - The .actions file content as a string
///
/// # Returns
/// A JSON Value containing the serialized ActionList
pub fn get_action_list(opts: &Value, actions: String) -> Result<Value, String> {
    let action_list = get_action_list_struct(opts, &actions)?;
    serde_json::to_value(&action_list)
        .map_err(|e| format!("Failed to serialize actions to JSON: {}", e))
}

/// Parse a .actions file into a tree-sitter Tree
///
/// This is a low-level function that returns the raw tree-sitter parse tree.
/// Most users should use get_action_list_struct() instead.
///
/// # Arguments
/// * `actions` - The .actions file content as a string
///
/// # Returns
/// A tree-sitter Tree representing the parsed structure
pub fn get_action_list_tree(actions: &str) -> Result<Tree, String> {
    let mut action_parser = tree_sitter::Parser::new();

    action_parser
        .set_language(&tree_sitter_actions::LANGUAGE.into())
        .expect("Failed to set language for tree-sitter parser");

    action_parser
        .parse(actions, None)
        .ok_or("Failed to parse tree".to_string())
}

/// Patch a primary ActionList with updates from a secondary list

/// Patch a primary ActionList with updates from a secondary list
///
/// Matches actions by UUID. If an action exists in both, it is updated
/// in the primary list. If it only exists in the secondary list, it is
/// appended to the primary list.
///
/// # Arguments
/// * `primary` - The source of truth list to be modified
/// * `secondary` - The list containing updates/additions
pub fn patch_action_list(primary: &mut ActionList, secondary: &ActionList) {
    for patch_action in secondary {
        if let Some(original_action) = primary.iter_mut().find(|a| a.id == patch_action.id) {
            // Update existing action
            *original_action = patch_action.clone();
        } else {
            // Add new action
            primary.push(patch_action.clone());
        }
    }
}

/// Filter actions using a SQL query
///
/// # Arguments
/// * `actions` - The full ActionList to query
/// * `sql_query` - The SQL query to execute
///
/// # Returns
/// A filtered ActionList containing only actions that match the query
pub fn run_sql_query(actions: &ActionList, sql_query: &str) -> Result<ActionList, String> {
    use std::collections::HashSet;

    // Create in-memory database and load actions
    let conn = sql::create_database()
        .map_err(|e| format!("Failed to create database: {}", e))?;

    sql::load_actions(&conn, actions)
        .map_err(|e| format!("Failed to load actions into database: {}", e))?;

    // Execute query and get matching IDs
    let matching_ids = sql::query_actions(&conn, sql_query)
        .map_err(|e| format!("SQL query failed: {}", e))?;

    // Convert to HashSet for fast lookup
    let id_set: HashSet<String> = matching_ids.into_iter().collect();

    // Filter original actions by matching IDs
    let filtered = actions
        .iter()
        .filter(|action| id_set.contains(&action.id.to_string()))
        .cloned()
        .collect();

    Ok(filtered)
}

/// Build and execute a SQL WHERE clause query
///
/// # Arguments
/// * `actions` - The full ActionList to query
/// * `where_clause` - The WHERE clause (without the WHERE keyword)
/// * `select` - Optional SELECT clause (defaults to "id")
/// * `from` - Optional FROM clause (defaults to "actions")
///
/// # Returns
/// A filtered ActionList containing only actions that match the WHERE clause
pub fn run_sql_where(
    actions: &ActionList,
    where_clause: &str,
    select: Option<&str>,
    from: Option<&str>,
) -> Result<ActionList, String> {
    let query = sql::build_where_query(where_clause, select, from);
    run_sql_query(actions, &query)
}
