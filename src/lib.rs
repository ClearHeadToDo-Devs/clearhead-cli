//! ClearHead CLI Library
//!
//! This library provides the CLI and LSP server implementation for the ClearHead framework.
//! It builds on top of clearhead_core, adding filesystem, configuration, and runtime concerns.

use serde_json::{Map, Value};
use tree_sitter::Tree;
use clearhead_core::WorkspaceConfig;

// Re-export core library types and functions
pub use clearhead_core::{
    Action, ActionList, ActionState, Charter, DomainModel, OutputFormat, ParseFailure,
    ParseMode, ParseOutcome, ParsedDocument, Plan, RecoveryReport, SourceMetadata,
    SourceRange, format, format_charter, implicit_charter, parse_actions, parse_actions_with_mode,
    parse_charter, parse_document, parse_tree, patch_action_list,
};

pub use clearhead_core::format::{FormatConfig, FormatStyle, IndentStyle};
pub use clearhead_core::workspace::actions::TableFormatOptions;

// Re-export environment_reader so command handlers can call resolve_workspace_paths
// without reaching back through the CLI binary layer.
pub use environment_reader::resolve_workspace_paths;

fn transient_graph() -> clearhead_core::graph::GraphName {
    clearhead_core::graph::GraphName::NamedNode(
        oxigraph::model::NamedNode::new(clearhead_core::graph::TRANSIENT_GRAPH_URI).unwrap(),
    )
}

fn workspace_graph_name_from_config(config: Option<&WorkspaceConfig>) -> clearhead_core::graph::GraphName {
    config
        .and_then(|c| c.workspace_id.as_deref())
        .map(clearhead_core::graph::workspace_graph_uri)
        .map(clearhead_core::graph::GraphName::NamedNode)
        .unwrap_or_else(transient_graph)
}

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

/// Exposes the CLI argument parser so tooling (e.g. `gen-man`) can build the
/// `clap::Command` tree without depending on the binary entry point.
pub mod argparser;

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
/// Maps the semantic fields core understands for graph operations.
/// Expansion config (`expansion_total_instances`, `expansion_primary_instances`)
/// is not passed here — it only affects file expansion, not graph queries.
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

