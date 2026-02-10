pub mod agenda;
pub mod complete;

use std::fs;
use std::path::{Path, PathBuf};
use tracing::warn;
use uuid::Uuid;

use crate::environment_reader::{Config, ensure_dir_exists, get_data_dir, load_config, resolve_file_path};
use clearhead_cli::crdt::ActionRepository;
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
    /// Used by 7+ commands: Agenda, Archive, SyncEvents, Add, Complete, Update, Delete.
    pub fn resolve_action_file(&self, file: Option<&PathBuf>) -> PathBuf {
        file.cloned()
            .unwrap_or_else(|| resolve_file_path(&self.config.default_file, &self.data_dir))
    }

    /// Resolve indent style and width from config.
    pub fn indent_config(&self) -> (clearhead_cli::IndentStyle, usize) {
        (parse_indent_style(&self.config.cli_indent_style), self.config.cli_indent_width)
    }
}

/// Load an ActionRepository and hydrate its ActionList.
pub fn load_repo(path: &Path) -> Result<(ActionRepository, ActionList), String> {
    let repo = ActionRepository::load(path.to_path_buf())
        .map_err(|e| format!("Failed to load repository: {}", e))?;
    let actions = repo
        .get_actions()
        .map_err(|e| format!("Failed to hydrate actions: {}", e))?;
    Ok((repo, actions))
}

/// Save actions through the repository (CRDT persist + file projection).
pub fn save_repo(repo: &mut ActionRepository, actions: &ActionList) -> Result<(), String> {
    repo.save(actions)
        .map_err(|e| format!("Failed to save repository: {}", e))?;
    Ok(())
}

/// Write content to a file if `write` is true, otherwise print to stdout.
/// Returns an error if `write` is true but no file path was provided.
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

fn parse_indent_style(s: &str) -> clearhead_cli::IndentStyle {
    match s.to_lowercase().as_str() {
        "tabs" => clearhead_cli::IndentStyle::Tabs,
        _ => clearhead_cli::IndentStyle::Spaces,
    }
}
