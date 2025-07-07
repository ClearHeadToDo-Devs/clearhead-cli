use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::treesitter::{NodeWrapper, TreeWrapper, create_node_wrapper, get_node_text};
use uuid::Uuid;

pub type ActionList = Vec<RootAction>;

impl fmt::Display for ActionList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for action in self {
            writeln!(f, "{}", action)?;
        }
        Ok(())
    }
}

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

macro_rules! impl_action_list_try_from {
    ($list_type:ty, $child_kind:literal, $expect_msg:literal) => {
        impl<'a> TryFrom<NodeWrapper<'a>> for $list_type {
            type Error = &'static str;
            fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
                let mut binding = value.node.walk();
                let child_iterator = value.node.children(&mut binding);
                let mut list: $list_type = Vec::new();
                for child in child_iterator {
                    if child.kind() == $child_kind {
                        let wrapper = create_node_wrapper(child, value.source.clone());
                        list.push(wrapper.try_into().expect($expect_msg));
                    }
                }
                Ok(list)
            }
        }
    };
}

macro_rules! impl_action_node_try_from {
    ($struct_name:ty, $children_field:ident, $children_kind:literal) => {
        impl<'a> TryFrom<NodeWrapper<'a>> for $struct_name {
            type Error = &'static str;
            fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
                let mut binding = value.node.walk();
                let child_iterator = value.node.children(&mut binding);

                let mut common: CommonActionProperties = CommonActionProperties::default();
                let mut $children_field = None;

                for child in child_iterator {
                    match child.kind() {
                        "core_action" => {
                            let core_wrapper = create_node_wrapper(child, value.source.clone());
                            common = core_wrapper.try_into()?;
                        }
                        $children_kind => {
                            $children_field =
                                Some(create_node_wrapper(child, value.source.clone()).try_into()?);
                        }
                        _ => {}
                    }
                }

                Ok(Self {
                    common,
                    $children_field,
                })
            }
        }
    };
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct RootAction {
    pub common: CommonActionProperties,
    pub story: Option<Story>,
    pub children: Option<ChildActionList>,
}

impl fmt::Display for RootAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.common)?;
        
        if let Some(story) = &self.story {
            write!(f, " *{}", story)?;
        }
        
        if let Some(children) = &self.children {
            for child in children {
                write!(f, " >{}", child)?;
            }
        }
        
        Ok(())
    }
}

impl<'a> TryFrom<NodeWrapper<'a>> for RootAction {
    type Error = &'static str;
    fn try_from(value: NodeWrapper<'a>) -> Result<Self, Self::Error> {
        let mut binding = value.node.walk();
        let child_iterator = value.node.children(&mut binding);

        let mut common = CommonActionProperties::default();
        let mut story: Option<Story> = None;
        let mut children: Option<ChildActionList> = None;

        for child in child_iterator {
            match child.kind() {
                "core_action" => {
                    let core_wrapper = create_node_wrapper(child, value.source.clone());
                    common = core_wrapper.try_into()?;
                }
                "story" => {
                    story = Some(get_node_text(&child, &value.source));
                }
                "child_actions" => {
                    // TODO: Implement child action parsing
                    children = Some(ChildActionList::try_from(create_node_wrapper(
                        child,
                        value.source.clone(),
                    ))?);
                }
                _ => {}
            }
        }

        Ok(RootAction {
            common,
            story,
            children,
        })
    }
}
type ChildActionList = Vec<ChildAction>;

impl_action_list_try_from!(
    ChildActionList,
    "child_action",
    "failed to convert child action"
);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct ChildAction {
    common: CommonActionProperties,
    grandchildren: Option<GrandChildActionList>,
}

impl fmt::Display for ChildAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.common)?;
        
        if let Some(grandchildren) = &self.grandchildren {
            for grandchild in grandchildren {
                write!(f, " >>{}", grandchild)?;
            }
        }
        
        Ok(())
    }
}

impl_action_node_try_from!(ChildAction, grandchildren, "grandchild_action_list");

type GrandChildActionList = Vec<GrandChildAction>;

impl_action_list_try_from!(
    GrandChildActionList,
    "grand_child_action",
    "failed to convert grand child action"
);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
struct GrandChildAction {
    common: CommonActionProperties,
    great_grandchildren: Option<GreatGrandChildActionList>,
}

