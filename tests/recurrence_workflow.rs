use clearhead_cli::entities::{Action, ActionState, Recurrence};
use uuid::Uuid;
use chrono::{Local, TimeZone};

#[test]
fn test_completing_recurrence_workflow() {
    // Setup: A recurring action template (Daily Standup)
    // Starting on 2025-01-01
    let dt_start = Local.with_ymd_and_hms(2025, 1, 1, 9, 0, 0).unwrap();
    
    let recurrence = Recurrence {
        frequency: "daily".to_string(),
        interval: Some(1),
        count: None,
        until: None,
        by_second: None,
        by_minute: None,
        by_hour: None,
        by_day: None,
        by_month_day: None,
        by_year_day: None,
        by_week_no: None,
        by_month: None,
        by_set_pos: None,
        week_start: None,
    };
    
    let template_id = Uuid::new_v4();
    let template = Action {
        id: template_id,
        do_date_time: Some(dt_start),
        recurrence: Some(recurrence),
        ..Action::new("Daily Standup")
    };

    let mut actions = vec![template];

    // Simulation of NEW "complete" command logic for recurring actions
    let idx = 0;
    let mut new_instance = None;
    
    {
        let action = &mut actions[idx];
        if action.recurrence.is_some() {
            // 1. Create a static completed instance
            let mut instance = action.clone();
            instance.id = Uuid::now_v7();
            instance.recurrence = None;
            instance.state = ActionState::Completed;
            instance.completed_date_time = Some(Local::now());
            new_instance = Some(instance);

            // 2. Update the template for the next occurrence
            let occurrences = action.expand_occurrences(2);
            let next_dt = occurrences[1].with_timezone(&Local);
            action.do_date_time = Some(next_dt);
            
            // Template stays NotStarted
            action.state = ActionState::NotStarted;
        }
    }

    if let Some(inst) = new_instance {
        actions.push(inst);
    }

    // Assertions
    assert_eq!(actions.len(), 2, "Should have 2 actions now (template + log entry)");
    
    let template = &actions[0];
    let log_entry = &actions[1];

    // Template checks
    assert_eq!(template.id, template_id);
    assert_eq!(template.state, ActionState::NotStarted, "Template should be reset to NotStarted");
    assert_eq!(template.do_date_time.unwrap().format("%Y-%m-%d").to_string(), "2025-01-02", "Template should advance to next day");
    assert!(template.recurrence.is_some(), "Template should still have recurrence rule");

    // Log entry checks
    assert_ne!(log_entry.id, template_id, "Log entry should have a new ID");
    assert_eq!(log_entry.state, ActionState::Completed, "Log entry should be Completed");
    assert_eq!(log_entry.name, "Daily Standup");
    assert_eq!(log_entry.do_date_time.unwrap().format("%Y-%m-%d").to_string(), "2025-01-01", "Log entry should keep the original instance date");
    assert!(log_entry.recurrence.is_none(), "Log entry should NOT have a recurrence rule");
}
