use crate::entities::{ActionList, ActionState};
use crate::format::{format, OutputFormat};
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
            // Check if entire tree is completed
            let mut tree_completed = action.state == ActionState::Completed;

            if tree_completed {
                // Check descendants
                let mut stack = vec![action.id];
                while let Some(current_id) = stack.pop() {
                    let children: Vec<_> = all_actions
                        .iter()
                        .filter(|a| a.parent_id == Some(current_id))
                        .collect();
                    for child in children {
                        if child.state != ActionState::Completed {
                            tree_completed = false;
                            break;
                        }
                        stack.push(child.id);
                    }
                    if !tree_completed {
                        break;
                    }
                }
            }

            if tree_completed {
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
    pub log_path: PathBuf,
}

/// Moves completed actions from a source string to a log file
pub fn archive_actions(
    content: &str,
    _source_path: &PathBuf,
    log_dir: &PathBuf,
) -> Result<(String, ArchiveResult), String> {
    let all_actions = crate::get_action_list_struct(&serde_json::json!({}), content)?;
    let (active_actions, archived_actions) = partition_actions_for_archive(&all_actions);

    if archived_actions.is_empty() {
        return Err("No completed action trees to archive.".to_string());
    }

    // Determine log filename
    use chrono::Local;
    let now = Local::now();
    let month_str = now.format("%Y-%m").to_string();
    let log_filename = format!("{}.actions", month_str);
    let log_path = log_dir.join(log_filename);

    // Prepare log content
    let mut log_content = if log_path.exists() {
        fs::read_to_string(&log_path).unwrap_or_default()
    } else {
        String::new()
    };

    let archived_text = format(&archived_actions, OutputFormat::Actions, None, None)?;
    if !log_content.is_empty() && !log_content.ends_with('\n') {
        log_content.push('\n');
    }
    log_content.push_str(&archived_text);

    // Write to log
    fs::write(&log_path, log_content).map_err(|e| {
        format!(
            "Failed to write to log file '{}': {}",
            log_path.display(),
            e
        )
    })?;

    // Return new active content
    let active_text = format(&active_actions, OutputFormat::Actions, None, None)?;

    Ok((
        active_text,
        ArchiveResult {
            archived_count: archived_actions.len(),
            log_path,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{Action, ActionState};
    use uuid::Uuid;

    fn mock_action(id: Uuid, parent: Option<Uuid>, state: ActionState) -> Action {
        Action {
            id,
            parent_id: parent,
            state,
            name: "test".to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            created_date_time: None,
            predecessors: None,
            story: None,
            alias: None,
            is_sequential: None,
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

        // Even though parent is completed, child is not, so nothing is archived
        assert_eq!(archived.len(), 0);
        assert_eq!(active.len(), 2);
    }
}
