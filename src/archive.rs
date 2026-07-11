use clearhead_core::{ActionList, ActionState, OutputFormat, format};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// Partitions a list of actions into (active_actions, archived_actions)
pub fn partition_actions_for_archive(all_actions: &ActionList) -> (ActionList, ActionList) {
    // 1. Identify "archive-ready" root actions
    // A root action is archive-ready if it is completed AND all its descendants are completed
    let mut archive_root_ids = HashSet::new();

    for action in all_actions {
        if action.parent_id.is_none() {
            let mut tree_done = matches!(action.state, ActionState::Completed | ActionState::Cancelled);

            if tree_done {
                let mut stack = vec![action.id];
                while let Some(current_id) = stack.pop() {
                    let children: Vec<_> = all_actions
                        .iter()
                        .filter(|a| a.parent_id == Some(current_id))
                        .collect();
                    for child in children {
                        if !matches!(child.state, ActionState::Completed | ActionState::Cancelled) {
                            tree_done = false;
                            break;
                        }
                        stack.push(child.id);
                    }
                    if !tree_done {
                        break;
                    }
                }
            }

            if tree_done {
                archive_root_ids.insert(action.id);
            }
        }
    }

    if archive_root_ids.is_empty() {
        return (all_actions.clone(), Vec::new());
    }

    // 2. Separate actions into active and archived
    let mut active_actions = Vec::new();
    let mut archived_actions = Vec::new();

    for action in all_actions {
        // Find root of this action
        let mut current_root_id = action.id;
        let mut current_parent_id = action.parent_id;

        let mut path = HashSet::new();
        path.insert(action.id);

        while let Some(pid) = current_parent_id {
            if !path.insert(pid) {
                break;
            } // Cycle detected
            current_root_id = pid;
            current_parent_id = all_actions
                .iter()
                .find(|a| a.id == pid)
                .and_then(|a| a.parent_id);
        }

        if archive_root_ids.contains(&current_root_id) {
            archived_actions.push(action.clone());
        } else {
            active_actions.push(action.clone());
        }
    }

    (active_actions, archived_actions)
}

/// Result of an archive operation
pub struct ArchiveResult {
    pub archived_count: usize,
    pub completed_path: PathBuf,
}

/// Moves completed action trees from `source_path` into its sibling
/// `<stem>.completed.actions` file, per the workspace naming conventions.
pub fn archive_actions(
    content: &str,
    source_path: &PathBuf,
) -> Result<(String, ArchiveResult), String> {
    let all_actions = crate::get_action_list_struct(content)?;
    let (active_actions, archived_actions) = partition_actions_for_archive(&all_actions);

    if archived_actions.is_empty() {
        return Err("No completed action trees to archive.".to_string());
    }

    // Derive sibling .completed.actions path using canonical stem logic
    let completed_path = clearhead_core::completed_actions_path(source_path);

    // Append to existing .completed.actions (or create it)
    let mut completed_content = if completed_path.exists() {
        fs::read_to_string(&completed_path).unwrap_or_default()
    } else {
        String::new()
    };

    let archived_text = format(&archived_actions, OutputFormat::Actions, None, None)?;
    if !completed_content.is_empty() && !completed_content.ends_with('\n') {
        completed_content.push('\n');
    }
    completed_content.push_str(&archived_text);

    fs::write(&completed_path, completed_content)
        .map_err(|e| format!("Failed to write to '{}': {}", completed_path.display(), e))?;

    let active_text = format(&active_actions, OutputFormat::Actions, None, None)?;

    Ok((
        active_text,
        ArchiveResult {
            archived_count: archived_actions.len(),
            completed_path,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearhead_core::{Action, ActionState};
    use uuid::Uuid;

    fn mock_action(id: Uuid, parent: Option<Uuid>, state: ActionState) -> Action {
        Action {
            id,
            parent_id: parent,
            state,
            name: "test".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_partition_actions() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let actions = vec![
            mock_action(id1, None, ActionState::Completed),
            mock_action(id2, Some(id1), ActionState::Completed),
            mock_action(id3, None, ActionState::NotStarted),
        ];

        let (active, archived) = partition_actions_for_archive(&actions);

        // id1 and id2 form a completed tree, id3 is active
        assert_eq!(archived.len(), 2);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, id3);
    }

    #[test]
    fn test_partition_does_not_archive_incomplete_trees() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let actions = vec![
            mock_action(id1, None, ActionState::Completed),
            mock_action(id2, Some(id1), ActionState::NotStarted),
        ];

        let (active, archived) = partition_actions_for_archive(&actions);

        assert_eq!(archived.len(), 0);
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_partition_archives_cancelled_root_tree() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let actions = vec![
            mock_action(id1, None, ActionState::Cancelled),
            mock_action(id2, Some(id1), ActionState::Cancelled),
            mock_action(id3, None, ActionState::NotStarted),
        ];

        let (active, archived) = partition_actions_for_archive(&actions);

        assert_eq!(archived.len(), 2);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, id3);
    }

    #[test]
    fn test_partition_archives_mixed_terminal_tree() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        // Root completed, one child completed, one child cancelled — all terminal
        let actions = vec![
            mock_action(id1, None, ActionState::Completed),
            mock_action(id2, Some(id1), ActionState::Completed),
            mock_action(id3, Some(id1), ActionState::Cancelled),
        ];

        let (active, archived) = partition_actions_for_archive(&actions);

        assert_eq!(archived.len(), 3);
        assert_eq!(active.len(), 0);
    }

    #[test]
    fn test_partition_does_not_archive_cancelled_root_with_open_children() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();

        let actions = vec![
            mock_action(id1, None, ActionState::Cancelled),
            mock_action(id2, Some(id1), ActionState::NotStarted),
        ];

        let (active, archived) = partition_actions_for_archive(&actions);

        assert_eq!(archived.len(), 0);
        assert_eq!(active.len(), 2);
    }
}
