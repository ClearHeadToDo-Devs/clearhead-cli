pub mod agenda;
pub mod charter;
pub mod complete;
pub mod file;
pub mod plan;
pub mod service;

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use tracing::warn;
use uuid::Uuid;

use crate::environment_reader::{Config, ensure_dir_exists, get_data_dir, load_config, resolve_file_path};
use clearhead_cli::crdt::{SyncRepo, load_action_repo};
use clearhead_core::workspace::actions::convert;
use clearhead_cli::telemetry::{Tool, TelemetryEvent, emit_event};
use clearhead_cli::ActionList;

/// Consolidated command context: config + resolved directories.
pub struct CommandContext {
    pub config: Config,
    pub data_dir: PathBuf,
    #[allow(dead_code)]
    pub config_dir: PathBuf,
}

impl CommandContext {
    pub fn new(cli: &crate::argparser::Cli) -> Result<Self, String> {
        let config = load_config(cli.config.clone())
            .map_err(|e| format!("Failed to load config: {}", e))?;

        let data_dir = resolve_file_path(&config.data_dir, &get_data_dir());
        let config_dir = resolve_file_path(&config.config_dir, &crate::environment_reader::get_config_dir());

        ensure_dir_exists(&data_dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
        ensure_dir_exists(&config_dir).map_err(|e| format!("Failed to create config dir: {}", e))?;

        Ok(Self { config, data_dir, config_dir })
    }

    /// Resolve an optional file arg against the default file from config.
    pub fn resolve_action_file(&self, file: Option<&PathBuf>) -> PathBuf {
        file.cloned()
            .unwrap_or_else(|| resolve_file_path(&self.config.default_file, &self.data_dir))
    }

    /// Resolve indent style and width from config.
    pub fn indent_config(&self) -> (clearhead_cli::IndentStyle, usize) {
        (parse_indent_style(&self.config.cli_indent_style), self.config.cli_indent_width)
    }
}

/// Load a SyncRepo and hydrate its ActionList.
///
/// If the CRDT has no actions for this file but the file exists on disk,
/// seeds the returned ActionList from the file content. The CRDT will be
/// updated when the caller calls `save_repo`.
pub fn load_repo(path: &Path) -> Result<(SyncRepo, ActionList), String> {
    let repo = load_action_repo(path)
        .map_err(|e| format!("Failed to load repository: {}", e))?;
    let model = repo
        .get_model()
        .map_err(|e| format!("Failed to hydrate actions: {}", e))?;
    let actions = convert::to_action_list(&model);

    // Seed from file if CRDT is empty (first access for this file)
    if actions.is_empty() && path.exists() {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;
        if !content.trim().is_empty() {
            let file_actions = clearhead_cli::parse_actions(&content)?;
            return Ok((repo, file_actions));
        }
    }

    Ok((repo, actions))
}

/// Save actions through the repository (CRDT persist).
pub fn save_repo(repo: &mut SyncRepo, actions: &ActionList) -> Result<(), String> {
    let model = convert::from_actions(actions);
    repo.save_model(&model)
        .map_err(|e| format!("Failed to save repository: {}", e))
}

/// Write content to a file if `write` is true, otherwise print to stdout.
pub fn write_or_print(content: &str, write: bool, file: Option<&PathBuf>) -> Result<(), String> {
    if write {
        let path = file.ok_or("Cannot use --write without specifying a file")?;
        fs::write(path, content)
            .map_err(|e| format!("Failed to write to file: {}", e))
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

/// Parse format string to OutputFormat
pub fn parse_format(s: &str) -> Result<clearhead_cli::OutputFormat, String> {
    match s.to_lowercase().as_str() {
        "actions" => Ok(clearhead_cli::OutputFormat::Actions),
        "json" => Ok(clearhead_cli::OutputFormat::Json),
        "xml" => Ok(clearhead_cli::OutputFormat::Xml),
        "table" => Ok(clearhead_cli::OutputFormat::Table),
        _ => Err(format!("Unknown format: {}", s)),
    }
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

fn parse_indent_style(s: &str) -> clearhead_cli::IndentStyle {
    match s.to_lowercase().as_str() {
        "tabs" => clearhead_cli::IndentStyle::Tabs,
        _ => clearhead_cli::IndentStyle::Spaces,
    }
}
