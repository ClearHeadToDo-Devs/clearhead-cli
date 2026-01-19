use crate::entities::{Action, ActionList, ActionState};
use chrono::{DateTime, Local, Utc};
use icalendar::{Calendar, Component, Event, EventLike, EventStatus};

// ============================================================================
// Pure Calendar Export Functions (Independently Testable)
// ============================================================================

/// Calculate event start and end times from action datetime and duration
///
/// # Arguments
/// * `do_datetime` - The action's scheduled time
/// * `duration` - Duration in minutes (default: 15)
///
/// # Returns
/// Tuple of (start_time_utc, end_time_utc)
pub fn calculate_event_times(
    do_datetime: DateTime<Local>,
    duration: Option<u32>,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let start_time_utc = do_datetime.with_timezone(&Utc);
    let duration_minutes = duration.unwrap_or(15);
    let end_time_utc = start_time_utc + chrono::Duration::minutes(duration_minutes as i64);
    (start_time_utc, end_time_utc)
}

/// Map ClearHead priority (1-5) to iCalendar priority (1-9)
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
        _ => 5, // default to medium
    }
}

/// Map ClearHead ActionState to iCalendar EventStatus
pub fn map_state_to_event_status(state: ActionState) -> EventStatus {
    match state {
        ActionState::NotStarted => EventStatus::Tentative,
        ActionState::InProgress => EventStatus::Confirmed,
        ActionState::Completed => EventStatus::Confirmed,
        ActionState::BlockedorAwaiting => EventStatus::Tentative,
        ActionState::Cancelled => EventStatus::Cancelled,
    }
}

/// Check if an action should be included in calendar export
///
/// # Arguments
/// * `action` - The action to check
/// * `open_only` - If true, only include NotStarted, InProgress, or BlockedorAwaiting
///
/// # Returns
/// `true` if the action should be included, `false` otherwise
pub fn should_include_action(action: &Action, open_only: bool) -> bool {
    // Must have a do_date_time to be a calendar event
    if action.do_date_time.is_none() {
        return false;
    }

    // Filter by state if open_only is true
    if open_only {
        matches!(
            action.state,
            ActionState::NotStarted | ActionState::InProgress | ActionState::BlockedorAwaiting
        )
    } else {
        true
    }
}

/// Convert a ClearHead Action to an iCalendar Event
///
/// # Arguments
/// * `action` - The action to convert
///
/// # Returns
/// `Some(Event)` if the action has a do_date_time, `None` otherwise
pub fn action_to_ical_event(action: &Action) -> Option<Event> {
    // Require do_date_time
    let do_datetime = action.do_date_time?;

    let mut event = Event::new();

    // UID: required by iCalendar spec
    event.uid(&action.id.to_string());

    // SUMMARY: action name
    event.summary(&action.name);

    // DESCRIPTION: optional description
    if let Some(description) = &action.description {
        event.description(description);
    }

    // DTSTART and DTEND
    let (start_time_utc, end_time_utc) = calculate_event_times(do_datetime, action.do_duration);
    event.starts(start_time_utc);
    event.ends(end_time_utc);

    // RRULE: recurrence rule
    if let Some(recurrence) = &action.recurrence {
        let rrule_str = format!("{}", recurrence);
        // Strip the "R:" prefix that our format uses
        let rrule = rrule_str.strip_prefix("R:").unwrap_or(&rrule_str);
        event.add_property("RRULE", rrule);
    }

    // STATUS: map action state to iCalendar status
    event.status(map_state_to_event_status(action.state));

    // COMPLETED: completion date for completed actions
    if action.state == ActionState::Completed {
        if let Some(completed_date_time) = action.completed_date_time {
            event.timestamp(completed_date_time.with_timezone(&Utc));
        }
    }

    // PRIORITY: map our priority to iCalendar priority scale
    if let Some(priority) = action.priority {
        event.priority(map_priority_to_ical(priority));
    }

    // CATEGORIES: context list (comma-separated per RFC 5545)
    if let Some(context_list) = &action.context_list {
        let categories = context_list.join(",");
        event.add_property("CATEGORIES", &categories);
    }

    Some(event)
}