impl fmt::Display for GrandChildAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.common)?;
        
        if let Some(great_grandchildren) = &self.great_grandchildren {
            for great_grandchild in great_grandchildren {
                write!(f, " >>>{}", great_grandchild)?;
            }
        }
        
        Ok(())
    }
}

impl_action_node_try_from!(
    GrandChildAction,
    great_grandchildren,
    "great_grand_child_actions"
);

type GreatGrandChildActionList = Vec<GreatGrandChildAction>;

impl_action_list_try_from!(
    GreatGrandChildActionList,
    "great_grand_child_action",
    "failed to convert great grand child action"
);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
struct GreatGrandChildAction {
    common: CommonActionProperties,
    great_great_grandchildren: Option<GreatGreatGrandChildActionList>,
}

impl fmt::Display for GreatGrandChildAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.common)?;
        
        if let Some(great_great_grandchildren) = &self.great_great_grandchildren {
            for great_great_grandchild in great_great_grandchildren {
                write!(f, " >>>>{}", great_great_grandchild)?;
            }
        }
        
        Ok(())
    }
}

impl_action_node_try_from!(
    GreatGrandChildAction,
    great_great_grandchildren,
    "great_great_grand_child_actions"
);

type GreatGreatGrandChildActionList = Vec<GreatGreatGrandChildAction>;

impl_action_list_try_from!(
    GreatGreatGrandChildActionList,
    "great_great_grand_child_action",
    "failed to convert great great grand child action"
);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
struct GreatGreatGrandChildAction {
    common: CommonActionProperties,
    leaf_children: Option<LeafActionList>,
}

impl fmt::Display for GreatGreatGrandChildAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.common)?;
        
        if let Some(leaf_children) = &self.leaf_children {
            for leaf_child in leaf_children {
                write!(f, " >>>>>{}", leaf_child)?;
            }
        }
        
        Ok(())
    }
}

impl_action_node_try_from!(GreatGreatGrandChildAction, leaf_children, "leaf_actions");

type LeafActionList = Vec<LeafAction>;

impl_action_list_try_from!(
    LeafActionList,
    "leaf_action",
    "failed to convert leaf action"
);

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
struct LeafAction {
    common: CommonActionProperties,
}

impl fmt::Display for LeafAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.common)
    }
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
#[derive(PartialEq, Default, Debug, Clone, Serialize, Deserialize)]
pub struct CommonActionProperties {
    pub state: ActionState,
    pub name: ActionName,
    pub description: Option<ActionDescription>,
    pub priority: Option<ActionPriority>,
    pub context_list: Option<ContextList>,
    pub id: Option<ActionId>,
    pub do_date_time: Option<ActionDoDateTime>,
    pub completed_date_time: Option<ActionCompletedDateTime>,
}

impl fmt::Display for CommonActionProperties {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // State and name (required)
        write!(f, "({}) {}", self.state, self.name)?;
        
        // Description (optional)
        if let Some(description) = &self.description {
            write!(f, " ${}", description)?;
        }
        
        // Priority (optional)
        if let Some(priority) = &self.priority {
            write!(f, " !{}", priority)?;
        }
        
        // Context list (optional)
        if let Some(context_list) = &self.context_list {
            for context in context_list {
                write!(f, " +{}", context.trim_start_matches('@'))?;
            }
        }
        
        // Do date time (optional)
        if let Some(do_date_time) = &self.do_date_time {
            write!(f, " @{}", do_date_time.format("%Y-%m-%dT%H:%M"))?;
        }
        
        // Completed date time (optional)
        if let Some(completed_date_time) = &self.completed_date_time {
            write!(f, " %{}", completed_date_time.format("%Y-%m-%dT%H:%M"))?;
        }
        
        // ID (optional)
        if let Some(id) = &self.id {
            write!(f, " #{}", id)?;
        }
        
        Ok(())
    }
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
                "state" => match child.child(1).unwrap().kind() {
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

impl fmt::Display for ActionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state_char = match self {
            ActionState::NotStarted => " ",
            ActionState::Completed => "x",
            ActionState::InProgress => "-",
            ActionState::BlockedorAwaiting => "=",
            ActionState::Cancelled => "_",
        };
        write!(f, "{}", state_char)
    }
}
