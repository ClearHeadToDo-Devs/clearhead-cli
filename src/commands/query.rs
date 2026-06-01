use crate::argparser::QueryFormat;
use crate::commands::CommandContext;
use std::io::IsTerminal;
use chrono::Utc;
use std::collections::HashMap;
use tracing::debug;

/// Prepend standard PREFIX declarations for any prefix not already declared.
/// Lets ad-hoc `query run` queries use short names (actions:, rdfs:, cco:, etc.)
/// without requiring the user to know the full IRIs.
fn inject_prefixes(sparql: &str) -> String {
    let lower = sparql.to_lowercase();
    const STANDARD: &[(&str, &str)] = &[
        ("actions", "https://clearhead.us/vocab/actions/v4#"),
        ("cco", "https://www.commoncoreontologies.org/"),
        ("rdfs", "http://www.w3.org/2000/01/rdf-schema#"),
        ("rdf", "http://www.w3.org/1999/02/22-rdf-syntax-ns#"),
        ("bfo", "http://purl.obolibrary.org/obo/"),
        ("xsd", "http://www.w3.org/2001/XMLSchema#"),
    ];
    let missing: String = STANDARD
        .iter()
        .filter(|(p, _)| !lower.contains(&format!("prefix {}:", p)))
        .map(|(p, iri)| format!("PREFIX {}: <{}>\n", p, iri))
        .collect();
    if missing.is_empty() {
        sparql.to_string()
    } else {
        format!("{}{}", missing, sparql)
    }
}

/// Replace well-known placeholders before query execution.
/// Time: ?NOW, ?CUTOFF_DATE → current UTC datetime literal
/// Status: ?STATUS_FILTER → full v4 IRI (e.g. <actions:InProgress>)
fn inject_params(sparql: &str, status: Option<&str>) -> String {
    let sparql = inject_prefixes(sparql);
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let datetime = format!("\"{}\"^^xsd:dateTime", now);
    let mut out = sparql
        .replace("?NOW", &datetime)
        .replace("?CUTOFF_DATE", &datetime);
    if let Some(iri) = status {
        out = out.replace("?STATUS_FILTER", iri);
    }
    out
}

pub fn query_workspace(
    ctx: &CommandContext,
    sparql: Option<&str>,
    where_clause: Option<&str>,
    format: Option<QueryFormat>,
) -> Result<(), String> {
    let full_query = match (sparql, where_clause) {
        (Some(q), None) => q.to_string(),
        (None, Some(w)) => {
            debug!(where_clause = %w, "Building raw WHERE query");
            clearhead_core::graph::build_raw_where_query(w)
        }
        (None, None) => {
            return Err("Provide a SPARQL query or --where clause.\n\
             Usage: clearhead query \"SELECT ?name WHERE { ... }\"\n\
             Usage: clearhead query --where \"?s rdfs:label ?name\""
                .to_string());
        }
        (Some(_), Some(_)) => return Err("Cannot combine positional query and --where".to_string()),
    };

    let wc = ctx.workspace_config();
    let rows =
        clearhead_cli::run_workspace_raw_query(&ctx.data_dir, &inject_params(&full_query, None), Some(&wc))?;

    if rows.is_empty() {
        println!("(no results)");
        return Ok(());
    }

    let default_format = if std::io::stdout().is_terminal() {
        QueryFormat::Table
    } else {
        QueryFormat::Json
    };

    match format.unwrap_or(default_format) {
        QueryFormat::Json => format_as_json(&rows),
        QueryFormat::Table => format_as_table(&rows),
    }
}

// =============================================================================
// Named queries
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuerySource {
    BuiltIn,
    User,
    Project,
}

impl std::fmt::Display for QuerySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuerySource::BuiltIn => write!(f, "built-in"),
            QuerySource::User => write!(f, "user"),
            QuerySource::Project => write!(f, "project"),
        }
    }
}

struct NamedQuery {
    /// Full SPARQL text — either embedded at compile time or read from disk.
    sparql: String,
    source: QuerySource,
}

// Required output columns for each response type.
const QFLIST_REQUIRED: &[&str] = &["name", "status", "source_file", "source_line", "charter_root"];

