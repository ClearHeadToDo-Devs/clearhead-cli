use std::io;
use std::process;
use tracing::{Level, debug, error};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

mod argparser;
use argparser::{Verb, parse_cli};

mod lsp;

pub mod environment_reader;

mod commands;
use commands::CommandContext;

fn main() {
    let cli = parse_cli();

    // Initialize tracing
    let log_level = match cli.debug {
        0 => Level::INFO,
        1 => Level::DEBUG,
        _ => Level::TRACE,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(io::stderr)
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    debug!(debug_level = cli.debug, "Debug mode enabled");
    if let Some(ref config_path) = cli.config {
        debug!(config = ?config_path, "Custom config file specified");
    }

    if let Err(e) = run_command(&cli) {
        error!(error = %e, "Command failed");
        process::exit(1);
    }
}

fn run_command(cli: &argparser::Cli) -> Result<(), String> {
    let ctx = CommandContext::new(cli)?;

    debug!(data_dir = %ctx.data_dir.display(), "Data directory resolved");

    match &cli.command {
        Verb::Read { target } => match target {
            argparser::ReadTarget::Plans {
                format,
                where_clause,
                sparql,
                sparql_file,
                file,
                stdio,
                table_options,
            } => commands::plan::read_plans(
                &ctx,
                format,
                where_clause,
                sparql,
                sparql_file,
                file,
                *stdio,
                table_options,
            ),
            argparser::ReadTarget::Charters {
                format,
                explicit_only,
            } => commands::charter::read_charters(&ctx, format, *explicit_only),
            argparser::ReadTarget::Agenda { file, days } => {
                commands::agenda::run_agenda(&ctx, file, *days)
            }
        },
        Verb::Show { target } => match target {
            argparser::ShowTarget::Plan {
                query,
                file,
                format,
                table_options,
            } => commands::plan::show_plan(&ctx, query, file, format, table_options),
            argparser::ShowTarget::Charter { query } => {
                commands::charter::show_charter(&ctx, query)
            }
        },
        Verb::Add { target } => match target {
            argparser::AddTarget::Plan {
                name,
                file,
                fields,
                write,
            } => commands::plan::add_plan(&ctx, name, file, fields, *write),
            argparser::AddTarget::Charter {
                title,
                alias,
                parent,
                write,
            } => commands::charter::add_charter(&ctx, title, alias, parent, *write),
        },
        Verb::Update { target } => match target {
            argparser::UpdateTarget::Plan {
                query,
                file,
                name,
                fields,
                write,
            } => commands::plan::update_plan(&ctx, query, file, name, fields, *write),
        },
        Verb::Complete { target } => match target {
            argparser::CompleteTarget::Plan { query, file, write } => {
                commands::plan::complete_plan(&ctx, query, file, *write)
            }
        },
        Verb::Delete { target } => match target {
            argparser::DeleteTarget::Plan { query, file, write } => {
                commands::plan::delete_plan(&ctx, query, file, *write)
            }
        },
        Verb::Format { target } => match target {
            argparser::FormatTarget::File {
                path,
                write,
                style,
                indent_style,
                indent_width,
            } => commands::file::format_file(&ctx, path, *write, style, indent_style, indent_width),
        },
        Verb::Lint { target } => match target {
            argparser::LintTarget::File { path } => commands::file::lint_file(path),
        },
        Verb::Normalize { target } => match target {
            argparser::NormalizeTarget::File {
                path,
                write,
                no_format,
            } => commands::file::normalize_file(&ctx, path, *write, *no_format),
        },
        Verb::Patch { target } => match target {
            argparser::PatchTarget::File {
                primary,
                secondary,
                write,
            } => commands::file::patch_file(primary, secondary, *write),
        },
        Verb::Archive { target } => match target {
            argparser::ArchiveTarget::Plans {
                file,
                log_dir,
                dry_run,
            } => commands::plan::archive_plans(&ctx, file, log_dir, *dry_run),
        },
        Verb::Export { target } => match target {
            argparser::ExportTarget::Plans {
                file,
                output,
                open_only,
            } => commands::plan::export_plans(&ctx, file, output, *open_only),
        },
        Verb::Start { target } => match target {
            argparser::StartTarget::Lsp => commands::service::start_lsp(),
        },
        Verb::Sync { target } => match target {
            argparser::SyncTarget::Events { file, dry_run } => {
                commands::service::sync_events(&ctx, file, *dry_run)
            }
        },
    }
}
