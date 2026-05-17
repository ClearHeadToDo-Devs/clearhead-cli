use crate::commands::CommandContext;
use crate::commands::query::run_named_query;
use tracing::debug;

pub fn run_agenda(ctx: &CommandContext) -> Result<(), String> {
    debug!(data_dir = %ctx.data_dir.display(), "Executing agenda");
    run_named_query(ctx, "agenda", None, None)
}
