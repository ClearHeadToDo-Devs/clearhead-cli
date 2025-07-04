use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::treesitter::{NodeWrapper, TreeWrapper, create_node_wrapper, get_node_text};
use uuid::Uuid;

pub type ActionList = Vec<RootAction>;

impl TryFrom<TreeWrapper> for ActionList {
    type Error = &'static str;
    fn try_from(value: TreeWrapper) -> Result<Self, Self::Error> {
        let root = value.tree.root_node();
        let mut action_list: ActionList = Vec::new();
        let mut binding = root.walk();

        let root_action_iterator = root.children(&mut binding);

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let mut story: Option<Story> = None;
        let mut children: Option<ChildActionList> = None;

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

impl<'a> TryFrom<NodeWrapper<'a>> for ChildActionList {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);
        let mut child_list: ChildActionList = Vec::new();
        for child in child_iterator {
            if child.kind() == "child_action" {
                let child_wrapper = create_node_wrapper(child, value.source.clone());
                child_list.push(
                    child_wrapper
                        .try_into()
                        .expect("failed to convert child action"),
                );
            }
        }
        Ok(child_list)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChildAction {
    common: CommonActionProperties,
    children: Option<GrandChildActionList>,
}

impl<'a> TryFrom<NodeWrapper<'a>> for ChildAction {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);
        let mut common = None;
        let mut children: Option<GrandChildActionList> = None;
        for child in child_iterator {
            match child.kind() {
                "core_action" => {
                    let core_wrapper = create_node_wrapper(child, value.source.clone());
                    common = Some(core_wrapper.try_into()?);
                }
                "grand_child_actions" => {
                    children = Some(GrandChildActionList::try_from(create_node_wrapper(
                        child,
                        value.source.clone(),
                    ))?);
                }
                _ => {}
            }
        }
        Ok(ChildAction {
            common: common.ok_or("Missing core action properties")?,
            children,
        })
    }
}

type GrandChildActionList = Vec<GrandChildAction>;

impl<'a> TryFrom<NodeWrapper<'a>> for GrandChildActionList {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);
        let mut grand_child_list: GrandChildActionList = Vec::new();
        for child in child_iterator {
            if child.kind() == "grand_child_action" {
                let grand_child_wrapper = create_node_wrapper(child, value.source.clone());
                grand_child_list.push(
                    grand_child_wrapper
                        .try_into()
                        .expect("failed to convert grand child action"),
                );
            }
        }
        Ok(grand_child_list)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GrandChildAction {
    common: CommonActionProperties,
    children: GreatGrandChildActionList,
}

impl<'a> TryFrom<NodeWrapper<'a>> for GrandChildAction {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);
        let mut common = None;
        let mut children: Option<GreatGrandChildActionList> = None;
        for child in child_iterator {
            match child.kind() {
                "core_action" => {
                    let core_wrapper = create_node_wrapper(child, value.source.clone());
                    common = Some(core_wrapper.try_into()?);
                }
                "great_grand_child_actions" => {
                    children = Some(GreatGrandChildActionList::try_from(create_node_wrapper(
                        child,
                        value.source.clone(),
                    ))?);
                }
                _ => {}
            }
        }
        Ok(GrandChildAction {
            common: common.ok_or("Missing core action properties")?,
            children: children.ok_or("Missing great grand child actions")?,
        })
    }
}

type GreatGrandChildActionList = Vec<GreatGrandChildAction>;

impl<'a> TryFrom<NodeWrapper<'a>> for GreatGrandChildActionList {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);
        let mut great_grand_child_list: GreatGrandChildActionList = Vec::new();
        for child in child_iterator {
            if child.kind() == "great_grand_child_action" {
                let great_grand_child_wrapper = create_node_wrapper(child, value.source.clone());
                great_grand_child_list.push(
                    great_grand_child_wrapper
                        .try_into()
                        .expect("failed to convert great grand child action"),
                );
            }
        }
        Ok(great_grand_child_list)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GreatGrandChildAction {
    common: CommonActionProperties,
    children: Option<GreatGreatGrandChildActionList>,
}

