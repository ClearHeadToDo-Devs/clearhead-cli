use chrono::{DateTime, Local, Utc};
use clearhead_core::{ActPhase, DomainModel, Plan, PlannedAct};
use icalendar::{Calendar, Component, Event, EventLike, EventStatus};

// ============================================================================
// Pure Calendar Export Functions (Independently Testable)
// ============================================================================

/// Calculate event start and end times from scheduled_at and duration.
///
/// Duration resolution order: act duration → plan duration → default 15 min.
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

/// Map ActPhase to iCalendar EventStatus.
pub fn map_phase_to_event_status(phase: ActPhase) -> EventStatus {
    match phase {
        ActPhase::NotStarted => EventStatus::Tentative,
        ActPhase::InProgress => EventStatus::Confirmed,
        ActPhase::Completed => EventStatus::Confirmed,
        ActPhase::Blocked => EventStatus::Tentative,
        ActPhase::Cancelled => EventStatus::Cancelled,
    }
}

/// Check if a PlannedAct should be included in calendar export.
///
/// - Must have `scheduled_at` to anchor the event in time.
/// - When `open_only`, excludes Completed and Cancelled acts.
pub fn should_include_act(act: &PlannedAct, open_only: bool) -> bool {
    if act.scheduled_at.is_none() {
        return false;
    }

    if open_only {
        matches!(
            act.phase,
            ActPhase::NotStarted | ActPhase::InProgress | ActPhase::Blocked
        )
    } else {
        true
    }
}

/// Convert a Plan + PlannedAct pair to an iCalendar Event.
///
/// The Plan supplies the "what" (SUMMARY, DESCRIPTION, PRIORITY, CATEGORIES, RRULE).
/// The PlannedAct supplies the "when" (DTSTART, DTEND, STATUS, COMPLETED).
///
/// Returns `None` if the act has no `scheduled_at`.
pub fn planned_act_to_ical_event(plan: &Plan, act: &PlannedAct) -> Option<Event> {
    let scheduled_at = act.scheduled_at?;

    let mut event = Event::new();

    // UID: use act ID — each occurrence is a distinct event
    event.uid(&act.id.to_string());

    // SUMMARY from Plan
    event.summary(&plan.name);

    // DESCRIPTION from Plan
    if let Some(description) = &plan.description {
        event.description(description);
    }

    // DTSTART / DTEND: duration is occurrence-level semantics on PlannedAct
    let (start, end) = calculate_event_times(scheduled_at, act.duration);
    event.starts(start);
    event.ends(end);

    // RRULE from Plan — recurrence is information about the plan, not the act.
    // The caller is responsible for only passing the first act when plan has recurrence
    // (making this the iCalendar master event).
    if let Some(recurrence) = &plan.recurrence {
        let rrule_str = recurrence.to_string();
        let rrule = rrule_str.strip_prefix("R:").unwrap_or(&rrule_str);
        event.add_property("RRULE", rrule);
    }

    // STATUS from PlannedAct phase
    event.status(map_phase_to_event_status(act.phase));

    // COMPLETED timestamp for finished acts
    if act.phase == ActPhase::Completed {
        if let Some(completed_at) = act.completed_at {
            event.timestamp(completed_at.with_timezone(&Utc));
        }
    }

    // PRIORITY from Plan
    if let Some(priority) = plan.priority {
        event.priority(map_priority_to_ical(priority));
    }

    // CATEGORIES from Plan contexts
    if let Some(contexts) = &plan.contexts {
        let categories = contexts.join(",");
        event.add_property("CATEGORIES", &categories);
    }

    Some(event)
}

