use cliche::entities::*;
use cliche::*;
use tree_sitter_actions::get_test_data;

/// Test parsing a minimal action using the grammar's built-in test data
#[test]
fn parse_minimal_action_from_grammar_examples() {
    let test_action = get_test_data()["children"]["minimal"]["content"].clone();
    let test_config = serde_json::json!({});

    let actions = get_action_list_struct(&test_config, &test_action).unwrap();

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].state, ActionState::NotStarted);
    assert_eq!(actions[0].name, "test");
    assert_eq!(actions[0].parent_id, None);
}

/// Test parsing an action with children hierarchy
#[test]
fn parse_action_with_children_from_grammar_examples() {
    let test_action = get_test_data()["children"]["with_children"]["content"].clone();
    let test_config = serde_json::json!({});

    let actions = get_action_list_struct(&test_config, &test_action).unwrap();

    // Should have 1 root + 2 children + 1 grandchild = 4 total
    assert_eq!(actions.len(), 4);

    // Check root action
    let root = &actions[0];
    assert_eq!(root.state, ActionState::Completed);
    assert_eq!(root.name, "Parent task");
    assert_eq!(root.parent_id, None);

    // Check first child
    let child1 = &actions[1];
    assert_eq!(child1.state, ActionState::NotStarted);
    assert_eq!(child1.name, "Child task one");
    assert_eq!(child1.parent_id, Some(root.id));

    // Check grandchild
    let grandchild = &actions[2];
    assert_eq!(grandchild.state, ActionState::NotStarted);
    assert_eq!(grandchild.name, "Grandchild task");
    assert_eq!(grandchild.parent_id, Some(child1.id));

    // Check second child
    let child2 = &actions[3];
    assert_eq!(child2.state, ActionState::NotStarted);
    assert_eq!(child2.name, "Child task two");
    assert_eq!(child2.parent_id, Some(root.id));
}

/// Test parsing action with description
#[test]
fn parse_action_with_description_from_grammar_examples() {
    let test_action = get_test_data()["properties"]["with_description"]["content"].clone();
    let test_config = serde_json::json!({});

    let actions = get_action_list_struct(&test_config, &test_action).unwrap();

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].state, ActionState::Completed);
    assert_eq!(actions[0].name, "buy milk");
    assert_eq!(actions[0].description.as_ref().map(|s| s.trim()), Some("from the organic section"));
}

/// Test parsing action with priority
#[test]
fn parse_action_with_priority_from_grammar_examples() {
    let test_action = get_test_data()["properties"]["with_priority"]["content"].clone();
    let test_config = serde_json::json!({});

    let actions = get_action_list_struct(&test_config, &test_action).unwrap();

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].state, ActionState::Completed);
    assert_eq!(actions[0].name, "buy groceries");
    assert_eq!(actions[0].priority, Some(1));
}

/// Test parsing action with story/project
#[test]
fn parse_action_with_story_from_grammar_examples() {
    let test_action = get_test_data()["properties"]["with_story"]["content"].clone();
    let test_config = serde_json::json!({});

    let actions = get_action_list_struct(&test_config, &test_action).unwrap();

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].state, ActionState::Completed);
    assert_eq!(actions[0].name, "story test");
    assert_eq!(actions[0].story.as_ref().map(|s| s.trim()), Some("Parent Story"));
}

/// Test parsing action with context tags
#[test]
fn parse_action_with_context_from_grammar_examples() {
    let test_action = get_test_data()["properties"]["with_context"]["content"].clone();
    let test_config = serde_json::json!({});

    let actions = get_action_list_struct(&test_config, &test_action).unwrap();

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].state, ActionState::NotStarted);
    assert_eq!(actions[0].name, "send email");

    let contexts = actions[0].context_list.as_ref().unwrap();
    assert_eq!(contexts.len(), 2);
    assert!(contexts.contains(&"office".to_string()));
    assert!(contexts.contains(&"computer".to_string()));
}

/// Test parsing action with ID
#[test]
fn parse_action_with_id_from_grammar_examples() {
    let test_action = get_test_data()["properties"]["with_id_no_dash"]["content"].clone();
    let test_config = serde_json::json!({});

    let actions = get_action_list_struct(&test_config, &test_action).unwrap();

    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].state, ActionState::Completed);
    assert_eq!(actions[0].name, "task with id");
    assert!(actions[0].id.to_string().starts_with("01951111"));
}

/// Test parsing comprehensive action with all metadata
#[test]
fn parse_action_with_everything_from_grammar_examples() {
    let test_action = get_test_data()["actions"]["with_everything"]["content"].clone();
    let test_config = serde_json::json!({});

    let actions = get_action_list_struct(&test_config, &test_action).unwrap();

    // Should have root + 5 nested children = 6 total
    assert_eq!(actions.len(), 6);

    // Check root has all metadata
    let root = &actions[0];
    assert_eq!(root.state, ActionState::Completed);
    assert_eq!(root.name, "Mega Action");
    assert!(root.description.is_some());
    assert_eq!(root.priority, Some(1));
    assert!(root.story.is_some());
    assert!(root.context_list.is_some());
    assert!(root.id.to_string().starts_with("01951111"));
}
