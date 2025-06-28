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
        "core action" => add_core_action_properties(map, content, node),
        _ => todo!(),
    }
}

fn add_core_action_properties(
    map: &mut HashMap<String, ActionProperty>,
    content: &str,
    node: &Node,
) -> Result<(), String> {
    let mut binding = node.walk();
    let action_property_iterator = node.children(&mut binding);

    action_property_iterator.for_each(|property| {
        map.insert(
            node.kind().to_string(),
            get_action_core_property_from_node(&property, content).expect("unable to convert node"),
        );
    });

    Ok(())
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

fn get_action_core_property_from_node(
    node: &Node,
    content: &str,
) -> Result<ActionProperty, String> {
    match node.kind() {
        "state" => match node.child(0).unwrap().kind() {
            "not_started" => Ok(ActionProperty::State(ActionState::NotStarted)),
            "completed" => Ok(ActionProperty::State(ActionState::Completed)),
            "in_progress" => Ok(ActionProperty::State(ActionState::InProgress)),
            "blocked" => Ok(ActionProperty::State(ActionState::BlockedorAwaiting)),
            "cancelled" => Ok(ActionProperty::State(ActionState::Cancelled)),
            _ => Err("new or malformed state".to_string()),
        },
        "name" => Ok(ActionProperty::Name(get_node_value(node, content))),
        "description" => Ok(ActionProperty::Description(get_node_value(node, content))),
        "story" => Ok(ActionProperty::Story(get_node_value(node, content))),
        "priority" => Ok(ActionProperty::Priority(
            get_node_value(node, content)
                .parse::<usize>()
                .expect("failed to parse priority properly"),
        )),
        _ => todo!(),
    }
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

fn get_node_value(node: &Node, content: &str) -> String {
    content[node.start_byte()..node.end_byte()].to_string()
}
