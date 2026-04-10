use crate::argparser::QueryFormat;
use crate::commands::CommandContext;
use std::collections::HashMap;
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
