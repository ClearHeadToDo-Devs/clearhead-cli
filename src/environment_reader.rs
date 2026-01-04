use config::{Config as ConfigBuilder, ConfigError, Environment, File, FileFormat};
use dirs::{config_dir, data_dir};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Configuration loaded from file and environment variables
/// Uses flat structure with cli_ prefix for implementation-specific settings
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    // Core settings (no prefix)
    #[serde(default = "default_data_dir")]
    pub data_dir: String,

    #[serde(default = "default_config_dir")]
    pub config_dir: String,

    #[serde(default = "default_file")]
    pub default_file: String,

    #[serde(default = "default_project_files")]
    pub project_files: Vec<String>,

    #[serde(default = "default_use_project_config")]
    pub use_project_config: bool,

    // CLI-specific settings (cli_ prefix)
    #[serde(default = "default_format")]
    pub cli_format: String,

    #[serde(default = "default_indent_style")]
    pub cli_indent_style: String,

    #[serde(default = "default_indent_width")]
    pub cli_indent_width: usize,
}

// Default functions
// Empty string means "use XDG defaults"
fn default_data_dir() -> String {
    String::new()
}

fn default_config_dir() -> String {
    String::new()
}

fn default_file() -> String {
    "inbox.actions".to_string()
}

fn default_project_files() -> Vec<String> {
    vec!["next.actions".to_string()]
}

fn default_use_project_config() -> bool {
    true
}

fn default_format() -> String {
    "actions".to_string()
}

fn default_indent_style() -> String {
    "spaces".to_string()
}

fn default_indent_width() -> usize {
    4
}

/// Discovered project context
#[derive(Debug, Clone)]
pub struct ProjectContext {
    pub root: PathBuf,
    pub config_path: Option<PathBuf>,
    pub default_file: Option<PathBuf>,
}

/// Get XDG config directory for clearhead
pub fn get_config_dir() -> PathBuf {
    config_dir()
        .expect("Failed to determine config directory")
        .join("clearhead")
}

/// Get XDG data directory for clearhead
pub fn get_data_dir() -> PathBuf {
    data_dir()
        .expect("Failed to determine data directory")
        .join("clearhead")
}

/// Expand shell variables in a path string
fn expand_path(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }

    // Handle environment variables like $HOME
    let expanded = shellexpand::env(path).unwrap_or_else(|_| path.into());
    PathBuf::from(expanded.as_ref())
}

/// Search upward for a project root (.clearhead/ or files matching project_files)
pub fn discover_project_root(project_files: &[String]) -> Option<ProjectContext> {
    let mut current = std::env::current_dir().ok()?;

    loop {
        // 1. Check for .clearhead/
        let dot_clearhead = current.join(".clearhead");
        if dot_clearhead.is_dir() {
            return Some(ProjectContext {
                root: current.clone(),
                config_path: {
                    let config = dot_clearhead.join("config.json");
                    if config.exists() {
                        Some(config)
                    } else {
                        None
                    }
                },
                default_file: Some(dot_clearhead.join("inbox.actions")),
            });
        }

        // 2. Check for any of the configured project files
        for project_file in project_files {
            let file_path = current.join(project_file);
            if file_path.is_file() {
                return Some(ProjectContext {
                    root: current.clone(),
                    config_path: {
                        // Check if .clearhead/config.json exists even without .clearhead/ being the trigger
                        let config = current.join(".clearhead/config.json");
                        if config.exists() {
                            Some(config)
                        } else {
                            None
                        }
                    },
                    default_file: Some(file_path),
                });
            }
        }

        if !current.pop() { break; }
    }
    None
}

/// Load partial configuration (without project config) to get project_files for discovery
/// This is phase 1 of config loading
fn load_partial_config(custom_config_path: Option<PathBuf>) -> Result<Config, ConfigError> {
    let global_config_path = custom_config_path
        .unwrap_or_else(|| get_config_dir().join("config.json"));

    ConfigBuilder::builder()
        .set_default("data_dir", default_data_dir())?
        .set_default("config_dir", default_config_dir())?
        .set_default("default_file", default_file())?
        .set_default("project_files", default_project_files())?
        .set_default("use_project_config", default_use_project_config())?
        .set_default("cli_format", default_format())?
        .set_default("cli_indent_style", default_indent_style())?
        .set_default("cli_indent_width", default_indent_width() as i64)?
        .add_source(
            File::from(global_config_path)
                .format(FileFormat::Json)
                .required(false)
        )
        .add_source(
            Environment::with_prefix("CLEARHEAD")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true)
        )
        .build()?
        .try_deserialize()
}

/// Load complete configuration with proper precedence:
/// defaults → global config → project config → env vars
///
/// This is a two-phase process:
/// 1. Load partial config to get project_files
/// 2. Discover project using those files
/// 3. Reload full config including project config
pub fn load_config_with_project_discovery(
    custom_config_path: Option<PathBuf>
) -> Result<(Config, Option<ProjectContext>), ConfigError> {
    // Phase 1: Load partial config to get project_files
    let partial_config = load_partial_config(custom_config_path.clone())?;

    // Phase 2: Discover project using project_files from config
    let project_context = discover_project_root(&partial_config.project_files);

    // Phase 3: Reload full config including project config
    let full_config = load_config_internal(custom_config_path, &project_context)?;

    Ok((full_config, project_context))
}

/// Internal function to load configuration with known project context
fn load_config_internal(
    custom_config_path: Option<PathBuf>,
    project_context: &Option<ProjectContext>
) -> Result<Config, ConfigError> {
    let global_config_path = custom_config_path
        .unwrap_or_else(|| get_config_dir().join("config.json"));

    let mut builder = ConfigBuilder::builder()
        // 1. Set defaults
        .set_default("data_dir", default_data_dir())?
        .set_default("config_dir", default_config_dir())?
        .set_default("default_file", default_file())?
        .set_default("project_files", default_project_files())?
        .set_default("use_project_config", default_use_project_config())?
        .set_default("cli_format", default_format())?
        .set_default("cli_indent_style", default_indent_style())?
        .set_default("cli_indent_width", default_indent_width() as i64)?;

    // 2. Load global config (JSON format)
    builder = builder.add_source(
        File::from(global_config_path)
            .format(FileFormat::Json)
            .required(false)
    );

    // 3. Load project config (if it exists)
    if let Some(ctx) = project_context {
        if let Some(ref project_config_path) = ctx.config_path {
            builder = builder.add_source(
                File::from(project_config_path.clone())
                    .format(FileFormat::Json)
                    .required(false)
            );
        }
    }

    // 4. Load environment variables with CLEARHEAD_ prefix
    // CLEARHEAD_CLI_FORMAT -> cli_format (keep underscores in field names)
    builder = builder.add_source(
        Environment::with_prefix("CLEARHEAD")
            .prefix_separator("_")
            .separator("__") // Use double underscore for nesting (we don't have nested fields anyway)
            .try_parsing(true)
    );

    builder.build()?.try_deserialize()
}

/// Ensure a directory exists, creating it if necessary
pub fn ensure_dir_exists(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Resolve a file path (handling ~/ expansion and relative vs absolute)
/// If file is empty, returns the fallback directory
pub fn resolve_file_path(file: &str, fallback: &Path) -> PathBuf {
    if file.is_empty() {
        return fallback.to_path_buf();
    }

    let expanded = expand_path(file);

    if expanded.is_absolute() {
        expanded
    } else {
        fallback.join(expanded)
    }
}
