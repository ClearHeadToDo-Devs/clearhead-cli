use crate::commands::query::run_named_query;
use crate::commands::CommandContext;
use tracing::debug;

/// CLI handler: run the built-in "agenda" named query and display results.
/// The --file and --days args are kept for CLI compat but superseded by the graph query.
pub fn run_agenda(ctx: &CommandContext, _file: &Option<std::path::PathBuf>, _days: u32) -> Result<(), String> {
    debug!(data_dir = %ctx.data_dir.display(), "Executing agenda");
    run_named_query(ctx, "agenda", None)
}