/// Load a single additional workspace into an existing store under its own named graph.
///
/// Reads `<workspace_root>/.clearhead/config.json` to discover the workspace's
/// stable `workspace_id`.  If the config is absent or has no `workspace_id`, the
/// workspace is loaded into a derived transient graph so data still participates
/// in cross-workspace queries — it will just lack a durable graph name.
///
/// Failures are returned as `Err(String)` so callers can decide whether to warn
/// and continue or propagate.
fn load_workspace_at_path_into_store(
    store: &clearhead_core::graph::Store,
    workspace_path: &std::path::Path,
) -> Result<(), String> {
    if !workspace_path.exists() {
        return Err(format!(
            "Additional workspace path does not exist: {}",
            workspace_path.display()
        ));
    }

    // Read workspace config to obtain the durable workspace_id.
    let config_path = workspace_path.join(".clearhead").join("config.json");
    let workspace_id: Option<String> = if config_path.exists() {
        let json = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read workspace config at {}: {}", config_path.display(), e))?;
        let v: serde_json::Value = serde_json::from_str(&json)
            .map_err(|e| format!("Failed to parse workspace config at {}: {}", config_path.display(), e))?;
        v.get("workspace_id")
            .and_then(|id| id.as_str())
            .map(|s| s.to_string())
    } else {
        None
    };

    // Derive a workspace config for graph loading (tag hierarchies come from the
    // loaded workspace's own config if present, but for cross-workspace queries we
    // keep it simple and pass None; the caller's primary workspace config is used
    // for its own graph; hierarchy injection per additional workspace is a future concern).
    let graph_name = workspace_id
        .as_deref()
        .map(clearhead_core::graph::workspace_graph_uri)
        .map(clearhead_core::graph::GraphName::NamedNode)
        .unwrap_or_else(transient_graph);

    let model = clearhead_core::load_domain_model(workspace_path)
        .map_err(|e| format!("Failed to load workspace at {}: {}", workspace_path.display(), e))?;

    clearhead_core::graph::load_domain_model(store, &model, None, graph_name)
        .map_err(|e| format!("Failed to insert workspace {} into store: {}", workspace_path.display(), e))?;

    Ok(())
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
    let store = graph::create_store().map_err(|e| format!("Failed to create store: {}", e))?;
    graph::load_domain_model(&store, &model, config, workspace_graph_name_from_config(config))
        .map_err(|e| format!("Failed to load domain model into store: {}", e))?;

    // Load each additional workspace into its own named graph in the same store.
    if let Some(cfg) = config {
        for path_str in &cfg.additional_workspaces {
            let path = std::path::Path::new(path_str);
            if let Err(e) = load_workspace_at_path_into_store(&store, path) {
                tracing::warn!("Skipping additional workspace '{}': {}", path_str, e);
            }
        }
    }

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
    let gn = workspace_graph_name_from_config(config);
    clearhead_core::graph::load_domain_model(&store, &model, config, gn)
        .map_err(|e| format!("Failed to load domain model: {}", e))?;

    // Load each additional workspace into its own named graph in the same store.
    if let Some(cfg) = config {
        for path_str in &cfg.additional_workspaces {
            let path = std::path::Path::new(path_str);
            if let Err(e) = load_workspace_at_path_into_store(&store, path) {
                tracing::warn!("Skipping additional workspace '{}': {}", path_str, e);
            }
        }
    }

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

#[cfg(test)]
mod multi_workspace_tests {
    use super::*;
    use clearhead_core::WorkspaceConfig;
    use std::fs;
    use tempfile::TempDir;

    /// Write a minimal workspace under `root` with one action and a config.json
    /// that holds the given `workspace_id`.  Returns the workspace root.
    fn make_workspace(root: &TempDir, name: &str, workspace_id: &str, action_name: &str) -> std::path::PathBuf {
        let ws = root.path().join(name);
        let charters = ws.join(".clearhead").join("charters");
        fs::create_dir_all(&charters).unwrap();

        // Write .clearhead/config.json
        let cfg = serde_json::json!({ "workspace_id": workspace_id });
        fs::write(
            ws.join(".clearhead").join("config.json"),
            serde_json::to_string_pretty(&cfg).unwrap(),
        ).unwrap();

        // Write a single .actions file with one action
        let act_id = uuid::Uuid::now_v7();
        let content = format!(
            "[ ] {} #{}\n",
            action_name,
            act_id
        );
        fs::write(charters.join("inbox.actions"), content).unwrap();

        ws
    }

    #[test]
    fn load_workspace_at_path_loads_model_into_named_graph() {
        let tmp = TempDir::new().unwrap();
        let ws_id = "00000000-0000-7000-8000-000000000001";
        let ws = make_workspace(&tmp, "alpha", ws_id, "Alpha task");

        let store = clearhead_core::graph::create_store().expect("store");
        load_workspace_at_path_into_store(&store, &ws).expect("load workspace");

        // The named graph for this workspace should contain the Action triple.
        let graph = clearhead_core::graph::workspace_graph_uri(ws_id);
        let sparql = format!(
            "PREFIX actions: <https://clearhead.us/vocab/actions/v4#>
             SELECT ?label WHERE {{
               GRAPH <{}> {{
                 ?s a actions:Action ; <http://www.w3.org/2000/01/rdf-schema#label> ?label .
               }}
             }}",
            graph.as_str()
        );
        let rows = clearhead_core::graph::query_raw(&store, &sparql).expect("query");
        assert!(
            rows.iter().any(|r| r.get("label").map(|l| l.as_str()) == Some("Alpha task")),
            "expected 'Alpha task' in named graph; got: {:?}",
            rows
        );
    }

    #[test]
    fn run_workspace_raw_query_merges_additional_workspaces() {
        let tmp = TempDir::new().unwrap();

        // Primary workspace
        let primary_id = "00000000-0000-7000-8000-000000000002";
        let primary = make_workspace(&tmp, "primary", primary_id, "Primary task");

        // Additional workspace
        let extra_id = "00000000-0000-7000-8000-000000000003";
        let extra = make_workspace(&tmp, "extra", extra_id, "Extra task");

        let config = WorkspaceConfig {
            workspace_id: Some(primary_id.to_string()),
            additional_workspaces: vec![extra.to_string_lossy().into_owned()],
            ..WorkspaceConfig::default()
        };

        let sparql = "PREFIX actions: <https://clearhead.us/vocab/actions/v4#>
                      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
                      SELECT ?label WHERE { GRAPH ?g { ?s a actions:Action ; rdfs:label ?label . } }";

        let rows = run_workspace_raw_query(&primary, sparql, Some(&config)).expect("query");
        let labels: Vec<&str> = rows.iter()
            .filter_map(|r| r.get("label").map(|s| s.as_str()))
            .collect();

        assert!(labels.contains(&"Primary task"), "expected primary task; got: {:?}", labels);
        assert!(labels.contains(&"Extra task"), "expected extra task from additional workspace; got: {:?}", labels);
    }

    #[test]
    fn missing_additional_workspace_warns_and_continues() {
        let tmp = TempDir::new().unwrap();
        let primary_id = "00000000-0000-7000-8000-000000000004";
        let primary = make_workspace(&tmp, "primary", primary_id, "Primary task");

        let config = WorkspaceConfig {
            workspace_id: Some(primary_id.to_string()),
            // This path does not exist — should warn, not crash.
            additional_workspaces: vec!["/nonexistent/path/that/does/not/exist".to_string()],
            ..WorkspaceConfig::default()
        };

        let sparql = "PREFIX actions: <https://clearhead.us/vocab/actions/v4#>
                      PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
                      SELECT ?label WHERE { GRAPH ?g { ?s a actions:Action ; rdfs:label ?label . } }";

        // Must succeed (warn internally, not error).
        let rows = run_workspace_raw_query(&primary, sparql, Some(&config)).expect("query");
        let labels: Vec<&str> = rows.iter()
            .filter_map(|r| r.get("label").map(|s| s.as_str()))
            .collect();
        assert!(labels.contains(&"Primary task"), "primary task must still appear; got: {:?}", labels);
    }
}
