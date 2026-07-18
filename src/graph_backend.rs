//! CLI graph runtime seam.
//!
//! Today the implementation still runs the graph in-process via
//! `clearhead_core::graph`, but command handlers should come through this
//! module rather than reaching into core directly. That gives the
//! graph-decoupling charter one place to swap from "embedded Oxigraph" to
//! `clearhead-graphd`.

use clearhead_core::WorkspaceConfig;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const GRAPHD_REQUEST_VERSION: u32 = 1;
const GRAPHD_ENV: &str = "CLEARHEAD_GRAPHD";

#[derive(Serialize)]
struct GraphdQueryRequest<'a> {
    version: u32,
    sparql: &'a str,
    config: GraphdConfig<'a>,
}

#[derive(Serialize)]
struct GraphdConfig<'a> {
    tag_hierarchies: &'a HashMap<String, Vec<String>>,
    additional_workspaces: &'a [String],
}

/// Execute a raw query through the one-shot `clearhead-graphd` process.
///
/// The executable defaults to `clearhead-graphd` on PATH and can be overridden
/// with `CLEARHEAD_GRAPHD`, which also makes the process boundary testable.
/// Stdout is reserved for the version-1 JSON row payload; diagnostics belong
/// on stderr.
pub fn run_graphd_raw_query(
    data_dir: &Path,
    sparql: &str,
    config: &WorkspaceConfig,
) -> Result<Vec<HashMap<String, String>>, String> {
    let executable = std::env::var_os(GRAPHD_ENV).unwrap_or_else(|| "clearhead-graphd".into());
    let request = GraphdQueryRequest {
        version: GRAPHD_REQUEST_VERSION,
        sparql,
        config: GraphdConfig {
            tag_hierarchies: &config.tag_hierarchies,
            additional_workspaces: &config.additional_workspaces,
        },
    };
    let payload = serde_json::to_vec(&request)
        .map_err(|e| format!("Failed to serialize graphd query request: {e}"))?;

    let mut child = Command::new(&executable)
        .arg("--workspace")
        .arg(data_dir)
        .arg("query")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "Failed to start {}: {e}. Install clearhead-graphd or set {GRAPHD_ENV}",
                std::path::Path::new(&executable).display()
            )
        })?;

    child
        .stdin
        .take()
        .ok_or_else(|| "Failed to open graphd stdin".to_string())?
        .write_all(&payload)
        .map_err(|e| format!("Failed to write graphd query request: {e}"))?;

    let output = child
        .wait_with_output()
        .map_err(|e| format!("Failed to wait for graphd: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "clearhead-graphd exited with {}{}",
            output.status,
            if stderr.trim().is_empty() {
                String::new()
            } else {
                format!(": {}", stderr.trim())
            }
        ));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("clearhead-graphd returned invalid JSON rows: {e}"))
}

/// Named graph for a loaded workspace: its configured `workspace_id` when
/// initialized, otherwise the per-load ephemeral fallback id — matching the
/// `ws:Workspace` node `insert_workspace_metadata` emits into the same graph.
fn workspace_graph_name(
    workspace: &clearhead_core::workspace::store::load::Workspace,
) -> clearhead_core::graph::GraphName {
    clearhead_core::graph::GraphName::NamedNode(clearhead_core::graph::workspace_graph_uri(
        &workspace.effective_id(),
    ))
}

/// Load a single additional workspace into an existing store under its own named graph.
///
/// Identity (`workspace_id`, `workspace_name`) is read by core from the
/// workspace's own manifest (`workspace.json`) during load. If no durable id is
/// persisted, the workspace loads into an ephemeral graph so its data still
/// participates in cross-workspace queries — it just lacks a graph name stable
/// across sessions.
///
/// Failures are returned as `Err(String)` so callers can decide whether to warn
/// and continue or propagate.
pub(crate) fn load_workspace_at_path_into_store(
    store: &clearhead_core::graph::Store,
    workspace_path: &Path,
) -> Result<(), String> {
    if !workspace_path.exists() {
        return Err(format!(
            "Additional workspace path does not exist: {}",
            workspace_path.display()
        ));
    }

    let workspace = clearhead_core::workspace::store::load::Workspace::load(workspace_path)
        .map_err(|e| {
            format!(
                "Failed to load workspace at {}: {}",
                workspace_path.display(),
                e
            )
        })?;
    let graph_name = workspace_graph_name(&workspace);

    clearhead_core::graph::insert_workspace_metadata(store, &workspace, graph_name.clone())
        .map_err(|e| {
            format!(
                "Failed to insert workspace metadata for {}: {}",
                workspace_path.display(),
                e
            )
        })?;
    let model = clearhead_core::DomainModel::from(workspace);
    clearhead_core::graph::load_domain_model(store, &model, None, graph_name).map_err(|e| {
        format!(
            "Failed to insert workspace {} into store: {}",
            workspace_path.display(),
            e
        )
    })?;

    Ok(())
}

/// Build the standard raw-WHERE SELECT query with ClearHead's prefix set.
pub fn build_raw_where_query(where_clause: &str) -> String {
    clearhead_core::graph::build_raw_where_query(where_clause)
}