/// Format ActionList as iCalendar (.ics) format
///
/// Converts actions with do_date_time into VEVENT components following RFC 5545.
/// Only includes actions that have a do_date_time set.
///
/// # Arguments
/// * `list` - The ActionList to export
/// * `open_only` - If true, only export actions with state: NotStarted, InProgress, or BlockedorAwaiting
///
/// # Returns
/// A formatted iCalendar string, or an error message if formatting fails
pub fn format_as_icalendar(list: &ActionList, open_only: bool) -> Result<String, String> {
    let mut calendar = Calendar::new()
        .name("ClearHead Actions")
        .description("Actions exported from ClearHead")
        .done();

    for action in list {
        // Check if action should be included
        if !should_include_action(action, open_only) {
            continue;
        }

        // Convert action to event
        if let Some(event) = action_to_ical_event(action) {
            calendar.push(event);
        }
    }

    Ok(calendar.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // Helper to create a test action
    fn create_test_action(
        name: &str,
        state: ActionState,
        do_datetime: Option<DateTime<Local>>,
    ) -> Action {
        Action {
            state,
            do_date_time: do_datetime,
            ..Action::new(name)
        }
    }

    #[test]
    fn test_calculate_event_times_default_duration() {
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let (start, end) = calculate_event_times(dt, None);

        assert_eq!(start, dt.with_timezone(&Utc));
        assert_eq!(end, (dt + chrono::Duration::minutes(15)).with_timezone(&Utc));
    }

    #[test]
    fn test_calculate_event_times_custom_duration() {
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let (start, end) = calculate_event_times(dt, Some(60));

        assert_eq!(start, dt.with_timezone(&Utc));
        assert_eq!(end, (dt + chrono::Duration::minutes(60)).with_timezone(&Utc));
    }

    #[test]
    fn test_map_priority_to_ical() {
        assert_eq!(map_priority_to_ical(1), 1);
        assert_eq!(map_priority_to_ical(2), 3);
        assert_eq!(map_priority_to_ical(3), 5);
        assert_eq!(map_priority_to_ical(4), 7);
        assert_eq!(map_priority_to_ical(5), 5); // default
        assert_eq!(map_priority_to_ical(99), 5); // default
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
            map_state_to_event_status(ActionState::BlockedorAwaiting),
            EventStatus::Tentative
        );
        assert_eq!(
            map_state_to_event_status(ActionState::Cancelled),
            EventStatus::Cancelled
        );
    }

    #[test]
    fn test_should_include_action_no_datetime() {
        let action = create_test_action("Task", ActionState::NotStarted, None);
        assert!(!should_include_action(&action, false));
        assert!(!should_include_action(&action, true));
    }

    #[test]
    fn test_should_include_action_with_datetime() {
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let action = create_test_action("Task", ActionState::NotStarted, Some(dt));
        assert!(should_include_action(&action, false));
        assert!(should_include_action(&action, true));
    }

    #[test]
    fn test_should_include_action_open_only_completed() {
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let action = create_test_action("Task", ActionState::Completed, Some(dt));
        assert!(should_include_action(&action, false)); // include when not open_only
        assert!(!should_include_action(&action, true)); // exclude when open_only
    }

    #[test]
    fn test_action_to_ical_event_basic() {
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let mut action = create_test_action("Test Event", ActionState::NotStarted, Some(dt));
        action.description = Some("Test description".to_string());
        action.priority = Some(1);

        let event = action_to_ical_event(&action).expect("Event should be created");
        let event_str = event.to_string();

        assert!(event_str.contains("Test Event"));
        assert!(event_str.contains("Test description"));
    }

    #[test]
    fn test_action_to_ical_event_no_datetime() {
        let action = create_test_action("Task", ActionState::NotStarted, None);
        assert!(action_to_ical_event(&action).is_none());
    }
}
