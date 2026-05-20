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

    // Stable UUID identifying this workspace's RDF named graph.
    // Generated once by `clearhead init` and written to config.json.
    #[serde(default)]
    pub workspace_id: Option<String>,

    // Human-readable name for this workspace, derived from the project directory
    // on `clearhead init`. Used as outer scope in multi-workspace reference syntax.
    #[serde(default)]
    pub workspace_name: Option<String>,

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

    // CLI-specific settings (cli_ prefix)
    #[serde(default = "default_format")]
    pub cli_format: String,

    #[serde(default = "default_indent_style")]
    pub cli_indent_style: String,

    #[serde(default = "default_indent_width")]
    pub cli_indent_width: usize,
}

impl Config {
    /// Get all ancestor tags for a given tag (transitive hierarchy traversal)
    /// Returns tags in order from immediate parent to root
    pub fn get_tag_ancestors(&self, tag: &str) -> Vec<String> {
        let tag_lower = tag.to_lowercase();
        let mut ancestors = Vec::new();
        let mut visited = std::collections::HashSet::new();

        // Build reverse mapping: child -> parent
        let mut child_to_parent: HashMap<String, String> = HashMap::new();
        for (parent, children) in &self.tag_hierarchies {
            let parent_lower = parent.to_lowercase();
            for child in children {
                child_to_parent.insert(child.to_lowercase(), parent_lower.clone());
            }
        }

        // Walk up the hierarchy
        let mut current = tag_lower;
        while let Some(parent) = child_to_parent.get(&current) {
            if !visited.insert(parent.clone()) {
                break; // Cycle detected
            }
            ancestors.push(parent.clone());
            current = parent.clone();
        }

        ancestors
    }

    /// Expand a tag to include itself and all ancestor tags
    pub fn expand_tag(&self, tag: &str) -> Vec<String> {
        let mut expanded = vec![tag.to_lowercase()];
        expanded.extend(self.get_tag_ancestors(tag));
        expanded
    }

    /// Expand all tags in a list to include ancestor tags
    pub fn expand_tags(&self, tags: &[String]) -> Vec<String> {
        let mut all_tags: std::collections::HashSet<String> = std::collections::HashSet::new();
        for tag in tags {
            for expanded in self.expand_tag(tag) {
                all_tags.insert(expanded);
            }
        }
        all_tags.into_iter().collect()
    }
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
    let global_config_path =
        custom_config_path.unwrap_or_else(|| get_config_dir().join("config.json"));

    ConfigBuilder::builder()
        // 1. Set defaults
        .set_default("data_dir", default_data_dir())?
        .set_default("config_dir", default_config_dir())?
        .set_default("default_file", default_file())?
        .set_default("cli_format", default_format())?
        .set_default("cli_indent_style", default_indent_style())?
        .set_default("cli_indent_width", default_indent_width() as i64)?
        .set_default("expansion_total_instances", default_expansion_total_instances() as i64)?
        .set_default("expansion_primary_instances", default_expansion_primary_instances() as i64)?
        // 2. Load global config (JSON format)
        .add_source(
            File::from(global_config_path)
                .format(FileFormat::Json)
                .required(false),
        )
        // 3. Load environment variables with CLEARHEAD_ prefix
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_config_with_hierarchies() -> Config {
        let mut tag_hierarchies = HashMap::new();
        // computer -> terminal -> neovim
        //          -> browser
        tag_hierarchies.insert(
            "computer".to_string(),
            vec!["terminal".to_string(), "browser".to_string()],
        );
        tag_hierarchies.insert(
            "terminal".to_string(),
            vec!["neovim".to_string(), "tmux".to_string()],
        );
        // driving -> grocery_store
        tag_hierarchies.insert("driving".to_string(), vec!["grocery_store".to_string()]);

        Config {
            data_dir: String::new(),
            config_dir: String::new(),
            default_file: String::new(),
            workspace_id: None,
            workspace_name: None,
            tag_hierarchies,
            cli_format: String::new(),
            cli_indent_style: String::new(),
            cli_indent_width: 4,
            expansion_total_instances: 2,
            expansion_primary_instances: 1,
        }
    }

    #[test]
    fn test_get_tag_ancestors_single_level() {
        let config = make_config_with_hierarchies();

        // terminal's parent is computer
        let ancestors = config.get_tag_ancestors("terminal");
        assert_eq!(ancestors, vec!["computer"]);

        // browser's parent is computer
        let ancestors = config.get_tag_ancestors("browser");
        assert_eq!(ancestors, vec!["computer"]);
    }

    #[test]
    fn test_get_tag_ancestors_multi_level() {
        let config = make_config_with_hierarchies();

        // neovim -> terminal -> computer
        let ancestors = config.get_tag_ancestors("neovim");
        assert_eq!(ancestors, vec!["terminal", "computer"]);

        // tmux -> terminal -> computer
        let ancestors = config.get_tag_ancestors("tmux");
        assert_eq!(ancestors, vec!["terminal", "computer"]);
    }

    #[test]
    fn test_get_tag_ancestors_root_tag() {
        let config = make_config_with_hierarchies();

        // computer has no parent
        let ancestors = config.get_tag_ancestors("computer");
        assert!(ancestors.is_empty());
    }

    #[test]
    fn test_get_tag_ancestors_unknown_tag() {
        let config = make_config_with_hierarchies();

        // unknown tag has no ancestors
        let ancestors = config.get_tag_ancestors("unknown");
        assert!(ancestors.is_empty());
    }

    #[test]
    fn test_get_tag_ancestors_case_insensitive() {
        let config = make_config_with_hierarchies();

        // Should work regardless of case
        let ancestors = config.get_tag_ancestors("NEOVIM");
        assert_eq!(ancestors, vec!["terminal", "computer"]);

        let ancestors = config.get_tag_ancestors("Terminal");
        assert_eq!(ancestors, vec!["computer"]);
    }

    #[test]
    fn test_expand_tag() {
        let config = make_config_with_hierarchies();

        // neovim expands to [neovim, terminal, computer]
        let mut expanded = config.expand_tag("neovim");
        expanded.sort(); // Sort for consistent comparison
        assert_eq!(expanded, vec!["computer", "neovim", "terminal"]);

        // computer expands to just [computer] (no ancestors)
        let expanded = config.expand_tag("computer");
        assert_eq!(expanded, vec!["computer"]);
    }

    #[test]
    fn test_expand_tags_multiple() {
        let config = make_config_with_hierarchies();

        // Expanding [neovim, grocery_store] should give all ancestors of both
        let mut expanded = config.expand_tags(&["neovim".to_string(), "grocery_store".to_string()]);
        expanded.sort();

        // neovim -> terminal, computer
        // grocery_store -> driving
        // Combined: computer, driving, grocery_store, neovim, terminal
        assert_eq!(
            expanded,
            vec!["computer", "driving", "grocery_store", "neovim", "terminal"]
        );
    }
}
