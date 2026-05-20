use serde_json::json;
use std::fs;

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

    let config = json!({
        "workspace_id": workspace_id,
        "workspace_name": workspace_name,
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
