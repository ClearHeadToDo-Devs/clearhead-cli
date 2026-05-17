//! Action mutation utilities
//!
//! Provides reference resolution and update operations for actions.
//! Used by CLI commands (add, update, complete, delete) and can be
//! used programmatically by other consumers of the library.

use clearhead_core::{Action, ActionList, ActionState};

/// Updates to apply to an action
///
/// All fields are optional - only `Some` values are applied.
/// This allows partial updates without needing to specify unchanged fields.
#[derive(Debug, Clone, Default)]
pub struct ActionUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub priority: Option<u32>,
    pub context: Option<Vec<String>>,
    pub alias: Option<String>,
    pub state: Option<ActionState>,
}

/// Result of resolving an action reference
#[derive(Debug, Clone)]
pub struct ResolvedAction {
    /// Index into the ActionList
    pub index: usize,
    /// How the reference was matched
    pub match_type: MatchType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchType {
    /// Matched by full UUID (36 chars with hyphens)
    FullUuid,
    /// Matched by short UUID prefix (8 hex chars)
    ShortUuid,
    /// Matched by alias
    Alias,
    /// Matched by name (case-insensitive)
    Name,
}

/// Resolve a reference to an action following the spec's resolution order:
/// 1. Full UUID match (36 characters with hyphens)
/// 2. Short UUID match (8 hex character prefix)
/// 3. Alias match (case-insensitive)
/// 4. Name match (case-insensitive, partial match)
///
/// Returns the first match found, or None if no action matches.
pub fn resolve_reference(actions: &ActionList, query: &str) -> Option<ResolvedAction> {
    let query_lower = query.to_lowercase();

    // 1. Full UUID match (36 chars: 8-4-4-4-12 with hyphens)
    if query.len() == 36 && query.chars().filter(|c| *c == '-').count() == 4 {
        for (i, action) in actions.iter().enumerate() {
            if action.id.to_string() == query {
                return Some(ResolvedAction {
                    index: i,
                    match_type: MatchType::FullUuid,
                });
            }
        }
    }

    // 2. Short UUID match (8 hex chars prefix)
    if query.len() == 8 && query.chars().all(|c| c.is_ascii_hexdigit()) {
        for (i, action) in actions.iter().enumerate() {
            if action.id.to_string().starts_with(query) {
                return Some(ResolvedAction {
                    index: i,
                    match_type: MatchType::ShortUuid,
                });
            }
        }
    }

    // 3. Alias match (case-insensitive)
    for (i, action) in actions.iter().enumerate() {
        if let Some(ref alias) = action.alias {
            if alias.to_lowercase() == query_lower {
                return Some(ResolvedAction {
                    index: i,
                    match_type: MatchType::Alias,
                });
            }
        }
    }

    // 4. Name match (case-insensitive, partial/contains match)
    for (i, action) in actions.iter().enumerate() {
        if action.name.to_lowercase().contains(&query_lower) {
            return Some(ResolvedAction {
                index: i,
                match_type: MatchType::Name,
            });
        }
    }

    None
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
    fn test_resolve_by_name() {
        let actions = vec![
            make_action("Buy groceries", None),
            make_action("Fix bug", None),
        ];

        let result = resolve_reference(&actions, "groceries");
        assert!(result.is_some());
        assert_eq!(result.unwrap().match_type, MatchType::Name);
    }

    #[test]
    fn test_resolve_by_alias() {
        let actions = vec![
            make_action("Deploy to staging", Some("staging")),
            make_action("Fix bug", None),
        ];

        let result = resolve_reference(&actions, "staging");
        assert!(result.is_some());
        assert_eq!(result.unwrap().match_type, MatchType::Alias);
    }

    #[test]
    fn test_resolve_alias_before_name() {
        // If "staging" is both an alias and part of a name, alias wins
        let actions = vec![
            make_action("Fix staging server", None),
            make_action("Deploy", Some("staging")),
        ];

        let result = resolve_reference(&actions, "staging");
        assert!(result.is_some());
        let resolved = result.unwrap();
        assert_eq!(resolved.match_type, MatchType::Alias);
        assert_eq!(resolved.index, 1);
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
