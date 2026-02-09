//! ClearHead CLI Library
//!
//! This library provides the CLI and LSP server implementation for the ClearHead framework.
//! It builds on top of clearhead_core, adding filesystem, configuration, and runtime concerns.

use serde_json::{Map, Value};
use tree_sitter::Tree;

// Re-export core library types and functions
pub use clearhead_core::{
    // Domain models
    ActPhase,
    // Core entities
    Action,
    ActionList,
    ActionState,

    // Charter types
    Charter,

    DomainModel,
    FormatConfig,
    FormatStyle,
    IndentStyle,
    LintDiagnostic,
    LintResults,
    LintSeverity,

    OutputFormat,

    ParsedDocument,

    Plan,
    PlannedAct,

    // Charter functions
    format_charter,
    implicit_charter,
    parse_charter,
    // Diff types
    diff as core_diff,

    // Format types and functions
    format,
    // Lint types and functions
    lint_document,
    // Parsing types and functions
    parse_actions,
    parse_document,
    parse_tree,
    // Patch function
    patch_action_list,
    // Sync types
    sync as core_sync,

    // Source metadata (from treesitter)
    treesitter::{SourceMetadata, SourceRange},
};

// CLI-specific modules with environment integration
pub mod crdt;

pub mod document;

pub mod graph;
// Legacy alias for compatibility
pub use graph as sql;

pub mod export;
pub use export::format_as_icalendar;

pub mod archive;

pub mod mutations;
pub use mutations::{ActionUpdate, MatchType, ResolvedAction, apply_updates, resolve_reference};

pub mod workspace;

pub mod environment_reader;
pub use environment_reader::{Config, get_config_dir, get_data_dir, load_config};

pub mod telemetry;
pub use telemetry::{
    TelemetryEvent, TelemetryRecord, Tool, emit, emit_event, event_from_field_change,
    event_from_state_change, get_telemetry_dir,
};

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

// CLI wrappers for backward compatibility

/// Parse a .actions file into a structured ActionList
///
/// # Arguments
/// * `_opts` - Configuration options (currently unused)
/// * `actions` - The .actions file content as a string
///
/// # Returns
/// A `Vec<Action>` representing the flat list of parsed actions
pub fn get_action_list_struct(_opts: &Value, actions: &str) -> Result<ActionList, String> {
    parse_actions(actions)
}

/// Parse a .actions file into a ParsedDocument (Actions + Source Metadata)
///
/// # Arguments
/// * `actions` - The .actions file content as a string
///
/// # Returns
/// A ParsedDocument containing the list of actions and their source locations
pub fn get_parsed_document(actions: &str) -> Result<ParsedDocument, String> {
    parse_document(actions)
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
    parse_tree(actions)
}

/// Filter actions using a SPARQL query
///
/// # Arguments
/// * `actions` - The full ActionList to query
/// * `sparql_query` - The SPARQL query to execute
///
/// # Returns
/// A filtered ActionList containing only actions that match the query
pub fn run_sql_query(actions: &ActionList, sparql_query: &str) -> Result<ActionList, String> {
    use std::collections::HashSet;

    // Create in-memory store and load actions
    let store = sql::create_database().map_err(|e| format!("Failed to create store: {}", e))?;

    sql::load_actions(&store, actions)
        .map_err(|e| format!("Failed to load actions into store: {}", e))?;

    // Execute query and get matching IDs
    let matching_ids = sql::query_actions(&store, sparql_query)
        .map_err(|e| format!("SPARQL query failed: {}", e))?;

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

/// Build and execute a SPARQL query from a WHERE clause (Graph Pattern)
///
/// # Arguments
/// * `actions` - The full ActionList to query
/// * `where_clause` - The WHERE clause (Graph Pattern)
/// * `select` - Unused (SPARQL always selects ?id in this helper)
/// * `from` - Unused
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

/// Run a SPARQL query on workspace actions with source tracking
///
/// This enables cross-file queries using the file_path and project properties.
///
/// # Arguments
/// * `workspace` - Workspace actions with source metadata
/// * `sparql_query` - The SPARQL query to execute
///
/// # Returns
/// A filtered ActionList containing only actions that match the query
pub fn run_workspace_sql_query(
    workspace: &workspace::WorkspaceActions,
    sparql_query: &str,
) -> Result<ActionList, String> {
    use std::collections::HashSet;

    // Create in-memory store
    let store = sql::create_database().map_err(|e| format!("Failed to create store: {}", e))?;

    // Load each file's actions with source metadata
    for sourced in &workspace.sourced_actions {
        let file_path_str = sourced.source.file_path.to_string_lossy();
        sql::load_actions_with_source(
            &store,
            &vec![sourced.action.clone()],
            Some(&file_path_str),
            sourced.source.project.as_deref(),
        )
        .map_err(|e| format!("Failed to load action into store: {}", e))?;
    }

    // Execute query and get matching IDs
    let matching_ids = sql::query_actions(&store, sparql_query)
        .map_err(|e| format!("SPARQL query failed: {}", e))?;

    // Convert to HashSet for fast lookup
    let id_set: HashSet<String> = matching_ids.into_iter().collect();

    // Filter original actions by matching IDs
    let filtered = workspace
        .sourced_actions
        .iter()
        .filter(|sa| id_set.contains(&sa.action.id.to_string()))
        .map(|sa| sa.action.clone())
        .collect();

    Ok(filtered)
}

/// Run a SPARQL WHERE clause query on workspace actions
pub fn run_workspace_sql_where(
    workspace: &workspace::WorkspaceActions,
    where_clause: &str,
    select: Option<&str>,
    from: Option<&str>,
) -> Result<ActionList, String> {
    let query = sql::build_where_query(where_clause, select, from);
    run_workspace_sql_query(workspace, &query)
}
