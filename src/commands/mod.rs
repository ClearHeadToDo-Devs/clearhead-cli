pub mod action;
pub mod agenda;
pub mod charter;
pub mod complete;
pub mod debug;
pub mod file;
pub mod plan;
pub mod query;
pub mod resolver;
pub mod service;
pub mod template;

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use tracing::warn;
use uuid::Uuid;

use crate::environment_reader::{
    Config, ensure_dir_exists, find_project_data_dir, get_data_dir, load_config,
    resolve_config_path, resolve_file_path,
};
use clearhead_cli::ActionList;
use clearhead_cli::telemetry::{TelemetryEvent, Tool, emit_event};

/// Consolidated command context: config + resolved directories.
pub struct CommandContext {
    pub config: Config,
    pub data_dir: PathBuf,
    #[allow(dead_code)]
    pub config_dir: PathBuf,
    pub config_path: PathBuf,
    pub project_root: Option<PathBuf>,
}

impl CommandContext {
    pub fn new(cli: &crate::argparser::Cli) -> Result<Self, String> {
        let config_path = resolve_config_path(cli.config.clone());
        let config =
            load_config(cli.config.clone()).map_err(|e| format!("Failed to load config: {}", e))?;

        let project_root = find_project_data_dir();

        let data_dir = if config.data_dir.is_empty() {
            project_root.clone().unwrap_or_else(get_data_dir)
        } else {
            resolve_file_path(&config.data_dir, &get_data_dir())
        };
        let config_dir = resolve_file_path(
            &config.config_dir,
            &crate::environment_reader::get_config_dir(),
        );

        ensure_dir_exists(&data_dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
        ensure_dir_exists(&config_dir)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;

        Ok(Self {
            config,
            data_dir,
            config_dir,
            config_path,
            project_root,
        })
    }

    /// Resolve an optional file arg against the default file from config.
    pub fn resolve_action_file(&self, file: Option<&PathBuf>) -> PathBuf {
        let charter_root = clearhead_core::charter_root(&self.data_dir);
        file.cloned()
            .unwrap_or_else(|| resolve_file_path(&self.config.default_file, &charter_root))
    }

    /// Resolve indent style and width from config.
    pub fn indent_config(&self) -> (clearhead_cli::IndentStyle, usize) {
        (
            parse_indent_style(&self.config.cli_indent_style),
            self.config.cli_indent_width,
        )
    }
}

/// Load actions from a .actions file on disk.
pub fn load_file(path: &Path) -> Result<ActionList, String> {
    load_file_for_read(path, "load-file")
}

/// Load actions for read-only operations using recoverable parse mode.
pub fn load_file_for_read(path: &Path, command: &str) -> Result<ActionList, String> {
    if !path.exists() {
        return Ok(ActionList::new());
    }

    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;
    parse_content_for_read(&content, &path.display().to_string(), command)
}

/// Parse actions content for read-only operations using recoverable parse mode.
pub fn parse_content_for_read(
    content: &str,
    source: &str,
    command: &str,
) -> Result<ActionList, String> {
    let outcome =
        clearhead_cli::parse_actions_with_mode(content, clearhead_cli::ParseMode::Recover)
            .map_err(|e| format!("Failed to parse '{}': {}", source, e))?;

    if !outcome.syntax_errors.is_empty() {
        report_parse_recovered(
            source,
            command,
            &outcome.syntax_errors,
            outcome.recovery.recoverable_actions,
        );
    }

    Ok(outcome.document.actions)
}

/// Parse actions content for mutating operations.
///
/// If syntax issues are present, this returns an error and callers must not write.
pub fn parse_content_for_mutation(
    content: &str,
    source: &str,
    command: &str,
) -> Result<ActionList, String> {
    let outcome =
        clearhead_cli::parse_actions_with_mode(content, clearhead_cli::ParseMode::Recover)
            .map_err(|e| format!("Failed to parse '{}': {}", source, e))?;

    if !outcome.syntax_errors.is_empty() {
        report_mutation_parse_failure(Path::new(source), command, &outcome.syntax_errors);
        return Err(format!(
            "Parse error in '{}': {} issue(s). File not modified.",
            source,
            outcome.syntax_errors.len()
        ));
    }

    Ok(outcome.document.actions)
}

/// Load actions for mutating operations.
///
/// If syntax issues are present, this returns an error and callers must not write.
pub fn load_file_for_mutation(path: &Path, command: &str) -> Result<ActionList, String> {
    if !path.exists() {
        return Ok(ActionList::new());
    }

    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;
    parse_content_for_mutation(&content, &path.display().to_string(), command)
}

/// Format actions and write to a .actions file on disk.
///
/// Also updates the charter sidecar (best-effort — sidecar failures
/// are logged but do not prevent the actions file from being saved).
pub fn save_file(path: &Path, actions: &ActionList) -> Result<(), String> {
    let content = clearhead_cli::format(actions, clearhead_cli::OutputFormat::Actions, None, None)?;
    fs::write(path, &content)
        .map_err(|e| format!("Failed to write file '{}': {}", path.display(), e))?;
    if let Err(e) = update_sidecar(path, actions) {
        warn!(path = %path.display(), error = %e, "Failed to update sidecar");
    }
    Ok(())
}

/// Ensure every action in the list has an entry in the charter sidecar.
///
/// New entries get `created` set to either the action's `created_at`
/// (if present in the DSL) or `now()`. Existing entries are not overwritten.
pub fn update_sidecar(actions_path: &Path, actions: &ActionList) -> Result<(), String> {
    use clearhead_core::workspace::sidecar;
    use chrono::Local;

    let sc_path = sidecar::sidecar_path(actions_path);
    let mut meta = sidecar::read_sidecar(&sc_path).map_err(|e| e.to_string())?;

    for action in actions.iter() {
        let key = action.id.to_string();
        meta.acts.entry(key).or_insert_with(|| sidecar::ActMeta {
            created: Some(action.created_at.unwrap_or_else(Local::now)),
            source_vevent: None,
        });
    }

    sidecar::write_sidecar(&sc_path, &meta).map_err(|e| e.to_string())
}

/// Write content to a file if `write` is true, otherwise print to stdout.
pub fn write_or_print(content: &str, write: bool, file: Option<&PathBuf>) -> Result<(), String> {
    if write {
        let path = file.ok_or("Cannot use --write without specifying a file")?;
        fs::write(path, content).map_err(|e| format!("Failed to write to file: {}", e))
    } else {
        println!("{}", content);
        Ok(())
    }
}

/// Emit a telemetry event, logging a warning on failure instead of propagating.
pub fn try_emit(action_id: &Uuid, event: TelemetryEvent) {
    if let Err(e) = emit_event(Tool::Cli, Some(action_id.to_string()), event) {
        warn!(error = %e, "Failed to emit telemetry event");
    }
}

#[allow(dead_code)]
/// Search all workspace .actions files for a plan matching `query`.
///
/// Returns the resolved file path and the full ActionList from that file.
/// Uses recover-mode parsing for read-oriented lookup compatibility.
pub fn find_plan_file(data_dir: &Path, query: &str) -> Result<(PathBuf, ActionList), String> {
    let action_files = clearhead_core::list_action_files(data_dir)
        .map_err(|e| format!("Failed to list workspace: {}", e))?;

    for file_path in action_files {
        let actions = load_file_for_read(&file_path, "find-plan-file")?;
        if clearhead_cli::resolve_reference(&actions, query).is_some() {
            return Ok((file_path, actions));
        }
    }

    Err(format!("No plan found matching '{}'", query))
}

/// Search all workspace .actions files for a plan matching `query`, using mutation-safe parsing.
#[allow(dead_code)]
pub fn find_plan_file_for_mutation(
    data_dir: &Path,
    query: &str,
    command: &str,
) -> Result<(PathBuf, ActionList), String> {
    let action_files = clearhead_core::list_action_files(data_dir)
        .map_err(|e| format!("Failed to list workspace: {}", e))?;

    for file_path in action_files {
        let actions = load_file_for_mutation(&file_path, command)?;
        if clearhead_cli::resolve_reference(&actions, query).is_some() {
            return Ok((file_path, actions));
        }
    }

    Err(format!("No plan found matching '{}'", query))
}

/// Read input from a file or stdin
pub fn read_input(file: Option<&PathBuf>) -> Result<String, String> {
    match file {
        Some(path) => fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e)),
        None => {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .map_err(|e| format!("Failed to read from stdin: {}", e))?;
            Ok(buffer)
        }
    }
}

