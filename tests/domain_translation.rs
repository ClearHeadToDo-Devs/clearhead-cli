use chrono::Local;
use clearhead_cli::{ActPhase, Action, ActionState, DomainModel, Plan, PlannedAct};
use std::collections::HashMap;
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
        depends_on: None,
        charter: None,
    };

    let act = PlannedAct {
        id: act_id,
        plan_id,
        phase: ActPhase::NotStarted,
        scheduled_at: Some(Local::now()),
        duration: Some(30),
        completed_at: None,
        created_at: Some(Local::now()),
    };

    let mut plans = HashMap::new();
    plans.insert(plan_id.to_string(), plan.clone());
    let mut acts = HashMap::new();
    acts.insert(act_id.to_string(), act.clone());

    let domain = DomainModel { plans, acts };

    // 2. Convert to ActionList (The missing function we need to implement)
    // For now, this won't compile until we add the method.
    // This test drives the implementation.
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

    assert_eq!(domain.plans.len(), 1);
    assert_eq!(domain.acts.len(), 1);

    let p = domain.plans.values().next().unwrap();
    let a = domain.acts.values().next().unwrap();

    assert_eq!(p.name, "Source Action");
    assert_eq!(p.priority, Some(2));
    assert_eq!(a.phase, ActPhase::InProgress);
}
