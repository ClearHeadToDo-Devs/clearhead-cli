use crate::argparser::QueryFormat;
use crate::commands::CommandContext;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use tracing::debug;

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
                .to_string())
        }
        (Some(_), Some(_)) => return Err("Cannot combine positional query and --where".to_string()),
    };

    let rows = clearhead_cli::run_workspace_raw_query(&ctx.data_dir, &full_query)?;

    if rows.is_empty() {
        println!("(no results)");
        return Ok(());
    }

    match format.unwrap_or(QueryFormat::Table) {
        QueryFormat::Json => format_as_json(&rows),
        QueryFormat::Table => format_as_table(&rows),
    }
}

// =============================================================================
// Named queries
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuerySource {
    Builtin,
    User,
    Project,
}

impl std::fmt::Display for QuerySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuerySource::Builtin => write!(f, "builtin"),
            QuerySource::User => write!(f, "user"),
            QuerySource::Project => write!(f, "project"),
        }
    }
}

enum QueryLocation {
    File(PathBuf),
    Builtin(&'static str),
}

struct NamedQuery {
    location: QueryLocation,
    source: QuerySource,
}

const BUILTIN_QUERIES: &[(&str, &str)] = &[
    (
        "all-plans",
        include_str!("../resources/queries/all-plans.sparql"),
    ),
    (
        "open-acts",
        include_str!("../resources/queries/open-acts.sparql"),
    ),
    (
        "overdue-acts",
        include_str!("../resources/queries/overdue-acts.sparql"),
    ),
    ("agenda", include_str!("../resources/queries/agenda.sparql")),
];

/// Scan user-global and project-local query directories, returning a map of
/// stem → NamedQuery. Project entries override user entries on name collision.
fn resolve_named_queries(ctx: &CommandContext) -> HashMap<String, NamedQuery> {
    let mut queries: HashMap<String, NamedQuery> = HashMap::new();

    // Builtin scope (lowest precedence)
    scan_builtin_queries(&mut queries);

    // User scope: <data_dir>/queries
    let user_dir = ctx.data_dir.join("queries");
    scan_query_dir(&user_dir, QuerySource::User, &mut queries);

    // Project scope: nearest workspace root/.clearhead/queries (overrides user)
    if let Ok(cwd) = env::current_dir() {
        if let Some(workspace_root) = clearhead_core::workspace::check_for_workspace(&cwd) {
            let project_dir = workspace_root.join(".clearhead").join("queries");
            scan_query_dir(&project_dir, QuerySource::Project, &mut queries);
        }
    }

    queries
}

fn scan_builtin_queries(out: &mut HashMap<String, NamedQuery>) {
    for (name, content) in BUILTIN_QUERIES {
        out.insert(
            (*name).to_string(),
            NamedQuery {
                location: QueryLocation::Builtin(content),
                source: QuerySource::Builtin,
            },
        );
    }
}

fn query_search_paths(ctx: &CommandContext) -> Vec<(QuerySource, PathBuf)> {
    let mut paths = Vec::new();
    paths.push((QuerySource::User, ctx.data_dir.join("queries")));

    if let Ok(cwd) = env::current_dir() {
        if let Some(workspace_root) = clearhead_core::workspace::check_for_workspace(&cwd) {
            paths.push((
                QuerySource::Project,
                workspace_root.join(".clearhead").join("queries"),
            ));
        }
    }

    paths
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
                out.insert(
                    stem.to_string(),
                    NamedQuery {
                        location: QueryLocation::File(path.clone()),
                        source,
                    },
                );
            }
        }
    }
}

pub fn run_named_query(
    ctx: &CommandContext,
    name: &str,
    format: Option<QueryFormat>,
) -> Result<(), String> {
    let queries = resolve_named_queries(ctx);
    let named = queries.get(name).ok_or_else(|| {
        let searched = query_search_paths(ctx)
            .into_iter()
            .map(|(source, path)| format!("  - {}: {}", source, path.display()))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "No query named '{}'.\nSearched:\n{}\nUse `clearhead query list` to see available.",
            name, searched
        )
    })?;

    let query_text = match &named.location {
        QueryLocation::Builtin(content) => (*content).to_string(),
        QueryLocation::File(path) => std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read query file '{}': {}", path.display(), e))?,
    };

    let full_query = normalize_query_file(query_text.trim());
    let rows = clearhead_cli::run_workspace_raw_query(&ctx.data_dir, &full_query)?;

    if rows.is_empty() {
        println!("(no results)");
        return Ok(());
    }

    match format.unwrap_or(QueryFormat::Table) {
        QueryFormat::Json => format_as_json(&rows),
        QueryFormat::Table => format_as_table(&rows),
    }
}

pub fn list_named_queries(ctx: &CommandContext) -> Result<(), String> {
    use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};

    let queries = resolve_named_queries(ctx);

    if queries.is_empty() {
        println!("No named queries found.");
        println!("Builtin queries are unavailable (unexpected). Add files to <data_dir>/queries or <workspace>/.clearhead/queries");
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("NAME").fg(Color::Cyan),
        Cell::new("SCOPE").fg(Color::Cyan),
        Cell::new("PATH").fg(Color::Cyan),
    ]);

    let mut names: Vec<&String> = queries.keys().collect();
    names.sort();
    for name in names {
        let q = &queries[name];
        let path = match &q.location {
            QueryLocation::Builtin(_) => "(builtin)".to_string(),
            QueryLocation::File(path) => path.display().to_string(),
        };
        table.add_row(vec![
            Cell::new(name),
            Cell::new(q.source.to_string()),
            Cell::new(path),
        ]);
    }

    println!("{}", table);
    Ok(())
}

fn format_as_json(rows: &[HashMap<String, String>]) -> Result<(), String> {
    let json =
        serde_json::to_string_pretty(rows).map_err(|e| format!("Failed to serialize: {}", e))?;
    println!("{}", json);
    Ok(())
}

fn normalize_query_file(raw: &str) -> String {
    if looks_like_full_sparql(raw) {
        raw.to_string()
    } else {
        clearhead_core::graph::build_raw_where_query(raw)
    }
}

fn looks_like_full_sparql(raw: &str) -> bool {
    let upper = raw.to_ascii_uppercase();
    upper.contains("SELECT") || upper.contains("ASK") || upper.contains("CONSTRUCT")
}

fn format_as_table(rows: &[HashMap<String, String>]) -> Result<(), String> {
    use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
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
