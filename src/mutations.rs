use chrono::{DateTime, Local};
use clearhead_core::{Action, ActionState, Charter, CharterState};

/// Updates to apply to a charter's metadata fields.
///
/// All fields are optional — only `Some` values are applied.
#[derive(Debug, Clone, Default)]
pub struct CharterUpdate {
    pub state: Option<CharterState>,
    pub title: Option<String>,
    pub alias: Option<String>,
}

pub fn apply_charter_update(charter: &mut Charter, update: CharterUpdate) {
    if let Some(state) = update.state {
        charter.state = Some(state);
    }
    if let Some(title) = update.title {
        charter.title = title;
    }
    if let Some(alias) = update.alias {
        charter.alias = Some(alias);
    }
}

/// Updates to apply to an action.
///
/// All fields are optional — only `Some` values are applied.
#[derive(Debug, Clone, Default)]
pub struct ActionUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub priority: Option<u32>,
    pub context: Option<Vec<String>>,
    pub is_sequential: Option<bool>,
    pub alias: Option<String>,
    pub state: Option<ActionState>,
    pub scheduled_at: Option<DateTime<Local>>,
    pub duration: Option<u32>,
}

/// Apply updates to an action
///
/// Only fields that are `Some` in the update are changed.
/// The action's ID and parent_id are never modified.
pub fn apply_updates(action: &mut Action, updates: ActionUpdate) {
    if let Some(name) = updates.name {
        action.name = name;
    }
    if let Some(description) = updates.description {
        action.description = Some(description);
    }
    if let Some(priority) = updates.priority {
        action.priority = Some(priority);
    }
    if let Some(context) = updates.context {
        action.contexts = if context.is_empty() {
            None
        } else {
            Some(context)
        };
    }
    if let Some(is_sequential) = updates.is_sequential {
        action.is_sequential = Some(is_sequential);
    }
    if let Some(alias) = updates.alias {
        action.alias = Some(alias);
    }
    if let Some(state) = updates.state {
        action.state = state;
        // If completing, set completed_at
        if state == ActionState::Completed && action.completed_at.is_none() {
            action.completed_at = Some(chrono::Local::now());
        }
    }
    if let Some(scheduled_at) = updates.scheduled_at {
        action.scheduled_at = Some(scheduled_at);
    }
    if let Some(duration) = updates.duration {
        action.duration = Some(duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_action(name: &str, alias: Option<&str>) -> Action {
        Action {
            id: Uuid::now_v7(),
            name: name.to_string(),
            alias: alias.map(|s| s.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_apply_partial_updates() {
        let mut action = make_action("Original name", None);
        action.priority = Some(3);

        apply_updates(
            &mut action,
            ActionUpdate {
                priority: Some(1),
                ..Default::default()
            },
        );

        assert_eq!(action.name, "Original name"); // unchanged
        assert_eq!(action.priority, Some(1)); // updated
    }
}
