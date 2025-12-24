use config::{Config as ConfigBuilder, ConfigError, Environment, File};
use dirs::{config_dir, data_dir};
use serde::Deserialize;
use std::path::PathBuf;

/// Base configuration loaded from file and environment variables
/// Uses serde defaults for fallback values
#[derive(Debug, Deserialize, Clone)]
pub struct BaseConfig {
    /// Default output format (actions, json, xml, table)
    #[serde(default = "default_format")]
    pub format: String,

    /// Default file name (relative to data_dir)
    #[serde(default = "default_file")]
    pub file: String,
}

fn default_format() -> String {
    "actions".to_string()
}

fn default_file() -> String {
    "inbox.actions".to_string()
}

/// Resolved configuration with all values determined
/// This is what the application uses at runtime
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub format: clearhead_cli::OutputFormat,
    pub file: PathBuf,
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
}

/// Get XDG config directory for clearhead_cli
pub fn get_config_dir() -> PathBuf {
    config_dir()
        .expect("Failed to determine config directory")
        .join("clearhead_cli")
}

/// Get XDG data directory for clearhead_cli
pub fn get_data_dir() -> PathBuf {
    data_dir()
        .expect("Failed to determine data directory")
        .join("clearhead_cli")
}

/// Load base configuration from file and environment variables
///
/// Precedence (lowest to highest):
/// 1. Defaults (in code)
/// 2. Config file (~/.config/clearhead_cli/config.toml)
/// 3. Environment variables (CLICHE_FORMAT, CLICHE_FILE, etc.)
///
/// CLI arguments are applied later in main.rs (highest priority)
pub fn load_base_config(custom_config_path: Option<PathBuf>) -> Result<BaseConfig, ConfigError> {
    let config_path = custom_config_path.unwrap_or_else(|| get_config_dir().join("config.toml"));

    ConfigBuilder::builder()
        // 1. Defaults (lowest priority)
        .set_default("format", default_format())?
        .set_default("file", default_file())?
        // 2. Config file (overrides defaults)
        .add_source(File::from(config_path).required(false))
        // 3. Environment variables (highest priority before CLI)
        //    CLICHE_FORMAT=json, CLICHE_FILE=work.actions
        .add_source(Environment::with_prefix("CLICHE"))
        .build()?
        .try_deserialize()
}

/// Ensure a directory exists, creating it if necessary
pub fn ensure_dir_exists(path: &PathBuf) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}
