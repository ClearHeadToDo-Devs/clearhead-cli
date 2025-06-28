use std::{array, collections::HashMap};

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
) -> Result<Vec<HashMap<String, ActionProperty>>, String> {
    let tree = get_action_list_tree(&actions)?;

    let root_node = tree.root_node();

    return get_action_list_vec(&actions, &root_node);
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

fn get_action_list_vec(
    content: &str,
    list: &Node,
) -> Result<Vec<HashMap<String, ActionProperty>>, String> {
    let mut binding = list.walk();
    let root_action_iterator = list.children(&mut binding);

    root_action_iterator
        .map(|root_action| get_action_map(content, &root_action))
        .collect()
}

fn get_action_map(content: &str, node: &Node) -> Result<HashMap<String, ActionProperty>, String> {
    let mut binding = node.walk();
    let action_property_iterator = node.children(&mut binding);

    let mut action_map = HashMap::new();

    action_property_iterator.for_each(|action_property| {
        add_action_property(&mut action_map, content, &action_property).unwrap()
    });

    Ok(action_map)
}

fn add_action_property(
    map: &mut HashMap<String, ActionProperty>,
    content: &str,
    node: &Node,
) -> Result<(), String> {
    match node.kind() {
        _ => todo!(),
    }
}
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