impl<'a> TryFrom<NodeWrapper<'a>> for GreatGrandChildAction {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);
        let mut common = None;
        let mut children: Option<GreatGreatGrandChildActionList> = None;
        for child in child_iterator {
            match child.kind() {
                "core_action" => {
                    let core_wrapper = create_node_wrapper(child, value.source.clone());
                    common = Some(core_wrapper.try_into()?);
                }
                "great_great_grand_child_actions" => {
                    children = Some(GreatGreatGrandChildActionList::try_from(
                        create_node_wrapper(child, value.source.clone()),
                    )?);
                }
                _ => {}
            }
        }
        Ok(GreatGrandChildAction {
            common: common.ok_or("Missing core action properties")?,
            children: children,
        })
    }
}

type GreatGreatGrandChildActionList = Vec<GreatGreatGrandChildAction>;

impl<'a> TryFrom<NodeWrapper<'a>> for GreatGreatGrandChildActionList {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);
        let mut great_great_grand_child_list: GreatGreatGrandChildActionList = Vec::new();
        for child in child_iterator {
            if child.kind() == "great_great_grand_child_action" {
                let great_great_grand_child_wrapper =
                    create_node_wrapper(child, value.source.clone());
                great_great_grand_child_list.push(
                    great_great_grand_child_wrapper
                        .try_into()
                        .expect("failed to convert great great grand child action"),
                );
            }
        }
        Ok(great_great_grand_child_list)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GreatGreatGrandChildAction {
    common: CommonActionProperties,
    children: Option<LeafActionList>,
}

impl<'a> TryFrom<NodeWrapper<'a>> for GreatGreatGrandChildAction {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);
        let mut common = None;
        let mut children: Option<LeafActionList> = None;
        for child in child_iterator {
            match child.kind() {
                "core_action" => {
                    let core_wrapper = create_node_wrapper(child, value.source.clone());
                    common = Some(core_wrapper.try_into()?);
                }
                "leaf_actions" => {
                    children = Some(LeafActionList::try_from(create_node_wrapper(
                        child,
                        value.source.clone(),
                    ))?);
                }
                _ => {}
            }
        }
        Ok(GreatGreatGrandChildAction {
            common: common.ok_or("Missing core action properties")?,
            children,
        })
    }
}

type LeafActionList = Vec<LeafAction>;

impl<'a> TryFrom<NodeWrapper<'a>> for LeafActionList {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);
        let mut leaf_list: LeafActionList = Vec::new();
        for child in child_iterator {
            if child.kind() == "leaf_action" {
                let leaf_wrapper = create_node_wrapper(child, value.source.clone());
                leaf_list.push(
                    leaf_wrapper
                        .try_into()
                        .expect("failed to convert leaf action"),
                );
            }
        }
        Ok(leaf_list)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeafAction {
    common: CommonActionProperties,
}

impl<'a> TryFrom<NodeWrapper<'a>> for LeafAction {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);
        let mut common = None;
        for child in child_iterator {
            if child.kind() == "core_action" {
                let core_wrapper = create_node_wrapper(child, value.source.clone());
                common = Some(core_wrapper.try_into()?);
            }
        }
        Ok(LeafAction {
            common: common.ok_or("Missing core action properties")?,
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let do_date_time = None;
        let completed_date_time = None;

        for child in child_iterator {
            match child.kind() {
                "state" => match child.child(0).unwrap().kind() {
                    "not_started" => state = ActionState::NotStarted,
                    "completed" => state = ActionState::Completed,
                    "in_progress" => state = ActionState::InProgress,
                    "blocked" => state = ActionState::BlockedorAwaiting,
                    "cancelled" => state = ActionState::Cancelled,
                    _ => return Err("Unknown or malformed action state"),
                },
                "name" => {
                    name = get_node_text(&child, &value.source).trim().to_string();
                }
                "description" => {
                    description = Some(get_node_text(&child, &value.source).trim().to_string());
                }
                "priority" => {
                    if let Ok(prio) = get_node_text(&child, &value.source).trim().parse::<usize>() {
                        priority = Some(prio);
                    }
                }
                "context_list" => {
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
                "action" => {
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

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionState {
    #[default]
    NotStarted,
    Completed,
    InProgress,
    BlockedorAwaiting,
    Cancelled,
}
