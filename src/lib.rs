//! ClearHead CLI Library
//!
//! This library provides the CLI and LSP server implementation for the ClearHead framework.
//! It builds on top of clearhead_core, adding filesystem, configuration, and runtime concerns.

use serde_json::{Map, Value};
use tree_sitter::Tree;
use clearhead_core::WorkspaceConfig;

// Re-export core library types and functions
pub use clearhead_core::{
    ActPhase, Action, ActionList, ActionState, Charter, DomainModel, OutputFormat, ParseFailure,
    ParseMode, ParseOutcome, ParsedDocument, Plan, PlannedAct, RecoveryReport, SourceMetadata,
    SourceRange, format, format_charter, implicit_charter, parse_actions, parse_actions_with_mode,
    parse_charter, parse_document, parse_tree, patch_action_list,
};

pub use clearhead_core::format::{FormatConfig, FormatStyle, IndentStyle};
pub use clearhead_core::workspace::actions::TableFormatOptions;

pub use clearhead_core::workspace::actions::{
    LintDiagnostic, LintResults, LintSeverity, lint_document,
};

pub mod export;
pub use export::format_as_icalendar;

pub mod archive;

pub mod mutations;
pub use mutations::{ActionUpdate, MatchType, ResolvedAction, apply_updates, resolve_reference};

pub mod environment_reader;
pub use environment_reader::{Config, get_config_dir, get_data_dir, load_config};

pub mod telemetry;
pub use telemetry::{
    TelemetryEvent, TelemetryRecord, Tool, emit, emit_event, event_from_field_change,
    event_from_state_change, get_telemetry_dir,
};

/// Merge two JSON hashmaps (right overwrites left on key conflicts)
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
pub fn get_action_list_struct(_opts: &Value, actions: &str) -> Result<ActionList, String> {
    parse_actions(actions)
}

/// Parse a .actions file into a ParsedDocument (Actions + Source Metadata)
pub fn get_parsed_document(actions: &str) -> Result<ParsedDocument, String> {
    parse_document(actions)
}

/// Parse a .actions file and return as JSON Value
pub fn get_action_list(opts: &Value, actions: String) -> Result<Value, String> {
    let action_list = get_action_list_struct(opts, &actions)?;
    serde_json::to_value(&action_list)
        .map_err(|e| format!("Failed to serialize actions to JSON: {}", e))
}

/// Parse a .actions file into a tree-sitter Tree
pub fn get_action_list_tree(actions: &str) -> Result<Tree, String> {
    parse_tree(actions)
}

/// Load all workspace .actions files as a multi-charter DomainModel.
pub fn load_workspace_domain_model(
    data_dir: &std::path::Path,
) -> Result<clearhead_core::DomainModel, String> {
    clearhead_core::load_domain_model(data_dir).map_err(|e| e.to_string())
}

/// Build a [`WorkspaceConfig`] from a tag hierarchy map.
///
/// Extracts the semantic fields core understands, leaving tool-specific
/// config behind. Call with `&config.tag_hierarchies`.
pub fn workspace_config_from(tag_hierarchies: &std::collections::HashMap<String, Vec<String>>) -> WorkspaceConfig {
    WorkspaceConfig {
        tag_hierarchies: tag_hierarchies.clone(),
        ..WorkspaceConfig::default()
    }
}

/// Load all actions from the workspace as a flat ActionList.
pub fn load_workspace_actions(data_dir: &std::path::Path) -> Result<ActionList, String> {
    use clearhead_core::workspace::actions::convert;
    let model = clearhead_core::load_domain_model(data_dir).map_err(|e| e.to_string())?;
    Ok(convert::to_action_list(&model))
}

