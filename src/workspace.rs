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

use std::path::{Path, PathBuf};
use crate::entities::{Action, ActionList};
use crate::get_action_list_struct;
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
        self.sourced_actions.iter().map(|sa| sa.action.clone()).collect()
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
        let entry = entry
            .map_err(|e| format!("Failed to read directory entry: {}", e))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    #[test]
    fn test_infer_project_name_root_file() {
        assert_eq!(infer_project_name(Path::new("work.actions")), Some("work".to_string()));
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
        assert!(!files.iter().any(|f| f.to_string_lossy().contains(".clearhead")));
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
        fs::write(root.join("invalid.actions"), "this is not valid action syntax {{{{").unwrap();

        // Should not error, just skip the invalid file
        let actions = load_workspace_actions(root).unwrap();

        // Should have at least the valid task
        assert!(actions.len() >= 1);
    }
}