/// Run a raw SPARQL SELECT query across the workspace and return all variable bindings.
pub fn run_workspace_raw_query(
    data_dir: &Path,
    sparql: &str,
    config: Option<&WorkspaceConfig>,
) -> Result<Vec<HashMap<String, String>>, String> {
    let store = clearhead_core::graph::create_store()
        .map_err(|e| format!("Failed to create store: {}", e))?;

    let primary = clearhead_core::workspace::store::load::Workspace::load(data_dir)
        .map_err(|e| format!("Failed to load workspace: {}", e))?;
    let graph_name = workspace_graph_name(&primary);

    clearhead_core::graph::insert_workspace_metadata(&store, &primary, graph_name.clone())
        .map_err(|e| format!("Failed to insert workspace metadata: {}", e))?;
    let model = clearhead_core::DomainModel::from(primary);
    clearhead_core::graph::load_domain_model(&store, &model, config, graph_name)
        .map_err(|e| format!("Failed to load domain model: {}", e))?;

    if let Some(cfg) = config {
        for path_str in &cfg.additional_workspaces {
            let path = Path::new(path_str);
            if let Err(e) = load_workspace_at_path_into_store(&store, path) {
                tracing::warn!("Skipping additional workspace '{}': {}", path_str, e);
            }
        }
    }

    clearhead_core::graph::query_raw(&store, sparql)
        .map_err(|e| format!("SPARQL query failed: {}", e))
}

/// Run a raw WHERE clause query across the workspace (SELECT *, prefixes injected).
pub fn run_workspace_raw_where(
    data_dir: &Path,
    where_clause: &str,
) -> Result<Vec<HashMap<String, String>>, String> {
    let query = build_raw_where_query(where_clause);
    run_workspace_raw_query(data_dir, &query, None)
}

/// Frame an ordered index result according to the query-output contract.
pub fn frame_index(
    rows: &[HashMap<String, String>],
) -> clearhead_core::graph::Result<serde_json::Value> {
    clearhead_core::graph::frame_index(rows)
}

/// Serialize a domain model to canonical JSON-LD.
pub fn serialize_domain_to_jsonld(
    model: &clearhead_core::DomainModel,
) -> clearhead_core::graph::Result<String> {
    clearhead_core::graph::serialize_domain_to_jsonld(model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Write a minimal workspace under `root` with one action and a
    /// workspace.json manifest holding the given `workspace_id`. Returns the
    /// workspace root.
    fn make_workspace(
        root: &TempDir,
        name: &str,
        workspace_id: &str,
        action_name: &str,
    ) -> std::path::PathBuf {
        let ws = root.path().join(name);
        let charters = ws.join(".clearhead").join("charters");
        fs::create_dir_all(&charters).unwrap();

        let manifest = serde_json::json!({ "workspace_id": workspace_id });
        fs::write(
            ws.join(".clearhead").join("workspace.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let action_id = uuid::Uuid::now_v7();
        let content = format!("[ ] {} #{}\n", action_name, action_id);
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

        let graph = clearhead_core::graph::workspace_graph_uri(ws_id);
        let sparql = format!(
            "PREFIX actions: <https://clearhead.us/vocab/actions/v4#>\n             SELECT ?label WHERE {{\n               GRAPH <{}> {{\n                 ?s a actions:Action ; <http://www.w3.org/2000/01/rdf-schema#label> ?label .\n               }}\n             }}",
            graph.as_str()
        );
        let rows = clearhead_core::graph::query_raw(&store, &sparql).expect("query");
        assert!(
            rows.iter()
                .any(|r| r.get("label").map(|l| l.as_str()) == Some("Alpha task")),
            "expected 'Alpha task' in named graph; got: {:?}",
            rows
        );
    }

    #[test]
    fn run_workspace_raw_query_merges_additional_workspaces() {
        let tmp = TempDir::new().unwrap();

        let primary_id = "00000000-0000-7000-8000-000000000002";
        let primary = make_workspace(&tmp, "primary", primary_id, "Primary task");

        let extra_id = "00000000-0000-7000-8000-000000000003";
        let extra = make_workspace(&tmp, "extra", extra_id, "Extra task");

        let config = WorkspaceConfig {
            additional_workspaces: vec![extra.to_string_lossy().to_string()],
            ..WorkspaceConfig::default()
        };

        let sparql = "PREFIX actions: <https://clearhead.us/vocab/actions/v4#>\n             SELECT ?label WHERE { ?s a actions:Action ; <http://www.w3.org/2000/01/rdf-schema#label> ?label . }";

        let rows = run_workspace_raw_query(&primary, sparql, Some(&config)).expect("query");
        let labels: Vec<&str> = rows
            .iter()
            .filter_map(|r| r.get("label").map(String::as_str))
            .collect();

        assert!(
            labels.contains(&"Primary task"),
            "missing primary label: {:?}",
            labels
        );
        assert!(
            labels.contains(&"Extra task"),
            "missing additional-workspace label: {:?}",
            labels
        );
    }

    #[test]
    fn run_workspace_raw_query_skips_missing_additional_workspaces() {
        let tmp = TempDir::new().unwrap();
        let primary = make_workspace(
            &tmp,
            "primary",
            "00000000-0000-7000-8000-000000000004",
            "Primary task",
        );

        let config = WorkspaceConfig {
            additional_workspaces: vec![tmp.path().join("missing").to_string_lossy().to_string()],
            ..WorkspaceConfig::default()
        };

        let sparql = "PREFIX actions: <https://clearhead.us/vocab/actions/v4#>\n             SELECT ?label WHERE { ?s a actions:Action ; <http://www.w3.org/2000/01/rdf-schema#label> ?label . }";

        let rows = run_workspace_raw_query(&primary, sparql, Some(&config)).expect("query");
        let labels: Vec<&str> = rows
            .iter()
            .filter_map(|r| r.get("label").map(String::as_str))
            .collect();

        assert_eq!(labels, vec!["Primary task"]);
    }
}
