use chrono::Local;
use clearhead_cli::telemetry::TelemetryEvent;
use clearhead_cli::{Action, ActionList, ActionState};
use tracing::{debug, warn};
use uuid::Uuid;

pub struct CompletionResult {
    pub action_id: Uuid,
    pub action_name: String,
    pub is_recurring: bool,
    pub event: TelemetryEvent,
    /// Index of the completed action in the (mutated) actions list.
    /// For non-recurring: the original index (now marked complete).
    /// For recurring: the last index (newly pushed completed instance).
    pub action_index: usize,
}

/// Find and complete an action by query (ID prefix or name substring).
/// Mutates `actions` in place. For recurring actions, creates a completed instance
/// and advances the template to its next occurrence.
pub fn complete_action(actions: &mut ActionList, query: &str) -> Result<CompletionResult, String> {
    // Find the target
    let target_index = actions
        .iter()
        .position(|a| {
            let id_match = a.id.to_string().starts_with(query);
            let name_match = a.name.contains(query);
            (id_match || name_match) && a.state != ActionState::Completed
        })
        .ok_or_else(|| format!("No open action found matching '{}'", query))?;

    let action = &mut actions[target_index];
    let action_id = action.id;
    let action_name = action.name.clone();
    let is_recurring = action.recurrence.is_some();

    let mut new_instance: Option<Action> = None;

    if is_recurring {
        // Create a static completed instance
        let mut instance = action.clone();
        instance.id = Uuid::now_v7();
        instance.recurrence = None;
        instance.state = ActionState::Completed;
        instance.completed_date_time = Some(Local::now());
        new_instance = Some(instance);

        // Advance the template to the next occurrence
        let occurrences = action.expand_occurrences(2);
        if occurrences.len() > 1 {
            let next_dt = occurrences[1].with_timezone(&Local);
            action.do_date_time = Some(next_dt);
            debug!(next_occurrence = %next_dt.to_rfc3339(), "Updating template to next occurrence");
        } else {
            warn!("Could not determine next occurrence for recurring action");
        }

        action.state = ActionState::NotStarted;
        action.completed_date_time = None;
    } else {
        action.state = ActionState::Completed;
        action.completed_date_time = Some(Local::now());
    }

    if let Some(instance) = new_instance {
        actions.push(instance);
    }

    let completed_at = chrono::Utc::now().to_rfc3339();
    let event = if is_recurring {
        TelemetryEvent::InstanceCompleted {
            template_uuid: action_id.to_string(),
            scheduled_date: completed_at.clone(),
            completed_date: completed_at,
        }
    } else {
        TelemetryEvent::ActionCompleted {
            name: action_name.clone(),
            completed_at,
        }
    };

    let action_index = if is_recurring {
        actions.len() - 1 // the pushed completed instance
    } else {
        target_index
    };

    Ok(CompletionResult {
        action_id,
        action_name,
        is_recurring,
        event,
        action_index,
    })
}
