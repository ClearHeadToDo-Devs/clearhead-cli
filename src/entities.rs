use chrono::{DateTime, Local};
use tree_sitter::{Node, Tree, TreeCursor};
use uuid::Uuid;

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
    fn try_from(value: TreeWrapper) -> Result<Self, Self::Error> {
        let root_wrapper = create_node_wrapper(value.tree.root_node(), value.source.clone());
        let mut action_list: ActionList = Vec::new();
        let mut binding = root_wrapper.node.walk();

        let root_action_iterator = root_wrapper.node.children(&mut binding);

        root_action_iterator.for_each(|action_node| {
            action_list.push(
                create_node_wrapper(action_node, value.source.clone())
                    .try_into()
                    .expect("failed to properly build action list"),
            )
        });

        return Ok(action_list);
    }
}

pub struct RootAction {
    common: CommonActionProperties,
    story: Option<Story>,
    children: Option<ChildActionList>,
}

impl<'a> TryFrom<NodeWrapper<'a>> for RootAction {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);

        let mut common = None;
        let mut story = None;
        let mut children = None;

        for child in child_iterator {
            match child.kind() {
                "core_action" => {
                    let core_wrapper = create_node_wrapper(child, value.source.clone());
                    common = Some(core_wrapper.try_into()?);
                }
                "story" => {
                    story = Some(get_node_text(&child, &value.source));
                }
                "child_actions" => {
                    // TODO: Implement child action parsing
                    children = None;
                }
                _ => {}
            }
        }

        Ok(RootAction {
            common: common.ok_or("Missing core action properties")?,
            story,
            children,
        })
    }
}
type ChildActionList = Vec<ChildAction>;

struct ChildAction {
    common: CommonActionProperties,
    children: Option<GrandChildActionList>,
}

type GrandChildActionList = Vec<GrandChildAction>;

struct GrandChildAction {
    common: CommonActionProperties,
    children: GreatGrandChildActionList,
}

type GreatGrandChildActionList = Vec<GreatGrandChildAction>;

struct GreatGrandChildAction {
    common: CommonActionProperties,
    children: Option<GreatGreatGrandChildActionList>,
}

type GreatGreatGrandChildActionList = Vec<GreatGreatGrandChildAction>;

struct GreatGreatGrandChildAction {
    common: CommonActionProperties,
    children: Option<LeafActionList>,
}

type LeafActionList = Vec<LeafAction>;

struct LeafAction {
    common: CommonActionProperties,
}
struct CommonActionProperties {
    state: ActionState,
    name: ActionName,
    description: Option<ActionDescription>,
    priority: Option<ActionPriority>,
    context_list: Option<ContextList>,
    id: Option<ActionId>,
    do_date_time: Option<ActionDoDateTime>,
    completed_date_time: Option<ActionCompletedDateTime>,
}

type Story = String;
type ActionName = String;
type ActionPriority = usize;
type ActionDescription = String;
type ContextList = Vec<String>;
type ActionId = Uuid;
type ActionDoDateTime = DateTime<Local>;
type ActionCompletedDateTime = DateTime<Local>;

impl<'a> TryFrom<NodeWrapper<'a>> for CommonActionProperties {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);

        let mut state = ActionState::default();
        let mut name = String::new();
        let mut description = None;
        let mut priority = None;
        let mut context_list = None;
        let mut id = None;
        let mut do_date_time = None;
        let mut completed_date_time = None;

        for child in child_iterator {
            match child.kind() {
                "action_state" => {
                    let state_text = get_node_text(&child, &value.source);
                    state = match state_text.trim() {
                        "( )" => ActionState::NotStarted,
                        "(x)" => ActionState::Completed,
                        "(~)" => ActionState::InProgress,
                        "(-)" => ActionState::BlockedorAwaiting,
                        "(c)" => ActionState::Cancelled,
                        _ => ActionState::NotStarted,
                    };
                }
                "action_name" => {
                    name = get_node_text(&child, &value.source).trim().to_string();
                }
                "action_description" => {
                    description = Some(get_node_text(&child, &value.source).trim().to_string());
                }
                "action_priority" => {
                    if let Ok(prio) = get_node_text(&child, &value.source).trim().parse::<usize>() {
                        priority = Some(prio);
                    }
                }
                "action_context" => {
                    let context_text = get_node_text(&child, &value.source);
                    let contexts: Vec<String> = context_text
                        .split_whitespace()
                        .filter(|s| s.starts_with('@'))
                        .map(|s| s.to_string())
                        .collect();
                    if !contexts.is_empty() {
                        context_list = Some(contexts);
                    }
                }
                "action_id" => {
                    if let Ok(uuid) = Uuid::parse_str(get_node_text(&child, &value.source).trim()) {
                        id = Some(uuid);
                    }
                }
                _ => {} // Ignore other node types for now
            }
        }

        Ok(CommonActionProperties {
            state,
            name,
            description,
            priority,
            context_list,
            id,
            do_date_time,
            completed_date_time,
        })
    }
}

fn get_node_text(node: &Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
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
