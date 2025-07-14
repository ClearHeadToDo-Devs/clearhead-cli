use cliche::entities::*;
use cliche::*;
use tree_sitter_actions::get_test_data;

// here, we are making use of the automatically generated test case file which we dynamically build
// at build time, we are only doing one test for now but the plan is that all tests covered within
// the primary structure of treesitter will be covered here to ensure parity
#[test]
fn convert_basic_action_from_examples() {
    let test_action = get_test_data()["children"]["minimal"]["content"].clone();
    let test_config_str = r#"{}"#;
    let test_config = serde_json::from_str(test_config_str).expect("unable to conver");

    let derived_struct = get_action_list_struct(&test_config, &test_action).unwrap();
    let expected_struct = vec![RootAction {
        common: CommonActionProperties {
            state: ActionState::NotStarted,
            name: "test".to_string(),
            description: None,
            priority: None,
            context_list: None,
            id: None,
            do_date_time: None,
            completed_date_time: None,
        },
        story: None,
        children: None,
    }];

    assert_eq!(derived_struct, expected_struct);
}
