use serde_json::json;
use std::fs;

/// The published contract for `.clearhead/config.json`. Stamped on every
/// workspace this command initializes so the file is self-describing and
/// editors validate on write — see `charter_metadata`'s matching pointer in
/// clearhead-core for the sidecar half of this convention.
const CONFIG_SCHEMA_URL: &str = "https://raw.githubusercontent.com/ClearHeadToDo-Devs/specifications/master/schemas/config.schema.json";

/// Initialize a clearhead workspace in the current directory.
///
/// Creates `.clearhead/config.json` with a stable workspace UUID and a name
/// derived from the current directory. Creates `.clearhead/charters/` if
/// absent. Idempotent — safe to rerun; does not overwrite an existing config.
pub fn run() -> Result<(), String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Cannot determine current directory: {}", e))?;

    let clearhead_dir = cwd.join(".clearhead");
    let config_path = clearhead_dir.join("config.json");
    let charters_dir = clearhead_dir.join("charters");

    fs::create_dir_all(&clearhead_dir)
        .map_err(|e| format!("Failed to create .clearhead/: {}", e))?;
    fs::create_dir_all(&charters_dir)
        .map_err(|e| format!("Failed to create .clearhead/charters/: {}", e))?;

    // Keep config.local.json — the git-ignored personal override — out of version
    // control. A scoped .clearhead/.gitignore owns this rule so we don't touch the
    // project root's ignore conventions. Written unconditionally (independent of
    // the config.json guard below) so existing workspaces pick it up on a rerun.
    let gitignore_path = clearhead_dir.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(&gitignore_path, "config.local.json\n")
            .map_err(|e| format!("Failed to write .clearhead/.gitignore: {}", e))?;
    }

    if config_path.exists() {
        println!(
            "Already initialized — {} exists.",
            config_path.display()
        );
        return Ok(());
    }

    let workspace_name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace")
        .to_string();

    let workspace_id = uuid::Uuid::now_v7().to_string();

    let created_at = chrono::Local::now().format("%Y-%m-%d").to_string();

    let config = json!({
        "$schema": CONFIG_SCHEMA_URL,
        "workspace_id": workspace_id,
        "workspace_name": workspace_name,
        "created_at": created_at,
    });

    let json_str = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&config_path, json_str)
        .map_err(|e| format!("Failed to write config.json: {}", e))?;

    println!(
        "Initialized workspace '{}' ({})",
        workspace_name,
        &workspace_id[..8]
    );
    Ok(())
}
