/// Integration tests for SQL query functionality
///
/// Tests examples from the canonical SQL schema specification
use clearhead_cli::{ActionList, ActionState};
use uuid::Uuid;

mod common;
use common::ActionBuilder;

fn create_test_actions() -> ActionList {
    let p1_id = Uuid::new_v4();
    let p2_id = Uuid::new_v4();
    let p3_id = Uuid::new_v4();
    let child1_id = Uuid::new_v4();
    let child2_id = Uuid::new_v4();

    vec![
        ActionBuilder::new("P1 Action")
            .id(p1_id)
            .description("High priority task")
            .priority(1)
            .context(vec!["work", "urgent"])
            .story("Sprint 1")
            .build(),
        ActionBuilder::new("P2 Action")
            .id(p2_id)
            .state(ActionState::InProgress)
            .priority(2)
            .context(vec!["work"])
            .story("Sprint 1")
            .build(),
        ActionBuilder::new("Completed Action")
            .id(p3_id)
            .state(ActionState::Completed)
            .priority(1)
            .context(vec!["work"])
            .story("Sprint 1")
            .build(),
        ActionBuilder::new("Child of P2")
            .id(child1_id)
            .parent(p2_id)
            .build(),
        ActionBuilder::new("Completed Child")
            .id(child2_id)
            .parent(p2_id)
            .state(ActionState::Completed)
            .build(),
    ]
}

#[test]
fn test_basic_priority_filter() {
    // Example from spec: "All P1 actions"
    let actions = create_test_actions();
    let filtered =
        clearhead_cli::run_sql_where(&actions, "priority = 1", None, None).expect("Query failed");

    assert_eq!(filtered.len(), 2, "Expected 2 P1 actions");
    assert!(filtered.iter().all(|a| a.priority == Some(1)));
}

#[test]
fn test_state_filter() {
    // Example from spec: "All completed actions"
    let actions = create_test_actions();
    let filtered = clearhead_cli::run_sql_where(&actions, "state = 'completed'", None, None)
        .expect("Query failed");

    assert_eq!(filtered.len(), 2, "Expected 2 completed actions");
    assert!(filtered.iter().all(|a| a.state == ActionState::Completed));
}

#[test]
fn test_context_filter() {
    // Example from spec: "Actions in 'work' context"
    let actions = create_test_actions();
    let sql = "SELECT a.id FROM actions a JOIN action_contexts c ON a.id = c.action_id WHERE c.context = 'work'";
    let filtered = clearhead_cli::run_sql_query(&actions, sql).expect("Query failed");

    assert_eq!(filtered.len(), 3, "Expected 3 actions in 'work' context");
}

#[test]
fn test_multiple_contexts() {
    // Example from spec: "Actions in 'work' OR 'urgent' context"
    let actions = create_test_actions();
    let sql = "SELECT DISTINCT a.id FROM actions a JOIN action_contexts c ON a.id = c.action_id WHERE c.context IN ('work', 'urgent')";
    let filtered = clearhead_cli::run_sql_query(&actions, sql).expect("Query failed");

    assert_eq!(
        filtered.len(),
        3,
        "Expected 3 actions with work or urgent context"
    );
}

#[test]
fn test_root_actions_only() {
    // Example from spec: "Root actions only"
    let actions = create_test_actions();
    let filtered = clearhead_cli::run_sql_where(&actions, "parent_id IS NULL", None, None)
        .expect("Query failed");

    assert_eq!(filtered.len(), 3, "Expected 3 root actions");
    assert!(filtered.iter().all(|a| a.parent_id.is_none()));
}

#[test]
fn test_children_of_action() {
    // Example from spec: "Immediate children of an action"
    let actions = create_test_actions();
    let p2_id = actions
        .iter()
        .find(|a| a.name == "P2 Action")
        .map(|a| a.id.to_string())
        .expect("P2 action not found");

    let sql = format!("SELECT id FROM actions WHERE parent_id = '{}'", p2_id);
    let filtered = clearhead_cli::run_sql_query(&actions, &sql).expect("Query failed");

    assert_eq!(filtered.len(), 2, "Expected 2 children of P2");
}

#[test]
fn test_complex_combined_query() {
    // Example from spec: "P1 actions in 'work' context"
    let actions = create_test_actions();
    let sql = "SELECT a.id FROM actions a JOIN action_contexts c ON a.id = c.action_id WHERE a.priority = 1 AND c.context = 'work'";
    let filtered = clearhead_cli::run_sql_query(&actions, sql).expect("Query failed");

    assert_eq!(filtered.len(), 2, "Expected 2 P1 actions in work context");
}

#[test]
fn test_not_started_actions() {
    let actions = create_test_actions();
    let filtered = clearhead_cli::run_sql_where(&actions, "state = 'not_started'", None, None)
        .expect("Query failed");

    assert_eq!(filtered.len(), 2, "Expected 2 not-started actions");
}

#[test]
fn test_actions_with_description() {
    let actions = create_test_actions();
    let filtered = clearhead_cli::run_sql_where(&actions, "description IS NOT NULL", None, None)
        .expect("Query failed");

    assert_eq!(filtered.len(), 1, "Expected 1 action with description");
}