/// Filter actions using a SPARQL query
pub fn run_sql_query(actions: &ActionList, sparql_query: &str) -> Result<ActionList, String> {
    use clearhead_core::workspace::actions::convert;
    use std::collections::HashSet;

    let store = clearhead_core::graph::create_database()
        .map_err(|e| format!("Failed to create store: {}", e))?;

    let charter = convert::from_actions_with_charter(actions, "_query".to_string());
    let model = clearhead_core::DomainModel {
        objectives: vec![],
        charters: vec![charter],
    };
    clearhead_core::graph::load_domain_model(&store, &model, None)
        .map_err(|e| format!("Failed to load domain model into store: {}", e))?;

    let matching_ids = clearhead_core::graph::query_action_ids(&store, sparql_query)
        .map_err(|e| format!("SPARQL query failed: {}", e))?;

    let id_set: HashSet<String> = matching_ids.into_iter().collect();

    let filtered = actions
        .iter()
        .filter(|action| id_set.contains(&action.id.to_string()))
        .cloned()
        .collect();

    Ok(filtered)
}

/// Build and execute a SPARQL query from a WHERE clause
pub fn run_sql_where(
    actions: &ActionList,
    where_clause: &str,
    select: Option<&str>,
    from: Option<&str>,
) -> Result<ActionList, String> {
    let query = clearhead_core::graph::build_where_query(where_clause, select, from);
    run_sql_query(actions, &query)
}

/// Run a SPARQL query across all workspace actions
///
/// Loads the workspace as a full DomainModel (preserving Charter → Plan hierarchy)
/// so that charter-based SPARQL patterns (bfo:has_part) resolve correctly.
/// Pass `config` to materialise tag hierarchies as contextBroader triples.
pub fn run_workspace_sql_query(
    data_dir: &std::path::Path,
    sparql_query: &str,
    config: Option<&WorkspaceConfig>,
) -> Result<ActionList, String> {
    use clearhead_core::graph;
    use clearhead_core::workspace::actions::convert;
    use std::collections::HashSet;

    let model = load_workspace_domain_model(data_dir)?;
    let store = graph::create_database().map_err(|e| format!("Failed to create store: {}", e))?;
    graph::load_domain_model(&store, &model, config)
        .map_err(|e| format!("Failed to load domain model into store: {}", e))?;

    let matching_ids = graph::query_action_ids(&store, sparql_query)
        .map_err(|e| format!("SPARQL query failed: {}", e))?;
    let id_set: HashSet<String> = matching_ids.into_iter().collect();

    let all_actions = convert::to_action_list(&model);
    Ok(all_actions
        .into_iter()
        .filter(|a| id_set.contains(&a.id.to_string()))
        .collect())
}

/// Run a raw SPARQL SELECT query across the workspace and return all variable bindings
pub fn run_workspace_raw_query(
    data_dir: &std::path::Path,
    sparql: &str,
    config: Option<&WorkspaceConfig>,
) -> Result<Vec<std::collections::HashMap<String, String>>, String> {
    let model = load_workspace_domain_model(data_dir)?;
    let store = clearhead_core::graph::create_store()
        .map_err(|e| format!("Failed to create store: {}", e))?;
    clearhead_core::graph::load_domain_model(&store, &model, config)
        .map_err(|e| format!("Failed to load domain model: {}", e))?;
    clearhead_core::graph::query_raw(&store, sparql)
        .map_err(|e| format!("SPARQL query failed: {}", e))
}

/// Run a raw WHERE clause query across the workspace (SELECT *, prefixes injected)
pub fn run_workspace_raw_where(
    data_dir: &std::path::Path,
    where_clause: &str,
) -> Result<Vec<std::collections::HashMap<String, String>>, String> {
    let query = clearhead_core::graph::build_raw_where_query(where_clause);
    run_workspace_raw_query(data_dir, &query, None)
}

/// Run a SPARQL WHERE clause query across all workspace actions
pub fn run_workspace_sql_where(
    data_dir: &std::path::Path,
    where_clause: &str,
    select: Option<&str>,
    from: Option<&str>,
) -> Result<ActionList, String> {
    let query = clearhead_core::graph::build_where_query(where_clause, select, from);
    run_workspace_sql_query(data_dir, &query, None)
}