fn validate_columns(rows: &[HashMap<String, String>], required: &[&str]) -> Result<(), String> {
    let Some(first) = rows.first() else { return Ok(()) };
    let missing: Vec<&str> = required.iter().filter(|c| !first.contains_key(**c)).copied().collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("Query is missing required columns: {}", missing.join(", ")))
    }
}

// Vendored v4 queries embedded at compile time.
const BUILT_IN_QUERIES: &[(&str, &str)] = &[
    (
        "acts-by-phase",
        include_str!("../queries/acts-by-phase.sparql"),
    ),
    ("agenda", include_str!("../queries/agenda.sparql")),
    ("all-plans", include_str!("../queries/all-plans.sparql")),
    (
        "all-plans-simple",
        include_str!("../queries/all-plans-simple.sparql"),
    ),
    (
        "completion-velocity",
        include_str!("../queries/completion-velocity.sparql"),
    ),
    (
        "dependency-chain",
        include_str!("../queries/dependency-chain.sparql"),
    ),
    (
        "high-priority",
        include_str!("../queries/high-priority.sparql"),
    ),
    (
        "next-actions",
        include_str!("../queries/next-actions.sparql"),
    ),
    ("qflist", include_str!("../queries/qflist.sparql")),
    (
        "orphaned-acts",
        include_str!("../queries/orphaned-acts.sparql"),
    ),
    (
        "overdue-tasks",
        include_str!("../queries/overdue-tasks.sparql"),
    ),
    ("open-plans", include_str!("../queries/open-plans.sparql")),
    (
        "plans-with-contexts",
        include_str!("../queries/plans-with-contexts.sparql"),
    ),
];

/// Build the query map. Priority: project > user > built-in.
fn resolve_named_queries(ctx: &CommandContext) -> HashMap<String, NamedQuery> {
    let mut queries: HashMap<String, NamedQuery> = HashMap::new();

    // Built-ins first (lowest priority)
    for (name, sparql) in BUILT_IN_QUERIES {
        queries.insert(
            name.to_string(),
            NamedQuery {
                sparql: sparql.to_string(),
                source: QuerySource::BuiltIn,
            },
        );
    }

    // User-global: ~/.clearhead/queries/
    if let Some(home) = dirs::home_dir() {
        let user_dir = home.join(".clearhead").join("queries");
        scan_query_dir(&user_dir, QuerySource::User, &mut queries);
    }

    // Project-local: <data_dir>/.clearhead/queries/ (highest priority)
    let project_dir = ctx.data_dir.join(".clearhead").join("queries");
    scan_query_dir(&project_dir, QuerySource::Project, &mut queries);

    queries
}

fn scan_query_dir(
    dir: &std::path::Path,
    source: QuerySource,
    out: &mut HashMap<String, NamedQuery>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("sparql") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if let Ok(sparql) = std::fs::read_to_string(&path) {
                    out.insert(stem.to_string(), NamedQuery { sparql, source });
                }
            }
        }
    }
}

pub fn run_named_query(
    ctx: &CommandContext,
    name: &str,
    status: Option<&str>,
    format: Option<QueryFormat>,
) -> Result<(), String> {
    let queries = resolve_named_queries(ctx);
    let named = queries.get(name).ok_or_else(|| {
        format!(
            "No query named '{}'. Use `clearhead query list` to see available.",
            name
        )
    })?;

    let wc = ctx.workspace_config();
    let rows = clearhead_cli::run_workspace_raw_query(
        &ctx.data_dir,
        &inject_params(&named.sparql, status),
        Some(&wc),
    )?;

    if rows.is_empty() {
        println!("(no results)");
        return Ok(());
    }

    let default_format = if std::io::stdout().is_terminal() {
        QueryFormat::Table
    } else {
        QueryFormat::Json
    };

    match format.unwrap_or(default_format) {
        QueryFormat::Json => format_as_json(&rows),
        QueryFormat::Table => format_as_table(&rows),
    }
}

/// Resolve user/project queries saved under queries/<type_name>/.
fn resolve_typed_queries(ctx: &CommandContext, type_name: &str) -> HashMap<String, NamedQuery> {
    let mut queries = HashMap::new();
    if let Some(home) = dirs::home_dir() {
        let dir = home.join(".clearhead").join("queries").join(type_name);
        scan_query_dir(&dir, QuerySource::User, &mut queries);
    }
    let dir = ctx.data_dir.join(".clearhead").join("queries").join(type_name);
    scan_query_dir(&dir, QuerySource::Project, &mut queries);
    queries
}

