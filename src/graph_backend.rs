//! Out-of-process graph backend for the CLI.
//!
//! The CLI speaks plain JSON across this boundary. `clearhead-graphd` owns
//! graph execution and graph-shaped serialization such as JSON-LD.

use clearhead_core::WorkspaceConfig;
use serde::Serialize;
use serde_json::Value;
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
    output: GraphdQueryOutput,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum GraphdQueryOutput {
    Rows,
    IndexJsonld,
}

#[derive(Serialize)]
struct GraphdConfig<'a> {
    tag_hierarchies: &'a HashMap<String, Vec<String>>,
    additional_workspaces: &'a [String],
}

fn graphd_command() -> Command {
    let executable = std::env::var_os(GRAPHD_ENV).unwrap_or_else(|| "clearhead-graphd".into());
    Command::new(executable)
}

fn invoke_graphd(args: &[&std::ffi::OsStr], payload: &[u8]) -> Result<Vec<u8>, String> {
    let mut command = graphd_command();
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|e| {
        format!("Failed to start clearhead-graphd: {e}. Install it or set {GRAPHD_ENV}")
    })?;
    child
        .stdin
        .take()
        .ok_or_else(|| "Failed to open graphd stdin".to_string())?
        .write_all(payload)
        .map_err(|e| format!("Failed to write graphd request: {e}"))?;

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
    Ok(output.stdout)
}

fn run_graphd_query(
    data_dir: &Path,
    sparql: &str,
    config: &WorkspaceConfig,
    output: GraphdQueryOutput,
) -> Result<Value, String> {
    let request = GraphdQueryRequest {
        version: GRAPHD_REQUEST_VERSION,
        sparql,
        config: GraphdConfig {
            tag_hierarchies: &config.tag_hierarchies,
            additional_workspaces: &config.additional_workspaces,
        },
        output,
    };
    let payload = serde_json::to_vec(&request)
        .map_err(|e| format!("Failed to serialize graphd query request: {e}"))?;
    let workspace = data_dir.as_os_str();
    let args = [
        std::ffi::OsStr::new("--workspace"),
        workspace,
        std::ffi::OsStr::new("query"),
    ];
    let stdout = invoke_graphd(&args, &payload)?;
    serde_json::from_slice(&stdout)
        .map_err(|e| format!("clearhead-graphd returned invalid JSON: {e}"))
}

/// Build the standard raw-WHERE SELECT query without requiring graph libraries
/// in the CLI process.
pub fn build_raw_where_query(where_clause: &str) -> String {
    format!(
        "PREFIX actions: <https://clearhead.us/vocab/actions/v4#>\n\
         PREFIX cco: <https://www.commoncoreontologies.org/>\n\
         PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>\n\
         PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>\n\
         PREFIX bfo: <http://purl.obolibrary.org/obo/>\n\
         PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>\n\
         SELECT * WHERE {{ {where_clause} }}"
    )
}

/// Execute a query through graphd and decode its plain JSON binding rows.
pub fn run_graphd_raw_query(
    data_dir: &Path,
    sparql: &str,
    config: &WorkspaceConfig,
) -> Result<Vec<HashMap<String, String>>, String> {
    serde_json::from_value(run_graphd_query(
        data_dir,
        sparql,
        config,
        GraphdQueryOutput::Rows,
    )?)
    .map_err(|e| format!("clearhead-graphd returned invalid query rows: {e}"))
}

/// Execute and frame an index query in graphd, returning its JSON-LD document.
pub fn run_graphd_index_query(
    data_dir: &Path,
    sparql: &str,
    config: &WorkspaceConfig,
) -> Result<Value, String> {
    run_graphd_query(data_dir, sparql, config, GraphdQueryOutput::IndexJsonld)
}

/// Ask graphd to turn a plain JSON domain model into canonical JSON-LD.
pub fn serialize_domain_to_jsonld(model: &clearhead_core::DomainModel) -> Result<String, String> {
    let payload = serde_json::to_vec(model)
        .map_err(|e| format!("Failed to serialize domain model as JSON: {e}"))?;
    let args = [std::ffi::OsStr::new("export-jsonld")];
    let stdout = invoke_graphd(&args, &payload)?;
    String::from_utf8(stdout)
        .map_err(|e| format!("clearhead-graphd returned non-UTF-8 JSON-LD: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_where_builder_injects_standard_prefixes() {
        let query = build_raw_where_query("?s rdfs:label ?name");
        assert!(query.contains("PREFIX actions:"));
        assert!(query.contains("PREFIX rdfs:"));
        assert!(query.contains("SELECT * WHERE { ?s rdfs:label ?name }"));
    }
}
