use serde_json::{Map, Value};
use tree_sitter::Tree;

pub mod treesitter;

pub mod entities;
use entities::ActionList;
pub use entities::format_action_list;

// merging json hashmaps as our universal structure
pub fn merge_hashmaps(
    left: &Map<String, Value>,
    right: &Map<String, Value>,
) -> Result<Value, String> {
    let mut merged = left.clone();
    for (key, value) in right {
        merged.insert(key.clone(), value.clone());
    }
    Ok(Value::Object(merged))
}

/// Parse a .actions file into a structured ActionList
///
/// # Arguments
/// * `_opts` - Configuration options (currently unused)
/// * `actions` - The .actions file content as a string
///
/// # Returns
/// A Vec<Action> representing the flat list of parsed actions
pub fn get_action_list_struct(_opts: &Value, actions: &str) -> Result<ActionList, String> {
    let tree = get_action_list_tree(actions)?;
    let tree_wrapper = treesitter::TreeWrapper {
        tree,
        source: actions.to_string(),
    };
    let action_list: ActionList = tree_wrapper.try_into().map_err(|e: &str| e.to_string())?;
    Ok(action_list)
}

/// Parse a .actions file and return as JSON Value
///
/// This is a convenience wrapper around get_action_list_struct that
/// serializes the result to JSON for data-centric workflows.
///
/// # Arguments
/// * `opts` - Configuration options (passed through)
/// * `actions` - The .actions file content as a string
///
/// # Returns
/// A JSON Value containing the serialized ActionList
pub fn get_action_list(opts: &Value, actions: String) -> Result<Value, String> {
    let action_list = get_action_list_struct(opts, &actions)?;
    serde_json::to_value(&action_list)
        .map_err(|e| format!("Failed to serialize actions to JSON: {}", e))
}

/// Parse a .actions file into a tree-sitter Tree
///
/// This is a low-level function that returns the raw tree-sitter parse tree.
/// Most users should use get_action_list_struct() instead.
///
/// # Arguments
/// * `actions` - The .actions file content as a string
///
/// # Returns
/// A tree-sitter Tree representing the parsed structure
pub fn get_action_list_tree(actions: &str) -> Result<Tree, String> {
    let mut action_parser = tree_sitter::Parser::new();

    action_parser
        .set_language(&tree_sitter_actions::LANGUAGE.into())
        .expect("Failed to set language for tree-sitter parser");

    action_parser
        .parse(actions, None)
        .ok_or("Failed to parse tree".to_string())
}
