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
        if cli.debug > 0 {
            error!(error = %e, "Command failed");
        } else {
            eprintln!("{}", e);
        }
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
                charter,
                recursive,
                file,
                stdio,
                table_options,
            } => commands::plan::read_plans(
                &ctx,
                format,
                charter,
                *recursive,
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
            argparser::ReadTarget::Acts {
                format,
                plan,
                charter,
                open_only,
                file,
            } => commands::act::read_acts_cmd(
                &ctx,
                *format,
                plan.as_deref(),
                charter.as_deref(),
                *open_only,
                file,
            ),
        },
        Verb::Show { target } => match target {
            argparser::ShowTarget::Plan {
                query,
                file,
                format,
                table_options,
            } => commands::plan::show_plan(&ctx, query, file, format, table_options),
            argparser::ShowTarget::Act { query, file } => {
                commands::act::show_act(&ctx, query, file)
            }
            argparser::ShowTarget::Charter { query } => {
                commands::charter::show_charter(&ctx, query)
            }
        },
        Verb::Add { target } => match target {
            argparser::AddTarget::Plan {
                name,
                file,
                charter,
                parent,
                fields,
                schedule,
                dry_run,
            } => commands::plan::add_plan(
                &ctx, name, file, charter, parent, fields, schedule, *dry_run,
            ),
            argparser::AddTarget::Act {
                name,
                charter,
                file,
                parent,
                priority,
                state,
                alias,
                scheduled_at,
                duration,
                dry_run,
            } => commands::act::add_act(
                &ctx, name, charter, file, parent, *priority, *state, alias,
                scheduled_at, *duration, *dry_run,
            ),
            argparser::AddTarget::Charter {
                title,
                alias,
                parent,
                template,
                dry_run,
            } => commands::charter::add_charter(&ctx, title, alias, parent, template, *dry_run),
        },
        Verb::Update { target } => match target {
            argparser::UpdateTarget::Plan {
                query,
                file,
                name,
                fields,
                schedule,
                dry_run,
            } => commands::plan::update_plan(&ctx, query, file, name, fields, schedule, *dry_run),
            argparser::UpdateTarget::Act {
                query,
                name,
                priority,
                state,
                scheduled_at,
                duration,
                file,
                dry_run,
            } => commands::act::update_act(
                &ctx, query, name, *priority, *state, scheduled_at, duration, file, *dry_run,
            ),
        },
        Verb::Complete { target } => match target {
            argparser::CompleteTarget::Plan {
                query,
                file,
                dry_run,
            } => commands::plan::complete_plan(&ctx, query, file, *dry_run),
            argparser::CompleteTarget::Act {
                query,
                file,
                dry_run,
            } => commands::act::complete_act(&ctx, query, file, *dry_run),
        },
        Verb::Delete { target } => match target {
            argparser::DeleteTarget::Plan {
                query,
                file,
                dry_run,
            } => commands::plan::delete_plan(&ctx, query, file, *dry_run),
            argparser::DeleteTarget::Act {
                query,
                file,
                dry_run,
            } => commands::act::delete_act(&ctx, query, file, *dry_run),
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
                scope,
                file,
                dry_run,
            } => commands::plan::archive_plans(&ctx, scope, file, *dry_run),
            argparser::ArchiveTarget::Acts {
                scope,
                file,
                dry_run,
            } => commands::act::archive_acts(&ctx, scope, file, *dry_run),
        },
        Verb::Export { target } => match target {
            argparser::ExportTarget::Plans {
                reference,
                output,
                open_only,
                recursive,
            } => commands::plan::export_plans(&ctx, reference, output, *open_only, *recursive),
        },
        Verb::Start { target } => match target {
            argparser::StartTarget::Lsp => commands::service::start_lsp(),
        },
        Verb::Sync { target } => match target {
            argparser::SyncTarget::Events { file, dry_run } => {
                commands::service::sync_events(&ctx, file, *dry_run)
            }
        },
        Verb::Query { target } => match target {
            argparser::QueryTarget::Run {
                sparql,
                where_clause,
                format,
            } => commands::query::query_workspace(
                &ctx,
                sparql.as_deref(),
                where_clause.as_deref(),
                *format,
            ),
            argparser::QueryTarget::NamedRun {
                name,
                status,
                format,
            } => commands::query::run_named_query(
                &ctx,
                name,
                status.map(|s| s.to_sparql_iri()),
                *format,
            ),
            argparser::QueryTarget::List => commands::query::list_named_queries(&ctx),
        },
        Verb::Debug => commands::debug::run(&ctx),
        Verb::Completion { shell } => {
            use clap::CommandFactory;
            use clap_complete::generate;
            generate(*shell, &mut argparser::Cli::command(), "clearhead", &mut io::stdout());
            Ok(())
        }
        Verb::Expand { target } => match target {
            argparser::ExpandTarget::Acts {
                file,
                days,
                dry_run,
            } => commands::act::expand_acts(&ctx, file, *days, *dry_run),
        },
        Verb::Cancel { target } => match target {
            argparser::CancelTarget::Act {
                query,
                file,
                dry_run,
            } => commands::act::cancel_act(&ctx, query, file, *dry_run),
        },
        Verb::Apply { target } => match target {
            argparser::ApplyTarget::Template {
                name,
                charter,
                file,
                dry_run,
            } => commands::template::apply_template(&ctx, name, charter, file, *dry_run),
        },
    }
}
