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

fn default_format() -> String {
    "actions".to_string()
}

fn default_indent_style() -> String {
    "spaces".to_string()
}

fn default_indent_width() -> usize {
    4
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

/// Load configuration with proper precedence:
/// defaults → global config → env vars
pub fn load_config(custom_config_path: Option<PathBuf>) -> Result<Config, ConfigError> {
    let global_config_path = custom_config_path
        .unwrap_or_else(|| get_config_dir().join("config.json"));

    ConfigBuilder::builder()
        // 1. Set defaults
        .set_default("data_dir", default_data_dir())?
        .set_default("config_dir", default_config_dir())?
        .set_default("default_file", default_file())?
        .set_default("cli_format", default_format())?
        .set_default("cli_indent_style", default_indent_style())?
        .set_default("cli_indent_width", default_indent_width() as i64)?
        // 2. Load global config (JSON format)
        .add_source(
            File::from(global_config_path)
                .format(FileFormat::Json)
                .required(false)
        )
        // 3. Load environment variables with CLEARHEAD_ prefix
        .add_source(
            Environment::with_prefix("CLEARHEAD")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true)
        )
        .build()?
        .try_deserialize()
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
