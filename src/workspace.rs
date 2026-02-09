//! Workspace module for discovering and loading action files
//!
//! This module provides workspace-wide operations:
//! - Discovering all .actions files in a data directory
//! - Loading and aggregating actions from multiple files
//! - Tracking file source metadata for cross-file queries
//!
//! # Architecture
//!
//! The workspace follows the naming conventions from the specification:
//! - Root level files: `<project-name>.actions` (single-file projects)
//! - Project directories: `<project-name>/next.actions` (multi-file projects)
//! - Archives: `<project>/logs/<YYYY-MM>.actions` (completed items)
//!
//! See: specifications/naming_conventions.md

use crate::get_action_list_struct;
use clearhead_core::{Action, ActionList, Charter, parse_charter, implicit_charter};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Metadata about an action's source file
#[derive(Debug, Clone)]
pub struct ActionSource {
    /// Path to the source file (relative to workspace root)
    pub file_path: PathBuf,
    /// Inferred project name from file/directory structure
    pub project: Option<String>,
}

/// An action with its source metadata
#[derive(Debug, Clone)]
pub struct SourcedAction {
    pub action: Action,
    pub source: ActionSource,
}

/// Collection of actions from multiple files with source tracking
#[derive(Debug, Default)]
pub struct WorkspaceActions {
    /// All actions with their source metadata
    pub sourced_actions: Vec<SourcedAction>,
}

impl WorkspaceActions {
    /// Create a new empty workspace
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a flat list of all actions (without source metadata)
    pub fn to_action_list(&self) -> ActionList {
        self.sourced_actions
            .iter()
            .map(|sa| sa.action.clone())
            .collect()
    }

    /// Add actions from a file
    pub fn add_from_file(&mut self, file_path: &Path, actions: ActionList, workspace_root: &Path) {
        let relative_path = file_path
            .strip_prefix(workspace_root)
            .unwrap_or(file_path)
            .to_path_buf();

        let project = infer_project_name(&relative_path);

        for action in actions {
            self.sourced_actions.push(SourcedAction {
                action,
                source: ActionSource {
                    file_path: relative_path.clone(),
                    project: project.clone(),
                },
            });
        }
    }

    /// Get the number of actions
    pub fn len(&self) -> usize {
        self.sourced_actions.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.sourced_actions.is_empty()
    }
}

/// Infer project name from file path
///
/// Rules:
/// - `project.actions` -> project
/// - `project/next.actions` -> project
/// - `project/logs/2026-01.actions` -> project
/// - `inbox.actions` -> None (not a project)
fn infer_project_name(relative_path: &Path) -> Option<String> {
    let components: Vec<_> = relative_path.components().collect();

    if components.is_empty() {
        return None;
    }

    // Single file at root: check if it's a project file
    if components.len() == 1 {
        let filename = relative_path.file_stem()?.to_str()?;
        // inbox is special - not a project
        if filename == "inbox" {
            return None;
        }
        return Some(filename.to_string());
    }

    // Nested file: first directory is the project
    let first = components.first()?;
    if let std::path::Component::Normal(name) = first {
        return name.to_str().map(String::from);
    }

    None
}

/// Discover all .actions files in a directory (recursively)
///
/// Follows the workspace structure:
/// - Root level .actions files
/// - Project subdirectories with next.actions
/// - Log directories with archived actions
pub fn discover_action_files(data_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    discover_recursive(data_dir, &mut files)?;

    // Sort for consistent ordering (by path)
    files.sort();

    debug!(count = files.len(), data_dir = %data_dir.display(), "Discovered action files");
    Ok(files)
}

fn discover_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }

    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory '{}': {}", dir.display(), e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_dir() {
            // Skip hidden directories (like .clearhead)
            if let Some(name) = path.file_name() {
                if name.to_string_lossy().starts_with('.') {
                    continue;
                }
            }
            // Recurse into subdirectories
            discover_recursive(&path, files)?;
        } else if path.is_file() {
            // Check for .actions extension
            if let Some(ext) = path.extension() {
                if ext == "actions" {
                    files.push(path);
                }
            }
        }
    }

    Ok(())
}