/// Collect all typed queries (subdirectory-scoped) for display in `query list`.
fn scan_all_typed_queries(ctx: &CommandContext) -> Vec<(String, String, NamedQuery)> {
    let dirs: Vec<(std::path::PathBuf, QuerySource)> = [
        dirs::home_dir().map(|h| (h.join(".clearhead").join("queries"), QuerySource::User)),
        Some((ctx.data_dir.join(".clearhead").join("queries"), QuerySource::Project)),
    ]
    .into_iter()
    .flatten()
    .collect();

    let mut result = Vec::new();
    for (base, source) in dirs {
        let Ok(entries) = std::fs::read_dir(&base) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let Some(type_name) = path.file_name().and_then(|n| n.to_str()).map(String::from) else { continue };
                let mut typed = HashMap::new();
                scan_query_dir(&path, source, &mut typed);
                for (name, query) in typed {
                    result.push((type_name.clone(), name, query));
                }
            }
        }
    }
    result
}

pub fn run_qflist_query(
    ctx: &CommandContext,
    name: Option<&str>,
    format: Option<QueryFormat>,
) -> Result<(), String> {
    let sparql = match name {
        None => BUILT_IN_QUERIES
            .iter()
            .find(|(n, _)| *n == "qflist")
            .map(|(_, s)| s.to_string())
            .ok_or("Built-in qflist query not found")?,
        Some(n) => resolve_typed_queries(ctx, "qflist")
            .remove(n)
            .map(|q| q.sparql)
            .ok_or_else(|| {
                format!(
                    "No qflist query named '{}'. Save a .sparql file to \
                     ~/.clearhead/queries/qflist/ or <workspace>/.clearhead/queries/qflist/",
                    n
                )
            })?,
    };

    let wc = ctx.workspace_config();
    let rows = clearhead_cli::run_workspace_raw_query(
        &ctx.data_dir,
        &inject_params(&sparql, None),
        Some(&wc),
    )?;

    if rows.is_empty() {
        println!("[]");
        return Ok(());
    }

    validate_columns(&rows, QFLIST_REQUIRED)?;

    match format.unwrap_or(QueryFormat::Json) {
        QueryFormat::Json => format_as_json(&rows),
        QueryFormat::Table => format_as_table(&rows),
    }
}

pub fn list_named_queries(ctx: &CommandContext) -> Result<(), String> {
    use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("NAME").fg(Color::Cyan),
        Cell::new("TYPE").fg(Color::Cyan),
        Cell::new("SOURCE").fg(Color::Cyan),
    ]);

    let queries = resolve_named_queries(ctx);
    let mut root_names: Vec<&String> = queries.keys().collect();
    root_names.sort();
    for name in root_names {
        let q = &queries[name];
        table.add_row(vec![Cell::new(name), Cell::new("—"), Cell::new(q.source.to_string())]);
    }

    // Typed queries from subdirectories
    let mut typed = scan_all_typed_queries(ctx);
    typed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (type_name, name, q) in &typed {
        table.add_row(vec![Cell::new(name), Cell::new(type_name), Cell::new(q.source.to_string())]);
    }

    println!("{}", table);
    println!(
        "Freeform: ~/.clearhead/queries/*.sparql or <workspace>/.clearhead/queries/*.sparql\n\
         Typed:    ~/.clearhead/queries/<type>/*.sparql"
    );
    Ok(())
}

fn format_as_json(rows: &[HashMap<String, String>]) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(rows).map_err(|e| format!("Failed to serialize: {}", e))?;
    println!("{}", json);
    Ok(())
}

fn format_as_table(rows: &[HashMap<String, String>]) -> Result<(), String> {
    use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
    use std::collections::BTreeSet;

    // BTreeSet for stable alphabetical column order (HashMap order is undefined)
    let columns: Vec<String> = rows
        .iter()
        .flat_map(|r| r.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(
        columns
            .iter()
            .map(|c| Cell::new(c).fg(Color::Cyan))
            .collect::<Vec<_>>(),
    );

    for row in rows {
        table.add_row(
            columns
                .iter()
                .map(|col| Cell::new(row.get(col).map(|s| s.as_str()).unwrap_or("")))
                .collect::<Vec<_>>(),
        );
    }
    println!("{}", table);
    Ok(())
}
