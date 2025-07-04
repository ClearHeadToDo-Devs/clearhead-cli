use 
#[test]
fn convert_basic_action() {
    let test_action = "(x) test\n";
    let test_tree = get_action_list_tree(test_action).unwrap();
    let expected_output = vec![RootAction {
        core: CoreActionProperties {
            name: "test".to_string(),
            state: ActionState::NotStarted,
            description: "".to_string(),
            priority: 0,
            context_list: vec![],
        },
        story: None,
        children: None,
    }];
}
