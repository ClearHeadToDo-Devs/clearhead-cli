use clearhead_cli::entities::*;
use clearhead_cli::*;
use std::fs;
use std::path::PathBuf;

/// Helper to read example file content
fn read_example(filename: &str) -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let example_path = manifest_dir.join("examples").join(filename);
    fs::read_to_string(&example_path)
        .expect(&format!("Failed to read example file: {}", filename))
}

/// Test parsing a minimal action
#[test]
fn parse_minimal_action_from_grammar_examples() {
    let test_action = read_example("minimal.actions");
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
    let test_action = read_example("with_children.actions");
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
    let test_action = read_example("with_description.actions");
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
    let test_action = read_example("with_priority.actions");
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
    let test_action = read_example("with_story.actions");
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
    let test_action = read_example("with_context.actions");
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
    let test_action = read_example("with_id_no_dash.actions");
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
    let test_action = read_example("with_everything.actions");
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

// Round-trip tests: parse → format → parse → verify equivalence

/// Test round-trip conversion for a minimal action
#[test]
fn roundtrip_minimal_action() {
    let original = read_example("minimal.actions");
    let test_config = serde_json::json!({});

    // Parse original
    let actions1 = get_action_list_struct(&test_config, &original).unwrap();

    // Format back to string
    let formatted = clearhead_cli::format(&actions1, clearhead_cli::OutputFormat::Actions).unwrap();

    // Parse the formatted string
    let actions2 = get_action_list_struct(&test_config, &formatted.trim()).unwrap();

    // Verify equivalence
    assert_eq!(actions1.len(), actions2.len());
    assert_eq!(actions1[0].id, actions2[0].id);
    assert_eq!(actions1[0].state, actions2[0].state);
    assert_eq!(actions1[0].name, actions2[0].name);
}

/// Test round-trip conversion for actions with children
#[test]
fn roundtrip_action_with_children() {
    let original = read_example("with_children.actions");
    let test_config = serde_json::json!({});

    // Parse original
    let actions1 = get_action_list_struct(&test_config, &original).unwrap();

    // Format back to string
    let formatted = clearhead_cli::format(&actions1, clearhead_cli::OutputFormat::Actions).unwrap();

    // Parse the formatted string
    let actions2 = get_action_list_struct(&test_config, &formatted.trim()).unwrap();

    // Verify equivalence - should have same number of actions
    assert_eq!(actions1.len(), actions2.len());

    // Verify each action matches
    for (a1, a2) in actions1.iter().zip(actions2.iter()) {
        assert_eq!(a1.id, a2.id);
        assert_eq!(a1.parent_id, a2.parent_id);
        assert_eq!(a1.state, a2.state);
        assert_eq!(a1.name, a2.name);
    }
}

/// Test round-trip conversion for action with metadata
#[test]
fn roundtrip_action_with_metadata() {
    let original = read_example("with_description.actions");
    let test_config = serde_json::json!({});

    // Parse original
    let actions1 = get_action_list_struct(&test_config, &original).unwrap();

    // Format back to string
    let formatted = clearhead_cli::format(&actions1, clearhead_cli::OutputFormat::Actions).unwrap();

    // Parse the formatted string
    let actions2 = get_action_list_struct(&test_config, &formatted.trim()).unwrap();

    // Verify equivalence
    assert_eq!(actions1.len(), actions2.len());
    assert_eq!(actions1[0].id, actions2[0].id);
    assert_eq!(actions1[0].state, actions2[0].state);
    assert_eq!(actions1[0].name, actions2[0].name);
    assert_eq!(actions1[0].description, actions2[0].description);
}

/// Print formatted output for visual inspection
#[test]
fn show_formatted_output() {
    let original = read_example("with_children.actions");
    let test_config = serde_json::json!({});
    let actions = get_action_list_struct(&test_config, &original).unwrap();
    let formatted = clearhead_cli::format(&actions, clearhead_cli::OutputFormat::Actions).unwrap();

    eprintln!("\n=== Formatted .actions file ===");
    eprintln!("{}", formatted);
    eprintln!("=== End ===\n");
}

/// Test round-trip conversion for comprehensive action with everything
#[test]
fn roundtrip_action_with_everything() {
    let original = read_example("with_everything.actions");
    let test_config = serde_json::json!({});

    // Parse original
    let actions1 = get_action_list_struct(&test_config, &original).unwrap();

    // Format back to string
    let formatted = clearhead_cli::format(&actions1, clearhead_cli::OutputFormat::Actions).unwrap();

    // Parse the formatted string
    let actions2 = get_action_list_struct(&test_config, &formatted.trim()).unwrap();

    // Verify equivalence - all 6 actions
    assert_eq!(actions1.len(), actions2.len());

    // Verify each action's core properties match
    for (a1, a2) in actions1.iter().zip(actions2.iter()) {
        assert_eq!(a1.id, a2.id, "IDs should match");
        assert_eq!(a1.parent_id, a2.parent_id, "Parent IDs should match");
        assert_eq!(a1.state, a2.state, "States should match");
        assert_eq!(a1.name, a2.name, "Names should match");
        assert_eq!(a1.description, a2.description, "Descriptions should match");
        assert_eq!(a1.priority, a2.priority, "Priorities should match");
        assert_eq!(a1.context_list, a2.context_list, "Context lists should match");
        assert_eq!(a1.story, a2.story, "Stories should match");
    }
}