/// Load all actions from a workspace directory
///
/// This is the main entry point for workspace-wide reads.
/// It discovers all .actions files, parses them, and aggregates
/// the results into a single ActionList.
///
/// # Arguments
/// * `data_dir` - The workspace root directory (typically XDG_DATA_HOME/clearhead)
///
/// # Returns
/// A flat ActionList containing all actions from all files
pub fn load_workspace_actions(data_dir: &Path) -> Result<ActionList, String> {
    let files = discover_action_files(data_dir)?;

    if files.is_empty() {
        debug!(data_dir = %data_dir.display(), "No action files found in workspace");
        return Ok(Vec::new());
    }

    let mut workspace = WorkspaceActions::new();

    for file_path in &files {
        match load_file_actions(file_path) {
            Ok(actions) => {
                debug!(
                    file = %file_path.display(),
                    count = actions.len(),
                    "Loaded actions from file"
                );
                workspace.add_from_file(file_path, actions, data_dir);
            }
            Err(e) => {
                // Log warning but continue with other files
                warn!(
                    file = %file_path.display(),
                    error = %e,
                    "Failed to parse action file, skipping"
                );
            }
        }
    }

    debug!(
        total_actions = workspace.len(),
        total_files = files.len(),
        "Workspace loaded"
    );

    Ok(workspace.to_action_list())
}

/// Load actions from a single file
fn load_file_actions(file_path: &Path) -> Result<ActionList, String> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read '{}': {}", file_path.display(), e))?;

    get_action_list_struct(&serde_json::json!({}), &content)
}

/// Load workspace actions with source tracking
///
/// Like `load_workspace_actions` but preserves source metadata
/// for cross-file queries and filtering.
pub fn load_workspace_with_sources(data_dir: &Path) -> Result<WorkspaceActions, String> {
    let files = discover_action_files(data_dir)?;
    let mut workspace = WorkspaceActions::new();

    for file_path in &files {
        match load_file_actions(file_path) {
            Ok(actions) => {
                workspace.add_from_file(file_path, actions, data_dir);
            }
            Err(e) => {
                warn!(
                    file = %file_path.display(),
                    error = %e,
                    "Failed to parse action file, skipping"
                );
            }
        }
    }

    Ok(workspace)
}

// ============================================================================
// Charter Discovery
// ============================================================================

/// A charter with metadata about how it was discovered.
#[derive(Debug, Clone)]
pub struct SourcedCharter {
    pub charter: Charter,
    pub source: CharterSource,
}

/// How a charter was discovered in the workspace.
#[derive(Debug, Clone)]
pub enum CharterSource {
    /// Parsed from an explicit .md file (e.g., health.md)
    ExplicitFile(PathBuf),
    /// Inferred from a .actions filename (e.g., health.actions → "health")
    ImplicitFromFile(PathBuf),
    /// Inferred from a project directory (e.g., build_clearhead/ → "build_clearhead")
    ImplicitFromDirectory(PathBuf),
}

impl SourcedCharter {
    pub fn is_explicit(&self) -> bool {
        matches!(self.source, CharterSource::ExplicitFile(_))
    }
}

/// Full workspace model composing charters and actions.
#[derive(Debug)]
pub struct Workspace {
    pub charters: Vec<SourcedCharter>,
    pub actions: WorkspaceActions,
}

