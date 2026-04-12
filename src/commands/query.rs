use crate::argparser::QueryFormat;
use crate::commands::CommandContext;
use std::collections::HashMap;
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
    User,
    Project,
}

impl std::fmt::Display for QuerySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuerySource::User => write!(f, "user"),
            QuerySource::Project => write!(f, "project"),
        }
    }
}

struct NamedQuery {
    path: PathBuf,
    source: QuerySource,
}

/// Scan user-global and project-local query directories, returning a map of
/// stem → NamedQuery. Project entries override user entries on name collision.
fn resolve_named_queries(ctx: &CommandContext) -> HashMap<String, NamedQuery> {
    let mut queries: HashMap<String, NamedQuery> = HashMap::new();

    // User-global: ~/.clearhead/queries/
    if let Some(home) = dirs::home_dir() {
        let user_dir = home.join(".clearhead").join("queries");
        scan_query_dir(&user_dir, QuerySource::User, &mut queries);
    }

    // Project-local: <data_dir>/.clearhead/queries/ (overrides user)
    let project_dir = ctx.data_dir.join(".clearhead").join("queries");
    scan_query_dir(&project_dir, QuerySource::Project, &mut queries);

    queries
}

fn scan_query_dir(dir: &std::path::Path, source: QuerySource, out: &mut HashMap<String, NamedQuery>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("sparql") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                out.insert(stem.to_string(), NamedQuery { path: path.clone(), source });
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
        format!(
            "No query named '{}'. Use `clearhead query list` to see available.",
            name
        )
    })?;

    let where_clause = std::fs::read_to_string(&named.path)
        .map_err(|e| format!("Failed to read query file '{}': {}", named.path.display(), e))?;

    let full_query = clearhead_core::graph::build_raw_where_query(where_clause.trim());
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
        println!("Add .sparql files to ~/.clearhead/queries/ or <workspace>/.clearhead/queries/");
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    table.set_header(vec![
        Cell::new("NAME").fg(Color::Cyan),
        Cell::new("SOURCE").fg(Color::Cyan),
    ]);

    let mut names: Vec<&String> = queries.keys().collect();
    names.sort();
    for name in names {
        let q = &queries[name];
        table.add_row(vec![Cell::new(name), Cell::new(q.source.to_string())]);
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
