use tree_sitter::{Node, Tree, TreeCursor};

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

type ActionList = Vec<RootAction>;

impl TryFrom<TreeWrapper> for ActionList {
    fn try_from(tree: TreeWrapper) -> Result<Self, &'static str> {
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
    fn try_from(node: NodeWrapper) -> Result<Self, &'static str> {
        let mut root_action = RootAction::default();
        for child in node.node.children(&mut node.node.walk()) {
            match child.kind() {
                "core_action" => root_action.core = child.try_into().unwrap(),
                "story" => {
                    root_action.story =
                        Some(node.source[child.start_byte()..child.end_byte()].to_string());
                }
                "child_action_list" => continue,
                _ => return Err("test"),
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
    context_list: ContextList,
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
                    let state_node = child.try_into();
                }
                "description" => {
                    properties.description =
                        let description_text_node = child.child(0
                        Some(child.child(1).utf8_text(&node.source).unwrap().to_string())
                }
                "priority" => {
                    properties.priority = Some(
                        child
                            .child(1)
                            .unwrap()
                            .utf8_text(&node.source)
                            .unwrap()
                            .parse()
                            .unwrap(),
                    )
                }
                "context_list" => properties.context_list = child.try_into(),
                _ => return Err("Unknown core action property"),
            }
        }
        Ok(properties)
    }
}

impl TryFrom<NodeWrapper> for ContextList {
    type Error = &'static str;
    fn try_from(node: NodeWrapper) -> Result<Self, Self::Error> {
        let mut contexts = Vec::new();
        for child in node.node.children(&mut node.node.walk()) {
            match child.kind() {
                "context_icon" => continue,
                "middle_context" => contexts.push(child.child(0).unwrap().utf8_text().unwrap()),
                "tail_context" => contexts.push(child.utf8_text(node.source).unwrap()),

                _ => return Err("Unknown context property"),
            }
            Ok(contexts)
        }
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
