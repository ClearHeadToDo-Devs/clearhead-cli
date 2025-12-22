use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::treesitter::{create_node_wrapper, get_node_text, NodeWrapper, TreeWrapper};
use uuid::Uuid;

pub type ActionList = Vec<Action>;

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub state: ActionState,
    pub name: String,
    pub description: Option<String>,
    pub priority: Option<usize>,
    pub context_list: Option<Vec<String>>,
    pub do_date_time: Option<DateTime<Local>>,
    pub completed_date_time: Option<DateTime<Local>>,
    pub story: Option<String>,
}

impl Action {
    /// Compute the depth of this action by walking up the parent chain
    pub fn depth(&self, action_list: &ActionList) -> usize {
        let mut depth = 0;
        let mut current_id = self.parent_id;
        while let Some(parent_id) = current_id {
            depth += 1;
            current_id = action_list
                .iter()
                .find(|a| a.id == parent_id)
                .and_then(|a| a.parent_id);
        }
        depth
    }
}

impl fmt::Display for Action {
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

        // ID
        write!(f, " #{}", self.id)?;

        // Story (optional, only for root actions)
        if let Some(story) = &self.story {
            write!(f, " *{}", story)?;
        }

        Ok(())
    }
}

impl TryFrom<TreeWrapper> for ActionList {
    type Error = &'static str;
    fn try_from(value: TreeWrapper) -> Result<Self, Self::Error> {
        let root = value.tree.root_node();
        let mut action_list = Vec::new();
        let mut cursor = root.walk();

        // Iterate through all root actions
        for root_action in root.children(&mut cursor) {
            if root_action.kind() == "root_action" {
                let wrapper = create_node_wrapper(root_action, value.source.clone());
                action_list.extend(parse_action_recursive(wrapper, None)?);
            }
        }

        Ok(action_list)
    }
}

/// Recursively parse an action node and all its children into a flat list
fn parse_action_recursive(
    node: NodeWrapper,
    parent_id: Option<Uuid>,
) -> Result<Vec<Action>, &'static str> {
    let mut actions = Vec::new();

    // Parse state using field access
    let state_node = node
        .node
        .child_by_field_name("state")
        .ok_or("Missing state field")?;
    let state_value_node = state_node
        .child_by_field_name("value")
        .ok_or("Missing state value field")?;
    let state = match state_value_node.kind() {
        "state_not_started" => ActionState::NotStarted,
        "state_completed" => ActionState::Completed,
        "state_in_progress" => ActionState::InProgress,
        "state_blocked" => ActionState::BlockedorAwaiting,
        "state_cancelled" => ActionState::Cancelled,
        _ => return Err("Unknown state type"),
    };

    // Parse name using field access
    let name_node = node
        .node
        .child_by_field_name("name")
        .ok_or("Missing name field")?;
    let name = get_node_text(&name_node, &node.source).trim().to_string();

    // Parse metadata fields
    let mut description = None;
    let mut priority = None;
    let mut context_list = None;
    let mut id = None;
    let mut story = None;
    let mut do_date_time = None;
    let mut completed_date_time = None;

    let mut metadata_cursor = node.node.walk();
    for metadata_node in node
        .node
        .children_by_field_name("metadata", &mut metadata_cursor)
    {
        match metadata_node.kind() {
            "description" => {
                // Get the text of the description node (skip the $ prefix)
                let desc_text = get_node_text(&metadata_node, &node.source);
                if desc_text.starts_with('$') {
                    description = Some(desc_text[1..].trim().to_string());
                }
            }
            "priority" => {
                // Get the text of the priority node (skip the ! prefix)
                let prio_text = get_node_text(&metadata_node, &node.source);
                if prio_text.starts_with('!') {
                    if let Ok(prio) = prio_text[1..].trim().parse::<usize>() {
                        priority = Some(prio);
                    }
                }
            }
            "story" => {
                // Get the text of the story node (skip the * prefix)
                let story_text = get_node_text(&metadata_node, &node.source);
                if story_text.starts_with('*') {
                    story = Some(story_text[1..].trim().to_string());
                }
            }
            "context" => {
                // Get the text of the context node (skip the + prefix)
                let context_text = get_node_text(&metadata_node, &node.source);
                if context_text.starts_with('+') {
                    let tags: Vec<String> = context_text[1..]
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                    if !tags.is_empty() {
                        context_list = Some(tags);
                    }
                }
            }
            "do_date" => {
                if let Some(_datetime_node) = metadata_node.child_by_field_name("datetime") {
                    // TODO: Parse datetime once we handle the full ISO 8601 format
                    // For now, leaving as None
                    do_date_time = None;
                }
            }
            "completed_date" => {
                if let Some(_datetime_node) = metadata_node.child_by_field_name("datetime") {
                    // TODO: Parse datetime once we handle the full ISO 8601 format
                    // For now, leaving as None
                    completed_date_time = None;
                }
            }
            "id" => {
                // Get the text of the id node (skip the # prefix)
                let id_text = get_node_text(&metadata_node, &node.source);
                if id_text.starts_with('#') {
                    if let Ok(uuid) = Uuid::parse_str(id_text[1..].trim()) {
                        id = Some(uuid);
                    }
                }
            }
            _ => {}
        }
    }

    // Generate ID if not present
    let action_id = id.unwrap_or_else(|| Uuid::new_v4());

    // Create the action
    actions.push(Action {
        id: action_id,
        parent_id,
        state,
        name,
        description,
        priority,
        context_list,
        do_date_time,
        completed_date_time,
        story,
    });

    // Recursively parse children using field access
    let mut child_cursor = node.node.walk();
    for child_node in node.node.children_by_field_name("child", &mut child_cursor) {
        let child_wrapper = create_node_wrapper(child_node, node.source.clone());
        actions.extend(parse_action_recursive(child_wrapper, Some(action_id))?);
    }

    Ok(actions)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_actions(source: &str) -> ActionList {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_actions::LANGUAGE.into())
            .expect("Failed to set language");

        let tree = parser.parse(source, None).expect("Failed to parse");
        let tree_wrapper = TreeWrapper {
            tree,
            source: source.to_string(),
        };

        tree_wrapper.try_into().expect("Failed to convert to ActionList")
    }

    #[test]
    fn test_parse_simple_action() {
        let source = "[ ] Buy milk";
        let actions = parse_actions(source);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Buy milk");
        assert_eq!(actions[0].state, ActionState::NotStarted);
        assert_eq!(actions[0].parent_id, None);
    }

    #[test]
    fn test_parse_with_metadata() {
        let source = "[x] Buy groceries $from the store !1 +shopping";
        let actions = parse_actions(source);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Buy groceries");
        assert_eq!(actions[0].state, ActionState::Completed);
        assert_eq!(actions[0].description.as_ref().map(|s| s.trim()), Some("from the store"));
        assert_eq!(actions[0].priority, Some(1));
        assert!(actions[0].context_list.is_some());
    }

    #[test]
    fn test_parse_with_children() {
        let source = "[ ] Parent action\n> [ ] Child action\n>> [ ] Grandchild action";
        let actions = parse_actions(source);

        assert_eq!(actions.len(), 3);

        // Check parent
        assert_eq!(actions[0].name, "Parent action");
        assert_eq!(actions[0].parent_id, None);

        // Check child
        assert_eq!(actions[1].name, "Child action");
        assert_eq!(actions[1].parent_id, Some(actions[0].id));

        // Check grandchild
        assert_eq!(actions[2].name, "Grandchild action");
        assert_eq!(actions[2].parent_id, Some(actions[1].id));
    }
}