/// Resolve a charter query to the primary `.actions` file for that charter.
///
/// Scans all workspace action files and matches the inferred charter name against
/// the query (by UUID prefix, alias, or inferred file stem / directory name).
pub fn charter_to_file_path(data_dir: &Path, charter_query: &str) -> Result<PathBuf, String> {
    let data_root = clearhead_core::charter_root(data_dir);
    let action_files = clearhead_core::list_action_files(data_dir)
        .map_err(|e| format!("Failed to list workspace: {}", e))?;

    let query_lower = charter_query.to_lowercase();

    for file_path in &action_files {
        let relative = file_path
            .strip_prefix(&data_root)
            .unwrap_or(file_path.as_path());
        let inferred = clearhead_core::infer_charter_name(relative).unwrap_or_default();
        if inferred.to_lowercase() == query_lower {
            return Ok(file_path.clone());
        }
    }

    // Fall back to model-level resolution (matches alias, UUID, partial title)
    let model = clearhead_core::load_domain_model(data_dir).map_err(|e| e.to_string())?;
    let found = crate::commands::charter::resolve_charter(&model.charters, charter_query)
        .ok_or_else(|| format!("No charter found matching '{}'", charter_query))?;

    let key = found.alias.as_deref().unwrap_or(&found.title);
    let key_lower = key.to_lowercase();

    for file_path in &action_files {
        let relative = file_path
            .strip_prefix(&data_root)
            .unwrap_or(file_path.as_path());
        let inferred = clearhead_core::infer_charter_name(relative).unwrap_or_default();
        if inferred.to_lowercase() == key_lower {
            return Ok(file_path.clone());
        }
    }

    Err(format!(
        "No actions file found for charter '{}'",
        charter_query
    ))
}

