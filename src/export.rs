use chrono::{DateTime, Local, Utc};
use clearhead_core::domain::Action;
use clearhead_core::{ActionState, DomainModel, Plan};
use icalendar::{Calendar, Component, Event, EventLike, EventStatus};

// ============================================================================
// Pure Calendar Export Functions (Independently Testable)
// ============================================================================

/// Calculate event start and end times from scheduled_at and duration.
///
/// Duration resolution order: action duration → default 15 min.
pub fn calculate_event_times(
    scheduled_at: DateTime<Local>,
    duration: Option<u32>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = scheduled_at.with_timezone(&Utc);
    let duration_minutes = duration.unwrap_or(15);
    let end = start + chrono::Duration::minutes(duration_minutes as i64);
    (start, end)
}

/// Map ClearHead priority (1-5) to iCalendar priority (1-9).
///
/// Mapping:
/// - ClearHead 1 (highest) → iCal 1
/// - ClearHead 2 → iCal 3
/// - ClearHead 3 → iCal 5
/// - ClearHead 4 → iCal 7
/// - ClearHead 5 or other → iCal 5 (default to medium)
pub fn map_priority_to_ical(priority: u32) -> u32 {
    match priority {
        1 => 1,
        2 => 3,
        3 => 5,
        4 => 7,
        _ => 5,
    }
}

/// Map ActionState to iCalendar EventStatus.
pub fn map_state_to_event_status(state: ActionState) -> EventStatus {
    match state {
        ActionState::NotStarted => EventStatus::Tentative,
        ActionState::InProgress => EventStatus::Confirmed,
        ActionState::Completed => EventStatus::Confirmed,
        ActionState::BlockedOrAwaiting => EventStatus::Tentative,
        ActionState::Cancelled => EventStatus::Cancelled,
    }
}

/// Check if an Action should be included in calendar export.
///
/// - Must have `scheduled_at` to anchor the event in time.
/// - When `open_only`, excludes Completed and Cancelled actions.
pub fn should_include_action(action: &Action, open_only: bool) -> bool {
    if action.scheduled_at.is_none() {
        return false;
    }

    if open_only {
        matches!(
            action.state,
            ActionState::NotStarted | ActionState::InProgress | ActionState::BlockedOrAwaiting
        )
    } else {
        true
    }
}

fn build_ical_event(
    action: &Action,
    summary: &str,
    description: Option<&str>,
    priority: Option<u32>,
    contexts: Option<&Vec<String>>,
    recurrence_rrule: Option<&str>,
) -> Option<Event> {
    let scheduled_at = action.scheduled_at?;

    let mut event = Event::new();

    // UID: use action ID — each occurrence is a distinct event
    event.uid(&action.id.to_string());
    event.summary(summary);

    if let Some(description) = description {
        event.description(description);
    }

    let (start, end) = calculate_event_times(scheduled_at, action.duration);
    event.starts(start);
    event.ends(end);

    if let Some(rrule) = recurrence_rrule {
        event.add_property("RRULE", rrule);
    }

    event.status(map_state_to_event_status(action.state));

    if action.state == ActionState::Completed {
        if let Some(completed_at) = action.completed_at {
            event.timestamp(completed_at.with_timezone(&Utc));
        }
    }

    if let Some(priority) = priority {
        event.priority(map_priority_to_ical(priority));
    }

    if let Some(contexts) = contexts {
        event.add_property("CATEGORIES", &contexts.join(","));
    }

    Some(event)
}

