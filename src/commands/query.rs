//! Query commands forward to graphd's own `query` interface.
//!
//! graphd owns query execution, the named-query registry, parameter injection,
//! and rendering. The CLI maps its arguments onto `graphd query …` and execs it
//! with **inherited stdio**, so graphd's terminal-vs-pipe detection sees the
//! real stream and there is exactly one renderer. The CLI adds nothing to the
//! output — it is a pure projection.
//!
//! Chain is the sole exception: resolving a fuzzy action query to a canonical
//! IRI is an actions-domain concern, so the CLI does that here, then forwards
//! `index chain --target <iri>`.

use crate::argparser::QueryFormat;
use crate::commands::CommandContext;
use crate::commands::verb_result::canonical_id;
use std::ffi::OsString;

fn format_arg(format: Option<QueryFormat>) -> Option<&'static str> {
    format.map(|f| match f {
        QueryFormat::Table => "table",
        QueryFormat::Json => "json",
        QueryFormat::Ndjson => "ndjson",
        QueryFormat::Jsonld => "jsonld",
    })
}

fn push_format(args: &mut Vec<OsString>, format: Option<QueryFormat>) {
    if let Some(f) = format_arg(format) {
        args.push("--format".into());
        args.push(f.into());
    }
}

/// Exec `graphd --workspace <ws> query <args…>` with inherited stdio, then
/// propagate its exit status so scripts see graphd's own result.
fn forward(ctx: &CommandContext, args: Vec<OsString>) -> anyhow::Result<()> {
    let status = clearhead_cli::graph_backend::graphd_command()
        .arg("--workspace")
        .arg(&ctx.data_dir)
        .arg("query")
        .args(&args)
        .status()
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to run clearhead-graphd: {e}. Install it or set CLEARHEAD_GRAPHD"
            )
        })?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

pub fn query_workspace(
    ctx: &CommandContext,
    sparql: Option<&str>,
    where_clause: Option<&str>,
    format: Option<QueryFormat>,
) -> anyhow::Result<()> {
    let mut args: Vec<OsString> = vec!["raw".into()];
    if let Some(s) = sparql {
        args.push(s.into());
    }
    if let Some(w) = where_clause {
        args.push("--where".into());
        args.push(w.into());
    }
    push_format(&mut args, format);
    forward(ctx, args)
}

pub fn run_named_query(
    ctx: &CommandContext,
    name: &str,
    status: Option<&str>,
    format: Option<QueryFormat>,
) -> anyhow::Result<()> {
    let mut args: Vec<OsString> = vec!["named".into(), name.into()];
    if let Some(s) = status {
        args.push("--status".into());
        args.push(s.into());
    }
    push_format(&mut args, format);
    forward(ctx, args)
}

pub fn run_index_query(
    ctx: &CommandContext,
    name: Option<&str>,
    format: Option<QueryFormat>,
) -> anyhow::Result<()> {
    let mut args: Vec<OsString> = vec!["index".into()];
    if let Some(n) = name {
        args.push(n.into());
    }
    push_format(&mut args, format);
    forward(ctx, args)
}

pub fn run_chain_query(
    ctx: &CommandContext,
    query: &str,
    format: Option<QueryFormat>,
) -> anyhow::Result<()> {
    let action = super::action::resolve_open_action(ctx, query)?
        .ok_or_else(|| anyhow::anyhow!("No open action matching '{}'", query))?;
    let target = format!("<{}>", canonical_id(action.id));

    let mut args: Vec<OsString> = vec!["index".into(), "chain".into(), "--target".into(), target.into()];
    push_format(&mut args, format);
    forward(ctx, args)
}

pub fn list_named_queries(ctx: &CommandContext) -> anyhow::Result<()> {
    forward(ctx, vec!["list".into()])
}

pub fn show_named_query(ctx: &CommandContext, name: &str) -> anyhow::Result<()> {
    forward(ctx, vec!["show".into(), name.into()])
}