/// Format a DomainModel as iCalendar (.ics).
///
/// Walks charter → plan → acts hierarchy:
/// - **Recurring plans**: emits one master VEVENT using the first scheduled act
///   as DTSTART, with the RRULE from the Plan. Calendar apps expand occurrences.
/// - **Non-recurring plans**: emits one VEVENT per scheduled PlannedAct.
///
/// Only acts with `scheduled_at` produce VEVENTs. Pass `open_only = true` to
/// exclude Completed and Cancelled acts.
pub fn format_as_icalendar(model: &DomainModel, open_only: bool) -> Result<String, String> {
    let mut calendar = Calendar::new()
        .name("ClearHead Actions")
        .description("Actions exported from ClearHead")
        .done();

    for charter in &model.charters {
        for plan in &charter.plans {
            let plan_acts: Vec<&PlannedAct> = charter
                .acts
                .iter()
                .filter(|a| a.plan_id == Some(plan.id))
                .collect();

            if plan.recurrence.is_some() {
                // Recurring plan: the first scheduled act becomes the master VEVENT.
                // RRULE lives on the Plan (information content), so it's emitted once.
                if let Some(act) = plan_acts.iter().find(|a| a.scheduled_at.is_some()) {
                    if should_include_act(act, open_only) {
                        if let Some(event) = planned_act_to_ical_event(plan, act) {
                            calendar.push(event);
                        }
                    }
                }
            } else {
                // Non-recurring: each scheduled act is its own VEVENT
                for act in plan_acts {
                    if !should_include_act(act, open_only) {
                        continue;
                    }
                    if let Some(event) = planned_act_to_ical_event(plan, act) {
                        calendar.push(event);
                    }
                }
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

    fn make_act(
        plan_id: Uuid,
        phase: ActPhase,
        scheduled_at: Option<DateTime<Local>>,
    ) -> PlannedAct {
        PlannedAct {
            id: Uuid::new_v5(&plan_id, b"act-0"),
            plan_id: Some(plan_id),
            phase,
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
    fn test_map_phase_to_event_status() {
        assert_eq!(
            map_phase_to_event_status(ActPhase::NotStarted),
            EventStatus::Tentative
        );
        assert_eq!(
            map_phase_to_event_status(ActPhase::InProgress),
            EventStatus::Confirmed
        );
        assert_eq!(
            map_phase_to_event_status(ActPhase::Completed),
            EventStatus::Confirmed
        );
        assert_eq!(
            map_phase_to_event_status(ActPhase::Blocked),
            EventStatus::Tentative
        );
        assert_eq!(
            map_phase_to_event_status(ActPhase::Cancelled),
            EventStatus::Cancelled
        );
    }

    #[test]
    fn test_should_include_act_no_scheduled_at() {
        let plan = make_plan("Task");
        let act = make_act(plan.id, ActPhase::NotStarted, None);
        assert!(!should_include_act(&act, false));
        assert!(!should_include_act(&act, true));
    }

    #[test]
    fn test_should_include_act_with_scheduled_at() {
        let plan = make_plan("Task");
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let act = make_act(plan.id, ActPhase::NotStarted, Some(dt));
        assert!(should_include_act(&act, false));
        assert!(should_include_act(&act, true));
    }

    #[test]
    fn test_should_include_act_open_only_excludes_completed() {
        let plan = make_plan("Task");
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let act = make_act(plan.id, ActPhase::Completed, Some(dt));
        assert!(should_include_act(&act, false));
        assert!(!should_include_act(&act, true));
    }

    #[test]
    fn test_planned_act_to_ical_event_basic() {
        let mut plan = make_plan("Test Event");
        plan.description = Some("Test description".to_string());
        plan.priority = Some(1);
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 14, 0, 0).unwrap();
        let act = make_act(plan.id, ActPhase::NotStarted, Some(dt));

        let event = planned_act_to_ical_event(&plan, &act).expect("Event should be created");
        let event_str = event.to_string();

        assert!(event_str.contains("Test Event"));
        assert!(event_str.contains("Test description"));
    }

    #[test]
    fn test_planned_act_to_ical_event_no_scheduled_at() {
        let plan = make_plan("Task");
        let act = make_act(plan.id, ActPhase::NotStarted, None);
        assert!(planned_act_to_ical_event(&plan, &act).is_none());
    }

    #[test]
    fn test_planned_act_to_ical_event_carries_rrule_from_plan() {
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
        let act = make_act(plan.id, ActPhase::NotStarted, Some(dt));

        let event = planned_act_to_ical_event(&plan, &act).expect("Event should be created");
        let event_str = event.to_string();

        assert!(event_str.contains("RRULE:FREQ=DAILY;BYDAY=MO,TU"));
    }

    #[test]
    fn test_act_duration_drives_calendar_length() {
        let plan = make_plan("Task");
        let dt = Local.with_ymd_and_hms(2025, 1, 10, 9, 0, 0).unwrap();
        let mut act = make_act(plan.id, ActPhase::NotStarted, Some(dt));
        act.duration = Some(30);

        let (start, end) = calculate_event_times(dt, act.duration);
        assert_eq!(end - start, chrono::Duration::minutes(30));
    }
}