/// Discover all charters in a data directory.
///
/// Discovery order:
/// 1. Explicit: .md files at root level, README.md in project directories
/// 2. Implicit: inferred from .actions filenames and project directories
///
/// Implicit charters are deduplicated — if an explicit charter exists with
/// the same name, the implicit one is skipped.
pub fn discover_charters(data_dir: &Path) -> Result<Vec<SourcedCharter>, String> {
    let mut charters = Vec::new();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    if !data_dir.is_dir() {
        return Ok(charters);
    }

    let entries = std::fs::read_dir(data_dir)
        .map_err(|e| format!("Failed to read directory '{}': {}", data_dir.display(), e))?;

    let mut root_md_files = Vec::new();
    let mut root_actions_files = Vec::new();
    let mut directories = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(name) = path.file_name() {
                if !name.to_string_lossy().starts_with('.') {
                    directories.push(path);
                }
            }
        } else if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "md" {
                    root_md_files.push(path);
                } else if ext == "actions" {
                    root_actions_files.push(path);
                }
            }
        }
    }

    // Phase 1: Explicit charters from .md files at root
    for md_path in &root_md_files {
        match load_explicit_charter(md_path) {
            Ok(charter) => {
                let name = charter.title.to_lowercase();
                seen_names.insert(name);
                if let Some(ref alias) = charter.alias {
                    seen_names.insert(alias.to_lowercase());
                }
                // Also track the file stem so health.md deduplicates health.actions
                if let Some(stem) = md_path.file_stem().and_then(|s| s.to_str()) {
                    seen_names.insert(stem.to_lowercase());
                }
                charters.push(SourcedCharter {
                    charter,
                    source: CharterSource::ExplicitFile(md_path.clone()),
                });
            }
            Err(e) => {
                debug!(file = %md_path.display(), error = %e, "Skipping .md file (not a valid charter)");
            }
        }
    }

    // Phase 1b: Explicit charters from README.md in project directories
    for dir_path in &directories {
        let readme = dir_path.join("README.md");
        if readme.is_file() {
            match load_explicit_charter(&readme) {
                Ok(charter) => {
                    let dir_name = dir_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    let name = charter.title.to_lowercase();
                    seen_names.insert(name);
                    seen_names.insert(dir_name.to_lowercase());
                    if let Some(ref alias) = charter.alias {
                        seen_names.insert(alias.to_lowercase());
                    }
                    charters.push(SourcedCharter {
                        charter,
                        source: CharterSource::ExplicitFile(readme),
                    });
                }
                Err(e) => {
                    debug!(dir = %dir_path.display(), error = %e, "Skipping README.md (not a valid charter)");
                }
            }
        }

        // Check for sub-directory charters
        if let Ok(sub_entries) = std::fs::read_dir(dir_path) {
            let parent_name = dir_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            for sub_entry in sub_entries.flatten() {
                let sub_path = sub_entry.path();
                if sub_path.is_dir() {
                    if let Some(name) = sub_path.file_name() {
                        if name.to_string_lossy().starts_with('.') {
                            continue;
                        }
                    }
                    // Check for .actions files in subdirectory to infer sub-charters
                    let has_actions = sub_path.join("next.actions").is_file()
                        || std::fs::read_dir(&sub_path)
                            .ok()
                            .map(|entries| {
                                entries
                                    .flatten()
                                    .any(|e| {
                                        e.path().extension().is_some_and(|ext| ext == "actions")
                                    })
                            })
                            .unwrap_or(false);

                    if has_actions {
                        let sub_name = sub_path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string();
                        if !seen_names.contains(&sub_name.to_lowercase()) {
                            let mut sub_charter = implicit_charter(&sub_name);
                            sub_charter.parent = Some(parent_name.clone());
                            seen_names.insert(sub_name.to_lowercase());
                            charters.push(SourcedCharter {
                                charter: sub_charter,
                                source: CharterSource::ImplicitFromDirectory(sub_path),
                            });
                        }
                    }
                }
            }
        }
    }

    // Phase 2: Implicit charters from .actions files
    for actions_path in &root_actions_files {
        if let Some(stem) = actions_path.file_stem().and_then(|s| s.to_str()) {
            if stem == "inbox" {
                continue;
            }
            if !seen_names.contains(&stem.to_lowercase()) {
                seen_names.insert(stem.to_lowercase());
                charters.push(SourcedCharter {
                    charter: implicit_charter(stem),
                    source: CharterSource::ImplicitFromFile(actions_path.clone()),
                });
            }
        }
    }

    // Phase 2b: Implicit charters from directories (that don't already have explicit charters)
    for dir_path in &directories {
        if let Some(dir_name) = dir_path.file_name().and_then(|n| n.to_str()) {
            if !seen_names.contains(&dir_name.to_lowercase()) {
                // Only create implicit charter if directory has .actions files
                let has_actions = discover_action_files(dir_path)
                    .map(|f| !f.is_empty())
                    .unwrap_or(false);
                if has_actions {
                    seen_names.insert(dir_name.to_lowercase());
                    charters.push(SourcedCharter {
                        charter: implicit_charter(dir_name),
                        source: CharterSource::ImplicitFromDirectory(dir_path.clone()),
                    });
                }
            }
        }
    }

    // Sort by title for consistent ordering
    charters.sort_by(|a, b| a.charter.title.cmp(&b.charter.title));

    debug!(count = charters.len(), data_dir = %data_dir.display(), "Discovered charters");
    Ok(charters)
}

