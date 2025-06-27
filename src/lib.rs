use std::collections::HashMap;

use chrono::DateTime;
use serde_json::Value;
use tree_sitter::Tree;

mod entities;
use entities::{ActionList, create_tree_wrapper};

use uuid::Uuid;

use tree_sitter::Node;
// this is the function where we actually use treesitter to parse the actions into the tree, and
// translate that into a proper vector of hashmaps so that we are passing back plain data
pub fn get_action_list(
    _opts: &HashMap<String, Value>,
    actions: String,
) -> Result<Vec<HashMap<String, Value>>, String> {
    let tree = match get_action_list_tree(&actions) {
        Ok(tree) => tree,
        Err(e) => {
            return Err(format!("Failed to parse actions: {}", e));
        }
    };

    let tree_wrapper = create_tree_wrapper(tree, actions);
    let action_list: ActionList = tree_wrapper
        .try_into()
        .map_err(|e| format!("Failed to convert tree to actions: {}", e))?;

    Ok(action_list
        .into_iter()
        .map(|action| action.to_hashmap())
        .collect())
}

fn get_action_list_tree(actions: &str) -> Result<Tree, String> {
    let mut action_parser = tree_sitter::Parser::new();

    action_parser
        .set_language(&tree_sitter_actions::LANGUAGE.into())
        .expect("Failed to set language for tree-sitter parser");

    return match action_parser.parse(actions, None) {
        Some(tree) => Ok(tree),
        None => Err("Failed to parse actions".to_string()),
    };
}

fn get_action_list_map(
    content: &str,
    tree: Tree,
) -> Result<Vec<HashMap<String, ActionProperty>>, String> {
    let root = tree.root_node();

    let mut binding = tree.walk();
    let root_action_iterator = root.children(&mut binding);

    root_action_iterator
        .map(|root_action| get_action_map(content, &root_action, true))
        .collect()
}

fn get_action_map(content: &str, node: &Node) -> Result<HashMap<String, ActionProperty>, String> {}
enum ActionProperty {
    Name(String),
    Story(String),
    Description(String),
    Priority(usize),
    State(ActionState),
    Context(Vec<String>),
    ID(Uuid),
    Children(Vec<HashMap<String, ActionProperty>>),
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionState {
    #[default]
    NotStarted,
    Completed,
    InProgress,
    BlockedorAwaiting,
    Cancelled,
}
