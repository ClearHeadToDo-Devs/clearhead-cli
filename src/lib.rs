use serde_json::{Map, Value};
use tree_sitter::Tree;

pub mod treesitter;

pub mod entities;
use entities::ActionList;

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

pub fn get_action_list_struct(_opts: &Value, actions: &str) -> Result<ActionList, String> {
    let tree = get_action_list_tree(actions)?;

    let tree_wrapper = treesitter::TreeWrapper {
        tree,
        source: actions.to_string(),
    };
    let action_list: ActionList = tree_wrapper.try_into()?;

    return Ok(action_list);
}
// this is the function where we actually use treesitter to parse the actions into the tree, and
// translate that into a proper vector of hashmaps so that we are passing back plain data
pub fn get_action_list(_opts: &Value, actions: String) -> Result<Value, String> {
    let tree = get_action_list_tree(&actions)?;

    let tree_wrapper = treesitter::TreeWrapper {
        tree,
        source: actions.clone(),
    };

    let action_list: ActionList = tree_wrapper.try_into()?;

    return Ok(serde_json::to_value(&action_list).unwrap());
}

fn get_action_list_tree(actions: &str) -> Result<Tree, String> {
    let mut action_parser = tree_sitter::Parser::new();

    action_parser
        .set_language(&tree_sitter_actions::LANGUAGE.into())
        .expect("Failed to set language for tree-sitter parser");

    action_parser
        .parse(actions, None)
        .ok_or("Failed to parse tree".to_string())
}
