use chrono::Local;
use clearhead_cli::{ActPhase, Action, ActionState, Charter, DomainModel, PlannedAct};
use clearhead_core::workspace::actions::convert;
use uuid::Uuid;

#[test]
fn test_domain_round_trip_simple() {
    // 1. Create a Domain Model manually (simulate CRDT state)
    let action_id = Uuid::now_v7();

    let act = PlannedAct {
        id: action_id,
        name: "Test Task".to_string(),
        description: Some("Description".to_string()),
        priority: Some(1),
        contexts: Some(vec!["ctx".to_string()]),
        phase: ActPhase::NotStarted,
        scheduled_at: Some(Local::now()),
        duration: Some(30),
        created_at: Some(Local::now()),
        ..Default::default()
    };

    let charter = Charter {
        id: Uuid::new_v4(),
        title: "inbox".to_string(),
        description: None,
        alias: None,
        parent: None,
        objectives: None,
        plans: vec![],
        actions: vec![act],
    };

    let domain = DomainModel {
        objectives: vec![],
        charters: vec![charter],
    };

    // 2. Convert to ActionList
    let actions = convert::to_action_list(&domain);

    assert_eq!(actions.len(), 1);
    let action = &actions[0];

    // 3. Verify Translation
    assert_eq!(action.id, action_id);
    assert_eq!(action.name, "Test Task");
    assert_eq!(action.state, ActionState::NotStarted);
    assert_eq!(action.description, Some("Description".to_string()));
    assert_eq!(action.priority, Some(1));
    assert_eq!(action.do_duration, Some(30));
}

#[test]
fn test_from_actions_preserves_data() {
    let action = Action {
        id: Uuid::now_v7(),
        name: "Source Action".to_string(),
        state: ActionState::InProgress,
        priority: Some(2),
        ..Default::default()
    };

    let actions: clearhead_core::ActionList = vec![action.clone()];
    let charter = convert::from_actions_with_charter(&actions, "test".to_string());
    let domain = clearhead_core::DomainModel {
        objectives: vec![],
        charters: vec![charter],
    };

    assert!(domain.all_plans().is_empty());
    assert_eq!(domain.all_acts().len(), 1);

    let a = domain.all_acts()[0];

    assert_eq!(a.name, "Source Action");
    assert_eq!(a.priority, Some(2));
    assert_eq!(a.phase, ActPhase::InProgress);
}
