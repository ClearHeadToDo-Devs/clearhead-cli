use chrono::Local;
use clearhead_cli::telemetry::TelemetryEvent;
use clearhead_cli::{ActionList, ActionState};
use uuid::Uuid;

#[allow(dead_code)]
pub struct CompletionResult {
    pub action_id: Uuid,
    pub action_name: String,
    pub event: TelemetryEvent,
    /// Index of the completed action in the (mutated) actions list.
    pub action_index: usize,
}

/// Find and complete an action by query (ID prefix or name substring).
/// Mutates `actions` in place.
#[allow(dead_code)]
pub fn complete_action(actions: &mut ActionList, query: &str) -> Result<CompletionResult, String> {
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

    action.state = ActionState::Completed;
    action.completed_date_time = Some(Local::now());

    let completed_at = chrono::Utc::now().to_rfc3339();
    let event = TelemetryEvent::ActionCompleted {
        name: action_name.clone(),
        completed_at,
    };

    Ok(CompletionResult {
        action_id,
        action_name,
        event,
        action_index: target_index,
    })
}