fn parse_indent_style(s: &str) -> clearhead_cli::IndentStyle {
    match s.to_lowercase().as_str() {
        "tabs" => clearhead_cli::IndentStyle::Tabs,
        _ => clearhead_cli::IndentStyle::Spaces,
    }
}

fn report_parse_recovered(
    source: &str,
    command: &str,
    syntax_errors: &[clearhead_cli::LintDiagnostic],
    recoverable_actions: usize,
) {
    emit_event(
        Tool::Cli,
        None,
        TelemetryEvent::ParseRecovered {
            file_path: source.to_string(),
            error_count: syntax_errors.len(),
            recoverable_count: recoverable_actions,
        },
    )
    .unwrap_or_else(|e| warn!(error = %e, "Failed to emit parse_recovered telemetry"));

    eprintln!(
        "warning: [{}] {} parsed with {} issue(s); proceeding with {} recoverable action(s)",
        source,
        command,
        syntax_errors.len(),
        recoverable_actions
    );
    print_syntax_diagnostics(syntax_errors);
}

fn report_mutation_parse_failure(
    path: &Path,
    command: &str,
    syntax_errors: &[clearhead_cli::LintDiagnostic],
) {
    emit_event(
        Tool::Cli,
        None,
        TelemetryEvent::ParseFailed {
            file_path: path.display().to_string(),
            error_count: syntax_errors.len(),
            first_error_code: syntax_errors
                .first()
                .map(|d| d.code.clone())
                .unwrap_or_else(|| "syntax-error".to_string()),
        },
    )
    .unwrap_or_else(|e| warn!(error = %e, "Failed to emit parse_failed telemetry"));

    emit_event(
        Tool::Cli,
        None,
        TelemetryEvent::MutationSkippedDueToParse {
            command: command.to_string(),
            file_path: path.display().to_string(),
            error_count: syntax_errors.len(),
        },
    )
    .unwrap_or_else(
        |e| warn!(error = %e, "Failed to emit mutation_skipped_due_to_parse telemetry"),
    );

    eprintln!(
        "error: [{}] {} skipped due to parse issues; file not modified",
        path.display(),
        command
    );
    print_syntax_diagnostics(syntax_errors);
}

fn print_syntax_diagnostics(syntax_errors: &[clearhead_cli::LintDiagnostic]) {
    for diagnostic in syntax_errors.iter().take(5) {
        eprintln!(
            "  - line {}, col {}: {}",
            diagnostic.range.start_row + 1,
            diagnostic.range.start_col + 1,
            diagnostic.message
        );
    }

    let remaining = syntax_errors.len().saturating_sub(5);
    if remaining > 0 {
        eprintln!("  - ... and {} more issue(s)", remaining);
    }
}