/// Load workspace charters + actions together.
pub fn load_workspace(data_dir: &Path) -> Result<Workspace, String> {
    let charters = discover_charters(data_dir)?;
    let actions = load_workspace_with_sources(data_dir)?;
    Ok(Workspace { charters, actions })
}

/// Resolve a charter by UUID prefix, alias, or name.
///
/// Resolution order:
/// 1. Full UUID match
/// 2. Short UUID prefix match
/// 3. Alias match (case-insensitive)
/// 4. Title match (case-insensitive, partial)
pub fn resolve_charter<'a>(
    charters: &'a [SourcedCharter],
    query: &str,
) -> Option<&'a SourcedCharter> {
    let query_lower = query.to_lowercase();

    // 1. Full UUID match
    if let Ok(uuid) = uuid::Uuid::parse_str(query) {
        if let Some(sc) = charters.iter().find(|sc| sc.charter.id == uuid) {
            return Some(sc);
        }
    }

    // 2. Short UUID prefix
    if query.len() >= 4 && query.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        if let Some(sc) = charters
            .iter()
            .find(|sc| sc.charter.id.to_string().starts_with(query))
        {
            return Some(sc);
        }
    }

    // 3. Alias match (case-insensitive, exact)
    if let Some(sc) = charters.iter().find(|sc| {
        sc.charter
            .alias
            .as_ref()
            .is_some_and(|a| a.to_lowercase() == query_lower)
    }) {
        return Some(sc);
    }

    // 4. Title match (case-insensitive, partial)
    charters
        .iter()
        .find(|sc| sc.charter.title.to_lowercase().contains(&query_lower))
}

