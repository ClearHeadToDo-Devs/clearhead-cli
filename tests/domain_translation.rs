use chrono::Local;
use clearhead_cli::{ActPhase, Action, ActionState, Charter, DomainModel, Plan, PlannedAct};
use uuid::Uuid;

#[test]
fn test_domain_round_trip_simple() {
    // 1. Create a Domain Model manually (simulate CRDT state)
    let plan_id = Uuid::now_v7();
    let act_id = Uuid::now_v7();

    let plan = Plan {
        id: plan_id,
        name: "Test Task".to_string(),
        description: Some("Description".to_string()),
        priority: Some(1),
        contexts: Some(vec!["ctx".to_string()]),
        recurrence: None,
        parent: None,
        objective: None,
        alias: None,
        is_sequential: None,
        duration: None,
        depends_on: None,
        acts: vec![PlannedAct {
            id: act_id,
            plan_id,
            phase: ActPhase::NotStarted,
            scheduled_at: Some(Local::now()),
            duration: Some(30),
            completed_at: None,
            created_at: Some(Local::now()),
        }],
    };

    let charter = Charter {
        id: Uuid::new_v4(),
        title: "inbox".to_string(),
        description: None,
        alias: None,
        parent: None,
        objectives: None,
        plans: vec![plan],
    };

    let domain = DomainModel {
        objectives: vec![],
        charters: vec![charter],
    };

    // 2. Convert to ActionList
    let actions = domain.to_action_list();

    assert_eq!(actions.len(), 1);
    let action = &actions[0];

    // 3. Verify Translation
    assert_eq!(
        action.id, plan_id,
        "Action ID should match Plan ID for singleton"
    );
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

    let domain = DomainModel::from_actions(&vec![action.clone()]);

    assert_eq!(domain.all_plans().len(), 1);
    assert_eq!(domain.all_acts().len(), 1);

    let p = domain.all_plans()[0];
    let a = domain.all_acts()[0];

    assert_eq!(p.name, "Source Action");
    assert_eq!(p.priority, Some(2));
    assert_eq!(a.phase, ActPhase::InProgress);
}
