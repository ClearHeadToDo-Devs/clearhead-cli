use cliche::entities::*;
use cliche::get_action_list_tree;

#[test]
fn convert_basic_action() {
    let test_action = "( ) test\n";
    let test_tree = get_action_list_tree(test_action).unwrap();
    let expected_output = vec![]
}
