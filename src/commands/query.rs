use anyhow::Context;
use crate::argparser::QueryFormat;
use crate::commands::CommandContext;
use crate::commands::verb_result::canonical_id;
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
    let now_dt = Utc::now();
    let now = now_dt.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let datetime = format!("\"{}\"^^xsd:dateTime", now);
    let end_of_today = format!("\"{}T23:59:59Z\"^^xsd:dateTime", now_dt.format("%Y-%m-%d"));
    let end_of_week = format!(
        "\"{}T23:59:59Z\"^^xsd:dateTime",
        (now_dt + chrono::Duration::days(7)).format("%Y-%m-%d")
    );
    let mut out = sparql
        .replace("?NOW", &datetime)
        .replace("?CUTOFF_DATE", &datetime)
        .replace("?END_OF_TODAY", &end_of_today)
        .replace("?END_OF_WEEK", &end_of_week);
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
) -> anyhow::Result<()> {
    let full_query = match (sparql, where_clause) {
        (Some(q), None) => q.to_string(),
        (None, Some(w)) => {
            debug!(where_clause = %w, "Building raw WHERE query");
            clearhead_core::graph::build_raw_where_query(w)
        }
        (None, None) => {
            anyhow::bail!("Provide a SPARQL query or --where clause.\n\
             Usage: clearhead query \"SELECT ?name WHERE {{ ... }}\"\n\
             Usage: clearhead query --where \"?s rdfs:label ?name\"");
        }
        (Some(_), Some(_)) => anyhow::bail!("Cannot combine positional query and --where"),
    };

    let wc = ctx.workspace_config();
    let rows =
        clearhead_cli::run_workspace_raw_query(&ctx.data_dir, &inject_params(&full_query, None), Some(&wc))
            .map_err(|e| anyhow::anyhow!(e))?;

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

// Built-in index queries (checked before user/project dirs).
// The index shape contract (required terms, JSON-LD framing) lives in
// clearhead_core::graph::shape — the engine validates, the CLI just prints.
const BUILT_IN_INDEX_QUERIES: &[(&str, &str)] = &[
    ("agenda", include_str!("../queries/index/agenda.sparql")),
    ("default", include_str!("../queries/index/default.sparql")),
    ("unscheduled", include_str!("../queries/index/unscheduled.sparql")),
    ("weekly", include_str!("../queries/index/weekly.sparql")),
];

// Vendored v4 queries embedded at compile time.
const BUILT_IN_QUERIES: &[(&str, &str)] = &[
    (
        "actions-by-phase",
        include_str!("../queries/actions-by-phase.sparql"),
    ),
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
    (
        "orphaned-actions",
        include_str!("../queries/orphaned-actions.sparql"),
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

    // User-global: <config_dir>/queries/ (XDG: ~/.config/clearhead/queries/)
    scan_query_dir(&ctx.config_dir.join("queries"), QuerySource::User, &mut queries);

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
) -> anyhow::Result<()> {
    let queries = resolve_named_queries(ctx);
    let named = queries.get(name).ok_or_else(|| {
        anyhow::anyhow!(
            "No query named '{}'. Use `clearhead query list` to see available.",
            name
        )
    })?;

    let wc = ctx.workspace_config();
    let rows = clearhead_cli::run_workspace_raw_query(
        &ctx.data_dir,
        &inject_params(&named.sparql, status),
        Some(&wc),
    )
    .map_err(|e| anyhow::anyhow!(e))?;

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
    let user_dir = ctx.config_dir.join("queries").join(type_name);
    scan_query_dir(&user_dir, QuerySource::User, &mut queries);
    let dir = ctx.data_dir.join(".clearhead").join("queries").join(type_name);
    scan_query_dir(&dir, QuerySource::Project, &mut queries);
    queries
}

/// Collect all typed queries (subdirectory-scoped) for display in `query list`.
fn scan_all_typed_queries(ctx: &CommandContext) -> Vec<(String, String, NamedQuery)> {
    let dirs: Vec<(std::path::PathBuf, QuerySource)> = vec![
        (ctx.config_dir.join("queries"), QuerySource::User),
        (ctx.data_dir.join(".clearhead").join("queries"), QuerySource::Project),
    ];

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

pub fn run_index_query(
    ctx: &CommandContext,
    name: Option<&str>,
    format: Option<QueryFormat>,
) -> anyhow::Result<()> {
    let name = name.unwrap_or("default");
    // Most-local wins: a project or user file shadows the built-in of the
    // same name — "the query is the wisdom: transparent, inspectable,
    // overridable". Start from `query show <name>` to copy-and-tweak.
    let sparql = resolve_typed_queries(ctx, "index")
        .remove(name)
        .map(|q| q.sparql)
        .or_else(|| {
            BUILT_IN_INDEX_QUERIES
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, s)| s.to_string())
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No index query named '{}'. Save a .sparql file to \
                 <config>/queries/index/ or <workspace>/.clearhead/queries/index/",
                name
            )
        })?;

    let wc = ctx.workspace_config();
    let rows = clearhead_cli::run_workspace_raw_query(
        &ctx.data_dir,
        &inject_params(&sparql, None),
        Some(&wc),
    )
    .map_err(|e| anyhow::anyhow!(e))?;

    match format.unwrap_or(QueryFormat::Json) {
        QueryFormat::Json => {
            let doc = clearhead_core::graph::frame_index(&rows)?;
            let json = serde_json::to_string_pretty(&doc)
                .context("Failed to serialize")?;
            println!("{}", json);
            Ok(())
        }
        QueryFormat::Table => {
            // Human view — rendered from raw rows; still contract-checked.
            clearhead_core::graph::frame_index(&rows)?;
            if rows.is_empty() {
                println!("(no results)");
                return Ok(());
            }
            format_as_table(&rows)
        }
    }
}

