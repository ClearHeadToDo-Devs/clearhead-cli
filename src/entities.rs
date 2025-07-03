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
        let node_wrapper = create_node_wrapper(value.node, value.source.clone());
        let binding = node_wrapper.node.walk();

        let child_iterator = value.node.children(&mut binding);

        Ok(RootAction {
            common: child_iterator
                .find(|action| action.kind() == "core_action")
                .expect("no structure")
                .into(),
            story: (),
            children: (),
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
type ActionDescription = usize;
type ContextList = Vec<String>;
type ActionId = Uuid;
type ActionDoDateTime = DateTime<Local>;
type ActionCompletedDateTime = DateTime<Local>;

impl<'a> TryFrom<NodeWrapper<'a>> for CommonActionProperties {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let node_wrapper = create_node_wrapper(value.node, value.source.clone());
        let binding = node_wrapper.node.walk();

        let child_iterator = value.node.children(&mut binding);

        Ok(CommonActionProperties {
            state: (),
            name: (),
            description: (),
            priority: (),
            context_list: (),
            id: (),
            do_date_time: (),
            completed_date_time: (),
        })
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
