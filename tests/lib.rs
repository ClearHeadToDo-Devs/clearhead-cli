use cliche::entities::*;
use cliche::*;

#[test]
fn convert_basic_action() {
    let test_action = "(x) test\n";
    let test_config_str = r#"{}"#;
    let test_config = serde_json::from_str(test_config_str).expect("unable to conver");

    let derived_struct = get_action_list_struct(&test_config, test_action).unwrap();
    let expected_struct = vec![RootAction {
        common: CommonActionProperties {
            state: ActionState::Completed,
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