/// Every open action that must be completed before `query`'s resolved action
/// can start — `chain.sparql` parameterized by that action's canonical id.
/// `?TARGET_ACTION` isn't one of inject_params's fixed placeholders (it's
/// resolved per-invocation from user input, not derived from wall-clock
/// time), so it's substituted here before the standard injection pass.
pub fn run_chain_query(
    ctx: &CommandContext,
    query: &str,
    format: Option<QueryFormat>,
) -> anyhow::Result<()> {
    let action = super::action::resolve_open_action(ctx, query)?.ok_or_else(|| {
        anyhow::anyhow!("No open action matching '{}'", query)
    })?;

    let target = format!("<{}>", canonical_id(action.id));
    let sparql = include_str!("../queries/index/chain.sparql").replace("?TARGET_ACTION", &target);

    let wc = ctx.workspace_config();
    let rows = clearhead_cli::run_workspace_raw_query(
        &ctx.data_dir,
        &inject_params(&sparql, None),
        Some(&wc),
    )
    .map_err(|e| anyhow::anyhow!(e))?;

    match format.unwrap_or(QueryFormat::Json) {
        QueryFormat::Json => {
            let doc = clearhead_core::graph::frame_index(&rows)?;
            let json = serde_json::to_string_pretty(&doc).context("Failed to serialize")?;
            println!("{}", json);
            Ok(())
        }
        QueryFormat::Table => {
            clearhead_core::graph::frame_index(&rows)?;
            if rows.is_empty() {
                println!("(no results)");
                return Ok(());
            }
            format_as_table(&rows)
        }
    }
}

pub fn list_named_queries(ctx: &CommandContext) -> anyhow::Result<()> {
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

    // Built-in index queries
    for (name, _) in BUILT_IN_INDEX_QUERIES {
        table.add_row(vec![Cell::new(name), Cell::new("index"), Cell::new("built-in")]);
    }

    // Typed queries from subdirectories (user/project)
    let mut typed = scan_all_typed_queries(ctx);
    typed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    for (type_name, name, q) in &typed {
        table.add_row(vec![Cell::new(name), Cell::new(type_name), Cell::new(q.source.to_string())]);
    }

    println!("{}", table);
    println!(
        "Freeform: ~/.config/clearhead/queries/*.sparql or <workspace>/.clearhead/queries/*.sparql\n\
         Typed:    ~/.config/clearhead/queries/<type>/*.sparql\n\
         Inspect any query with `clearhead query show <name>`"
    );
    Ok(())
}

/// Print a query's SPARQL to stdout, raw and pipeable:
/// `clearhead query show agenda > ~/.config/clearhead/queries/index/agenda.sparql`
/// is the sanctioned copy-and-tweak override workflow.
pub fn show_named_query(ctx: &CommandContext, name: &str) -> anyhow::Result<()> {
    // Same resolution order the runners use: typed (project > user), then
    // built-in index, then freeform named (project > user > built-in).
    let sparql = resolve_typed_queries(ctx, "index")
        .remove(name)
        .map(|q| q.sparql)
        .or_else(|| {
            BUILT_IN_INDEX_QUERIES
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, s)| s.to_string())
        })
        .or_else(|| resolve_named_queries(ctx).remove(name).map(|q| q.sparql))
        .ok_or_else(|| {
            anyhow::anyhow!("No query named '{}'. Use `clearhead query list` to see available.", name)
        })?;
    print!("{}", sparql);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn end_of_today_injects_todays_date_at_end_of_day() {
        let query = "FILTER(?x <= ?END_OF_TODAY)";
        let result = inject_params(query, None);
        let today = Utc::now().format("%Y-%m-%d").to_string();
        assert!(
            result.contains(&format!("\"{}T23:59:59Z\"^^xsd:dateTime", today)),
            "unexpected output: {result}"
        );
    }

    #[test]
    fn end_of_today_does_not_contain_current_time() {
        // END_OF_TODAY must always be 23:59:59, never the actual clock time.
        let result = inject_params("?END_OF_TODAY", None);
        assert!(result.contains("T23:59:59Z"), "unexpected output: {result}");
    }

    #[test]
    fn end_of_week_is_seven_days_out_at_end_of_day() {
        let result = inject_params("?END_OF_WEEK", None);
        let expected = (Utc::now() + chrono::Duration::days(7)).format("%Y-%m-%d").to_string();
        assert!(
            result.contains(&format!("\"{}T23:59:59Z\"^^xsd:dateTime", expected)),
            "unexpected output: {result}"
        );
    }

    #[test]
    fn now_injection_still_works() {
        let before = Utc::now().format("%Y-%m-%d").to_string();
        let result = inject_params("?NOW", None);
        let after = Utc::now().format("%Y-%m-%d").to_string();
        // Either today's date is present (common case) or it crossed midnight
        // during the test — accept either adjacent date.
        assert!(
            result.contains(&before) || result.contains(&after),
            "unexpected output: {result}"
        );
        assert!(result.contains("^^xsd:dateTime"), "unexpected output: {result}");
    }

    #[test]
    fn status_filter_replaced_when_provided() {
        let result = inject_params("FILTER(?state = ?STATUS_FILTER)", Some("<actions:InProgress>"));
        assert!(result.contains("<actions:InProgress>"), "unexpected output: {result}");
        assert!(!result.contains("?STATUS_FILTER"), "placeholder not replaced: {result}");
    }
}

fn format_as_json(rows: &[HashMap<String, String>]) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(rows).context("Failed to serialize")?;
    println!("{}", json);
    Ok(())
}

fn format_as_table(rows: &[HashMap<String, String>]) -> anyhow::Result<()> {
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
