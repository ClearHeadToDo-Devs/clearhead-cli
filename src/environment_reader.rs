use config::{Config as ConfigBuilder, ConfigError, Environment, File, FileFormat};
use dirs::{config_dir, data_dir};
use serde::Deserialize;
use std::collections::HashMap;
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

    // Workspace identity (workspace_id, workspace_name, created_at) is NOT config —
    // it does not layer through the precedence chain. It lives in the workspace
    // manifest (.clearhead/workspace.json) and is read by core from the workspace
    // itself. See clearhead_core::workspace::manifest::WorkspaceManifest.

    // Additional workspace directories to include in multi-workspace queries.
    // Each entry may be an absolute path, a path starting with `~/` (expanded to
    // the user's home directory), a path with `$VAR` / `${VAR}` environment
    // variable references, or a path relative to the directory that contains
    // the config.json that declares it.
    // Resolved at `CommandContext` construction time via `resolve_workspace_paths`.
    #[serde(default)]
    pub additional_workspaces: Vec<String>,

    // Bypass project detection entirely and operate on the user workspace
    // only (specifications/configuration.md, Workspace Resolution). The one
    // sanctioned way to ignore an enclosing project.
    #[serde(default)]
    pub default_to_user_scope: bool,

    // Tag hierarchies for implicit inheritance
    // Maps parent tag -> list of child tags
    #[serde(default)]
    pub tag_hierarchies: HashMap<String, Vec<String>>,

    // Expansion: total instances generated per schedule across both files
    #[serde(default = "default_expansion_total_instances")]
    pub expansion_total_instances: u32,

    // Expansion: instances placed in the primary .actions file
    #[serde(default = "default_expansion_primary_instances")]
    pub expansion_primary_instances: u32,

    // Directory where plan .ics files are written, flat as
    // <plan_path>/<charter>/<uid>.ics. A CalDAV server can point at the same
    // directory to share plans. When None, plans live under <data_root>/plans.
    #[serde(default)]
    pub plan_path: Option<String>,

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

fn default_expansion_total_instances() -> u32 {
    2
}

fn default_expansion_primary_instances() -> u32 {
    1
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

/// Walk up from cwd looking for a `.clearhead/` directory.
/// Returns the first ancestor directory that contains `.clearhead/`, or `None`.
pub fn find_project_data_dir() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(".clearhead").is_dir() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Expand shell variables in a path string
fn expand_path(path: &str) -> PathBuf {
    if let Some(relative) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(relative);
    }

    // Handle environment variables like $HOME
    let expanded = shellexpand::env(path).unwrap_or_else(|_| path.into());
    PathBuf::from(expanded.as_ref())
}

/// Load configuration with proper precedence (lowest to highest):
/// defaults → global config → project config → project.local config → env vars
pub fn load_config(custom_config_path: Option<PathBuf>) -> Result<Config, ConfigError> {
    let global_config_path =
        custom_config_path.unwrap_or_else(|| get_config_dir().join("config.json"));

    // Project config lives at <project-root>/.clearhead/config.json and is
    // written when a team wants shared behavior. It overrides the global config
    // for workspace-specific settings (additional_workspaces, tag_hierarchies, …)
    // and is committed so the whole team shares it. Workspace identity is not
    // here — it lives in the sibling workspace.json manifest.
    //
    // Project-local config sits beside it at config.local.json: a git-ignored
    // personal override (e.g. one developer's own plan_path) that wins over the
    // committed project config. Both resolve from the same project root.
    let project_root = find_project_data_dir();
    let project_config_path = project_root
        .as_ref()
        .map(|root| root.join(".clearhead").join("config.json"));
    let project_local_config_path = project_root
        .as_ref()
        .map(|root| root.join(".clearhead").join("config.local.json"));

    let mut builder = ConfigBuilder::builder()
        // 1. Set defaults
        .set_default("data_dir", default_data_dir())?
        .set_default("config_dir", default_config_dir())?
        .set_default("default_file", default_file())?
        .set_default("cli_format", default_format())?
        .set_default("cli_indent_style", default_indent_style())?
        .set_default("cli_indent_width", default_indent_width() as i64)?
        .set_default(
            "expansion_total_instances",
            default_expansion_total_instances() as i64,
        )?
        .set_default(
            "expansion_primary_instances",
            default_expansion_primary_instances() as i64,
        )?
        // 2. Load global config (JSON format)
        .add_source(
            File::from(global_config_path)
                .format(FileFormat::Json)
                .required(false),
        );

    // 3. Layer in project config (higher priority than global)
    if let Some(project_cfg) = project_config_path {
        builder = builder.add_source(
            File::from(project_cfg)
                .format(FileFormat::Json)
                .required(false),
        );
    }

    // 4. Layer in project-local config (git-ignored personal override; wins
    //    over the committed project config)
    if let Some(project_local_cfg) = project_local_config_path {
        builder = builder.add_source(
            File::from(project_local_cfg)
                .format(FileFormat::Json)
                .required(false),
        );
    }

    builder
        // 5. Load environment variables with CLEARHEAD_ prefix (highest priority)
        .add_source(
            Environment::with_prefix("CLEARHEAD")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true),
        )
        .build()?
        .try_deserialize()
}

/// Resolve the config file path that would be loaded for this invocation.
pub fn resolve_config_path(custom_config_path: Option<PathBuf>) -> PathBuf {
    custom_config_path.unwrap_or_else(|| get_config_dir().join("config.json"))
}

/// Resolve a list of `additional_workspaces` path strings to absolute `PathBuf`s.
///
/// Each entry is processed in order:
/// 1. `~/…` — expanded to the user's home directory.
/// 2. `$VAR` / `${VAR}` — environment-variable references expanded by
///    [`shellexpand::env`].
/// 3. Absolute paths — returned as-is after variable expansion.
/// 4. Relative paths — joined onto `base`, which should be the directory that
///    contains the `config.json` file that declared the entry (for a project-
///    local config that is `<project-root>/.clearhead/`; for the global config
///    it is `~/.config/clearhead/`).
///
/// Entries whose resolved path does not exist are **included** in the output —
/// callers decide how to handle missing paths so that config errors surface at
/// the point of use rather than silently here.
pub fn resolve_workspace_paths(paths: &[String], base: &Path) -> Vec<PathBuf> {
    paths
        .iter()
        .map(|p| {
            let expanded = expand_path(p);
            if expanded.is_absolute() {
                expanded
            } else {
                base.join(expanded)
            }
        })
        .collect()
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