/// Convert a Plan + Action pair to an iCalendar Event.
///
/// The Plan supplies the "what" (SUMMARY, DESCRIPTION, PRIORITY, CATEGORIES, RRULE).
/// The Action supplies the "when" (DTSTART, DTEND, STATUS, COMPLETED).
///
/// Returns `None` if the action has no `scheduled_at`.
pub fn action_to_ical_event(plan: &Plan, action: &Action) -> Option<Event> {
    let recurrence_rrule = plan.recurrence.as_ref().map(|recurrence| {
        let rrule_str = recurrence.to_string();
        rrule_str.strip_prefix("R:").unwrap_or(&rrule_str).to_string()
    });

    build_ical_event(
        action,
        &plan.name,
        plan.description.as_deref(),
        action.priority,
        action.contexts.as_ref(),
        recurrence_rrule.as_deref(),
    )
}

fn direct_action_to_ical_event(action: &Action) -> Option<Event> {
    build_ical_event(
        action,
        &action.name,
        action.description.as_deref(),
        action.priority,
        action.contexts.as_ref(),
        None,
    )
}

/// Format a DomainModel as iCalendar (.ics).
///
/// Walks charter → plan → actions hierarchy:
/// - **Recurring plans**: emits one master VEVENT using the first scheduled action
///   as DTSTART, with the RRULE from the Plan. Calendar apps expand occurrences.
/// - **Non-recurring plans**: emits one VEVENT per scheduled Action.
///
/// Only actions with `scheduled_at` produce VEVENTs. Pass `open_only = true` to
/// exclude Completed and Cancelled actions.
pub fn format_as_icalendar(model: &DomainModel, open_only: bool) -> Result<String, String> {
    let mut calendar = Calendar::new()
        .name("ClearHead Actions")
        .description("Actions exported from ClearHead")
        .done();

    for charter in &model.charters {
        for plan in &charter.plans {
            let plan_actions: Vec<&Action> = charter
                .actions
                .iter()
                .filter(|a| a.plan_id == Some(plan.id))
                .collect();

            if plan.recurrence.is_some() {
                // Recurring plan: the first scheduled action becomes the master VEVENT.
                // RRULE lives on the Plan (information content), so it's emitted once.
                if let Some(action) = plan_actions.iter().find(|a| a.scheduled_at.is_some()) {
                    if should_include_action(action, open_only) {
                        if let Some(event) = action_to_ical_event(plan, action) {
                            calendar.push(event);
                        }
                    }
                }
            } else {
                // Non-recurring: each scheduled action is its own VEVENT
                for action in plan_actions {
                    if !should_include_action(action, open_only) {
                        continue;
                    }
                    if let Some(event) = action_to_ical_event(plan, action) {
                        calendar.push(event);
                    }
                }
            }
        }

        for action in charter.actions.iter().filter(|a| a.plan_id.is_none()) {
            if !should_include_action(action, open_only) {
                continue;
            }
            if let Some(event) = direct_action_to_ical_event(action) {
                calendar.push(event);
            }
        }
    }

    Ok(calendar.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use clearhead_core::domain::Recurrence;
    use uuid::Uuid;

    fn make_plan(name: &str) -> Plan {
        Plan {
            id: Uuid::new_v4(),
            name: name.to_string(),
            ..Default::default()
        }
    }

    fn make_action(
        plan_id: Uuid,
        state: ActionState,
        scheduled_at: Option<DateTime<Local>>,
    ) -> Action {
        Action {
            id: Uuid::new_v5(&plan_id, b"act-0"),
            plan_id: Some(plan_id),
            state,
            scheduled_at,
            ..Default::default()
        }
    }

    #[test]
    fn test_calculate_event_times_default_duration() {
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let (start, end) = calculate_event_times(dt, None);

        assert_eq!(start, dt.with_timezone(&Utc));
        assert_eq!(
            end,
            (dt + chrono::Duration::minutes(15)).with_timezone(&Utc)
        );
    }

    #[test]
    fn test_calculate_event_times_custom_duration() {
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let (start, end) = calculate_event_times(dt, Some(60));

        assert_eq!(start, dt.with_timezone(&Utc));
        assert_eq!(
            end,
            (dt + chrono::Duration::minutes(60)).with_timezone(&Utc)
        );
    }

    #[test]
    fn test_map_priority_to_ical() {
        assert_eq!(map_priority_to_ical(1), 1);
        assert_eq!(map_priority_to_ical(2), 3);
        assert_eq!(map_priority_to_ical(3), 5);
        assert_eq!(map_priority_to_ical(4), 7);
        assert_eq!(map_priority_to_ical(5), 5);
        assert_eq!(map_priority_to_ical(99), 5);
    }

    #[test]
    fn test_map_state_to_event_status() {
        assert_eq!(
            map_state_to_event_status(ActionState::NotStarted),
            EventStatus::Tentative
        );
        assert_eq!(
            map_state_to_event_status(ActionState::InProgress),
            EventStatus::Confirmed
        );
        assert_eq!(
            map_state_to_event_status(ActionState::Completed),
            EventStatus::Confirmed
        );
        assert_eq!(
            map_state_to_event_status(ActionState::BlockedOrAwaiting),
            EventStatus::Tentative
        );
        assert_eq!(
            map_state_to_event_status(ActionState::Cancelled),
            EventStatus::Cancelled
        );
    }

    #[test]
    fn test_should_include_action_no_scheduled_at() {
        let plan = make_plan("Task");
        let action = make_action(plan.id, ActionState::NotStarted, None);
        assert!(!should_include_action(&action, false));
        assert!(!should_include_action(&action, true));
    }

    #[test]
    fn test_should_include_action_with_scheduled_at() {
        let plan = make_plan("Task");
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let action = make_action(plan.id, ActionState::NotStarted, Some(dt));
        assert!(should_include_action(&action, false));
        assert!(should_include_action(&action, true));
    }

    #[test]
    fn test_should_include_action_open_only_excludes_completed() {
        let plan = make_plan("Task");
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let action = make_action(plan.id, ActionState::Completed, Some(dt));
        assert!(should_include_action(&action, false));
        assert!(!should_include_action(&action, true));
    }

    #[test]
    fn test_action_to_ical_event_basic() {
        let mut plan = make_plan("Test Event");
        plan.description = Some("Test description".to_string());
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let action = make_action(plan.id, ActionState::NotStarted, Some(dt));

        let event = action_to_ical_event(&plan, &action).expect("Event should be created");
        let event_str = event.to_string();

        assert!(event_str.contains("Test Event"));
        assert!(event_str.contains("Test description"));
    }

    #[test]
    fn test_action_to_ical_event_no_scheduled_at() {
        let plan = make_plan("Task");
        let action = make_action(plan.id, ActionState::NotStarted, None);
        assert!(action_to_ical_event(&plan, &action).is_none());
    }

    #[test]
    fn test_action_to_ical_event_carries_rrule_from_plan() {
        let mut plan = make_plan("Daily standup");
        plan.recurrence = Some(Recurrence {
            frequency: "daily".to_string(),
            interval: None,
            count: None,
            until: None,
            by_second: None,
            by_minute: None,
            by_hour: None,
            by_day: Some(vec!["MO".to_string(), "TU".to_string()]),
            by_month_day: None,
            by_year_day: None,
            by_week_no: None,
            by_month: None,
            by_set_pos: None,
            week_start: None,
        });
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 9, 0, 0).unwrap();
        let action = make_action(plan.id, ActionState::NotStarted, Some(dt));

        let event = action_to_ical_event(&plan, &action).expect("Event should be created");
        let event_str = event.to_string();

        assert!(event_str.contains("RRULE:FREQ=DAILY;BYDAY=MO,TU"));
    }

    #[test]
    fn test_action_duration_drives_calendar_length() {
        let plan = make_plan("Task");
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 9, 0, 0).unwrap();
        let mut action = make_action(plan.id, ActionState::NotStarted, Some(dt));
        action.duration = Some(30);

        let (start, end) = calculate_event_times(dt, action.duration);
        assert_eq!(end - start, chrono::Duration::minutes(30));
    }
}