/// Load an explicit charter from a markdown file.
fn load_explicit_charter(path: &Path) -> Result<Charter, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read '{}': {}", path.display(), e))?;
    parse_charter(&content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_infer_project_name_root_file() {
        assert_eq!(
            infer_project_name(Path::new("work.actions")),
            Some("work".to_string())
        );
        assert_eq!(infer_project_name(Path::new("inbox.actions")), None); // special case
    }

    #[test]
    fn test_infer_project_name_nested() {
        assert_eq!(
            infer_project_name(Path::new("myproject/next.actions")),
            Some("myproject".to_string())
        );
        assert_eq!(
            infer_project_name(Path::new("myproject/logs/2026-01.actions")),
            Some("myproject".to_string())
        );
    }

    #[test]
    fn test_discover_action_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create test structure
        fs::write(root.join("inbox.actions"), "[ ] Task 1").unwrap();
        fs::write(root.join("work.actions"), "[ ] Task 2").unwrap();

        let project_dir = root.join("project1");
        fs::create_dir(&project_dir).unwrap();
        fs::write(project_dir.join("next.actions"), "[ ] Task 3").unwrap();

        let logs_dir = project_dir.join("logs");
        fs::create_dir(&logs_dir).unwrap();
        fs::write(logs_dir.join("2026-01.actions"), "[x] Old task").unwrap();

        // Create a hidden dir that should be skipped
        let hidden = root.join(".clearhead");
        fs::create_dir(&hidden).unwrap();
        fs::write(hidden.join("workspace.crdt"), "binary data").unwrap();

        let files = discover_action_files(root).unwrap();

        assert_eq!(files.len(), 4);
        assert!(files.iter().any(|f| f.ends_with("inbox.actions")));
        assert!(files.iter().any(|f| f.ends_with("work.actions")));
        assert!(files.iter().any(|f| f.ends_with("next.actions")));
        assert!(files.iter().any(|f| f.ends_with("2026-01.actions")));
        // Hidden dir should be skipped
        assert!(!files
            .iter()
            .any(|f| f.to_string_lossy().contains(".clearhead")));
    }

    #[test]
    fn test_load_workspace_actions() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(root.join("inbox.actions"), "[ ] Task 1\n[ ] Task 2").unwrap();
        fs::write(root.join("work.actions"), "[ ] Task 3").unwrap();

        let actions = load_workspace_actions(root).unwrap();

        assert_eq!(actions.len(), 3);
    }

    #[test]
    fn test_load_workspace_skips_invalid_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(root.join("valid.actions"), "[ ] Valid task").unwrap();
        fs::write(
            root.join("invalid.actions"),
            "this is not valid action syntax {{{{",
        )
        .unwrap();

        // Should not error, just skip the invalid file
        let actions = load_workspace_actions(root).unwrap();

        // Should have at least the valid task
        assert!(actions.len() >= 1);
    }

    #[test]
    fn test_discover_charters_implicit_from_actions() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(root.join("health.actions"), "[ ] Exercise").unwrap();
        fs::write(root.join("inbox.actions"), "[ ] Random").unwrap();

        let charters = discover_charters(root).unwrap();

        assert_eq!(charters.len(), 1);
        assert_eq!(charters[0].charter.title, "health");
        assert!(!charters[0].is_explicit());
    }

    #[test]
    fn test_discover_charters_explicit_wins() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(root.join("health.actions"), "[ ] Exercise").unwrap();
        fs::write(root.join("health.md"), "# Health & Fitness\n\nStay fit.\n").unwrap();

        let charters = discover_charters(root).unwrap();

        // Should be just one charter, the explicit one
        assert_eq!(charters.len(), 1);
        assert_eq!(charters[0].charter.title, "Health & Fitness");
        assert!(charters[0].is_explicit());
    }

    #[test]
    fn test_discover_charters_directory() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        let project_dir = root.join("build_clearhead");
        fs::create_dir(&project_dir).unwrap();
        fs::write(project_dir.join("next.actions"), "[ ] Build").unwrap();

        let charters = discover_charters(root).unwrap();

        assert_eq!(charters.len(), 1);
        assert_eq!(charters[0].charter.title, "build_clearhead");
        assert!(!charters[0].is_explicit());
    }

    #[test]
    fn test_resolve_charter_by_alias() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(
            root.join("health.md"),
            "---\nalias: health\n---\n# Health & Fitness\n",
        )
        .unwrap();

        let charters = discover_charters(root).unwrap();
        let found = resolve_charter(&charters, "health");
        assert!(found.is_some());
        assert_eq!(found.unwrap().charter.title, "Health & Fitness");
    }

    #[test]
    fn test_resolve_charter_by_name() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(root.join("work.actions"), "[ ] Task").unwrap();

        let charters = discover_charters(root).unwrap();
        let found = resolve_charter(&charters, "work");
        assert!(found.is_some());
        assert_eq!(found.unwrap().charter.title, "work");
    }

    #[test]
    fn test_load_workspace_combines_charters_and_actions() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(root.join("health.actions"), "[ ] Exercise").unwrap();
        fs::write(root.join("work.actions"), "[ ] Task").unwrap();

        let workspace = load_workspace(root).unwrap();
        assert_eq!(workspace.charters.len(), 2);
        assert_eq!(workspace.actions.len(), 2);
    }
}
