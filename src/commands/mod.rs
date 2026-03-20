pub mod act;
pub mod agenda;
pub mod charter;
pub mod complete;
pub mod file;
pub mod plan;
pub mod query;
pub mod resolver;
pub mod service;

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use tracing::warn;
use uuid::Uuid;

use crate::environment_reader::{Config, ensure_dir_exists, get_data_dir, load_config, resolve_file_path};
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

/// Load actions from a .actions file on disk.
pub fn load_file(path: &Path) -> Result<ActionList, String> {
    if path.exists() {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read file '{}': {}", path.display(), e))?;
        clearhead_cli::parse_actions(&content)
    } else {
        Ok(ActionList::new())
    }
}

/// Format actions and write to a .actions file on disk.
pub fn save_file(path: &Path, actions: &ActionList) -> Result<(), String> {
    let content = clearhead_cli::format(actions, clearhead_cli::OutputFormat::Actions, None, None)?;
    fs::write(path, content)
        .map_err(|e| format!("Failed to write file '{}': {}", path.display(), e))
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

/// Search all workspace .actions files for a plan matching `query`.
///
/// Returns the resolved file path and the full ActionList from that file.
/// Used by mutating commands (update, complete, delete) when no `-f` is given,
/// so they operate on the correct file rather than silently defaulting to inbox.
pub fn find_plan_file(
    data_dir: &Path,
    query: &str,
) -> Result<(PathBuf, ActionList), String> {
    use clearhead_core::{FsWorkspaceStore, WorkspaceStore};

    let store = FsWorkspaceStore::new(data_dir);
    let objectives = store
        .list_objectives()
        .map_err(|e| format!("Failed to list workspace: {}", e))?;

    for obj in &objectives {
        let file_path = data_dir.join(&obj.key);
        let actions = load_file(&file_path)?;
        if clearhead_cli::resolve_reference(&actions, query).is_some() {
            return Ok((file_path, actions));
        }
    }

    Err(format!("No plan found matching '{}'", query))
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

/// Resolve a charter query to the primary `.actions` file for that charter.
///
/// Mapping:
/// - `<dir>/README.md` charter → `<dir>/next.actions`
/// - `<name>.md` root charter → `<name>.actions`
/// - `<dir>/<name>.md` sub-charter → `<dir>/<name>.actions`
/// - implicit from `.actions` file → that file
/// - implicit from directory → `<dir>/next.actions`
pub fn charter_to_file_path(
    data_dir: &Path,
    charter_query: &str,
) -> Result<PathBuf, String> {
    use clearhead_core::{FsWorkspaceStore, WorkspaceStore};

    let store = FsWorkspaceStore::new(data_dir);
    let charters = store
        .discover_charters()
        .map_err(|e| e.to_string())?;

    let found = crate::commands::charter::resolve_discovered_charter(&charters, charter_query)
        .ok_or_else(|| format!("No charter found matching '{}'", charter_query))?;

    let source = Path::new(&found.source_key);
    let actions_path = if source.file_name().map(|n| n == "README.md").unwrap_or(false) {
        // build_clearhead/README.md → build_clearhead/next.actions
        let dir = source.parent().unwrap_or(Path::new(""));
        dir.join("next.actions")
    } else if source.extension().map(|e| e == "md").unwrap_or(false) {
        // health.md → health.actions
        // build_clearhead/observability.md → build_clearhead/observability.actions
        source.with_extension("actions")
    } else if source.extension().map(|e| e == "actions").unwrap_or(false) {
        // implicit from .actions file
        source.to_path_buf()
    } else {
        // implicit from directory name
        source.join("next.actions")
    };

    Ok(data_dir.join(actions_path))
}

fn parse_indent_style(s: &str) -> clearhead_cli::IndentStyle {
    match s.to_lowercase().as_str() {
        "tabs" => clearhead_cli::IndentStyle::Tabs,
        _ => clearhead_cli::IndentStyle::Spaces,
    }
}
