//! CLI CRDT integration — thin wrapper around core CRDT types.
//!
//! Adds CLI-specific concerns:
//! - XDG directory detection for workspace paths
//! - File-import seeding (seed CRDT from disk on first access)
//!
//! All CRDT logic lives in `clearhead_core::crdt`.

use crate::environment_reader::get_data_dir;
use std::path::{Path, PathBuf};

// Re-export core CRDT types so existing `clearhead_cli::crdt::*` imports keep working
pub use clearhead_core::crdt::{
    ActionRepository, CrdtStorage, Workspace, WorkspaceDoc, CRDT_SCHEMA_VERSION,
};

/// Detect the global workspace from XDG paths and validate a file is within it.
///
/// This is the CLI equivalent of constructing a `Workspace` — it resolves
/// XDG_STATE_HOME and XDG_DATA_HOME to find the hub and spoke directories.
pub fn detect_workspace(file_path: &Path) -> Result<Workspace, String> {
    let workspace = global_workspace()?;
    workspace.validate_file(file_path)?;
    Ok(workspace)
}

/// Construct the global workspace from XDG paths (no file validation).
pub fn global_workspace() -> Result<Workspace, String> {
    let state_dir = get_xdg_state_home()?;
    let data_dir = get_data_dir();
    Ok(Workspace::new(state_dir, data_dir))
}

/// Load an ActionRepository for a file, with XDG workspace detection.
///
/// This is the CLI convenience wrapper: detects workspace from XDG paths,
/// validates the file is within it, and loads the CRDT.
pub fn load_action_repo(file_path: &Path) -> Result<ActionRepository, String> {
    let workspace = detect_workspace(file_path)?;
    ActionRepository::load(workspace, file_path.to_path_buf())
}

fn get_xdg_state_home() -> Result<PathBuf, String> {
    let state_home = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".local").join("state")
        });

    let clearhead_state = state_home.join("clearhead");
    std::fs::create_dir_all(&clearhead_state)
        .map_err(|e| format!("Failed to create state directory: {}", e))?;

    Ok(clearhead_state)
}
