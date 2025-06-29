use chrono::{DateTime, Local};
use tree_sitter::{Node, Tree};
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

pub struct RootAction {
    common: CommonActionProperties,
    story: Option<Story>,
    children: Option<ChildActionList>,
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

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionState {
    #[default]
    NotStarted,
    Completed,
    InProgress,
    BlockedorAwaiting,
    Cancelled,
}
