use std::collections::HashMap;
use serde_json::Value;
use tree_sitter::{Node, Tree};

pub fn create_tree_wrapper(tree: Tree, source: String) -> TreeWrapper {
    TreeWrapper { tree, source }
}

// we need both the tree and the source to do our type conversions properly
pub struct TreeWrapper {
    tree: Tree,
    source: String,
}

pub fn create_node_wrapper(node: Node, source: String) -> NodeWrapper {
    NodeWrapper { node, source }
}

// same goes for the nodes, infact, we are going to be passing a cloned version of the string so
// everything has what they need early
pub struct NodeWrapper<'a> {
    node: Node<'a>,
    source: String,
}

pub type ActionList = Vec<RootAction>;

impl TryFrom<TreeWrapper> for ActionList {
    type Error = &'static str;
    
    fn try_from(tree: TreeWrapper) -> Result<Self, Self::Error> {
        let mut actions = Vec::new();
        for node in tree.tree.root_node().children(&mut tree.tree.walk()) {
            let wrapped_node = create_node_wrapper(node, tree.source.clone());
            let root_action: RootAction = wrapped_node.try_into()?;
            actions.push(root_action);
        }
        return Ok(actions);
    }
}

#[derive(Default)]
pub struct RootAction {
    core: CoreActionProperties,
    story: Option<String>,
    children: Option<Vec<ChildAction>>,
}

impl<'a> TryFrom<NodeWrapper<'a>> for RootAction {
    type Error = &'static str;
    
    fn try_from(node: NodeWrapper) -> Result<Self, Self::Error> {
        let mut root_action = RootAction::default();
        for child in node.node.children(&mut node.node.walk()) {
            match child.kind() {
                "core_action" => {
                    let wrapped_child = create_node_wrapper(child, node.source.clone());
                    root_action.core = wrapped_child.try_into()?;
                }
                "story" => {
                    root_action.story =
                        Some(node.source[child.start_byte()..child.end_byte()].to_string());
                }
                "child_action_list" => continue,
                _ => return Err("Unknown root action child"),
            }
        }
        return Ok(root_action);
    }
}

pub struct ChildAction {
    core: CoreActionProperties,
    grandchildren: Vec<GrandChildAction>,
}

pub struct GrandChildAction {
    core: CoreActionProperties,
    great_grandchildren: Vec<GrandChildAction>,
}

pub struct GreatGrandChildAction {
    core: CoreActionProperties,
    great_great_grandchildren: Vec<GrandChildAction>,
}

pub struct DoubleGreatGrandChildAction {
    core: CoreActionProperties,
    leaf_actions: Vec<LeafAction>,
}

pub struct LeafAction {
    core: CoreActionProperties,
}

#[derive(Default)]
pub struct CoreActionProperties {
    name: String,
    state: ActionState,
    description: Option<String>,
    priority: Option<u8>,
    context_list: Option<ContextList>,
}

type ContextList = Vec<String>;

impl<'a> TryFrom<NodeWrapper<'a>> for CoreActionProperties {
    type Error = &'static str;
    fn try_from(node: NodeWrapper) -> Result<Self, Self::Error> {
        let mut properties = CoreActionProperties::default();
        for child in node.node.children(&mut node.node.walk()) {
            match child.kind() {
                "name" => {
                    properties.name = node.source[child.start_byte()..child.end_byte()].to_string()
                }
                "state" => {
                    let wrapped_child = create_node_wrapper(child, node.source.clone());
                    properties.state = wrapped_child.try_into()?;
                }
                "description" => {
                    let description_text_node = child.child(0).unwrap();

                    properties.description = Some(
                        node.source
                            [description_text_node.start_byte()..description_text_node.end_byte()]
                            .to_string(),
                    );
                }
                "priority" => {
                    let priority_text_node = child.child(1).unwrap();

                    properties.priority = Some(
                        node.source[priority_text_node.start_byte()..priority_text_node.end_byte()]
                            .parse()
                            .map_err(|_| "Failed to parse priority")?,
                    );
                }
                "context_list" => {
                    let wrapped_child = create_node_wrapper(child, node.source.clone());
                    properties.context_list = Some(wrapped_child.try_into()?);
                }
                _ => return Err("Unknown core action property"),
            }
        }
        Ok(properties)
    }
}

impl<'a> TryFrom<NodeWrapper<'a>> for ContextList {
    type Error = &'static str;
    fn try_from(node: NodeWrapper) -> Result<Self, Self::Error> {
        let mut contexts = Vec::new();
        for child in node.node.children(&mut node.node.walk()) {
            match child.kind() {
                "context_icon" => continue,
                "middle_context" => {
                    let node_text = child.child(1).unwrap();
                    contexts
                        .push(node.source[node_text.start_byte()..node_text.end_byte()].to_string())
                }
                "tail_context" => {
                    contexts.push(child.utf8_text(node.source.as_bytes()).unwrap().to_string());
                }
                _ => return Err("Unknown context property"),
            }
        }
        Ok(contexts)
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

impl<'a> TryFrom<NodeWrapper<'a>> for ActionState {
    type Error = &'static str;
    fn try_from(node: NodeWrapper) -> Result<Self, Self::Error> {
        match node.node.child(0).unwrap().kind() {
            "not_started" => Ok(ActionState::NotStarted),
            "completed" => Ok(ActionState::Completed),
            "in_progress" => Ok(ActionState::InProgress),
            "blocked_or_awaiting" => Ok(ActionState::BlockedorAwaiting),
            "cancelled" => Ok(ActionState::Cancelled),
            _ => Err("Unknown action state"),
        }
    }
}

pub struct ExtendedDateTime<Tz: chrono::TimeZone> {
    date: chrono::DateTime<Tz>,
    recurrance: Option<Recurrance>,
}

pub enum Recurrance {
    None,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl RootAction {
    pub fn to_hashmap(&self) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        
        // Add core properties
        map.insert("name".to_string(), Value::String(self.core.name.clone()));
        map.insert("state".to_string(), Value::String(format!("{:?}", self.core.state)));
        
        if let Some(ref description) = self.core.description {
            map.insert("description".to_string(), Value::String(description.clone()));
        }
        
        if let Some(priority) = self.core.priority {
            map.insert("priority".to_string(), Value::Number(priority.into()));
        }
        
        if let Some(ref context_list) = self.core.context_list {
            let contexts: Vec<Value> = context_list.iter()
                .map(|c| Value::String(c.clone()))
                .collect();
            map.insert("context_list".to_string(), Value::Array(contexts));
        }
        
        if let Some(ref story) = self.story {
            map.insert("story".to_string(), Value::String(story.clone()));
        }
        
        map
    }
}
