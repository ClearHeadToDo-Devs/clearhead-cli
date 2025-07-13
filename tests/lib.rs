use cliche::entities::*;
use cliche::*;
use reqwest::blocking::get;
use std::collections::HashMap;

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
#[test]
fn convert_basic_action_from_examples() {
    let test_action =
        get_test_examples().unwrap()["children"]["minimal_example"]["content"].clone();
    let test_config_str = r#"{}"#;
    let test_config = serde_json::from_str(test_config_str).expect("unable to conver");

    let derived_struct = get_action_list_struct(&test_config, &test_action).unwrap();
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

fn get_test_examples() -> Result<HashMap<String, HashMap<String, HashMap<String, String>>>, String>
{
    //first, we get the list of test files, their names, and descriptions directly from the GH repo
    let list_get_response = get("https://raw.githubusercontent.com/ClearHeadToDo-Devs/tree-sitter-actions/refs/heads/master/test/test_descriptions.json").expect("unable to get url in question").text().expect("unable to get text from request");
    let binding: serde_json::Value = serde_json::from_str(&list_get_response).expect("could not parse JSON");
    let test_list = binding.as_object().expect("couldnt get map out of this");

    // Then we create the hashmap we will ultimately return
    let mut files: HashMap<String, HashMap<String, HashMap<String, String>>> = HashMap::new();

    for (filename, tests) in test_list {
        let mut file_tests: HashMap<String, HashMap<String, String>> = HashMap::new();
        for (test_name, description) in tests.as_object().expect("not a child object") {
            //get the example file that corresponds to the test name
            let test_file_contents = get(format!("https://raw.githubusercontent.com/ClearHeadToDo-Devs/tree-sitter-actions/refs/heads/master/examples/{}.actions",test_name)).expect("couldnt find example file").text().expect("unable to translate the body");
            file_tests.insert(
                test_name.to_string(),
                HashMap::from([
                    ("descripition".to_string(), description.to_string()),
                    ("content".to_string(), test_file_contents),
                ]),
            );
        }
        files.insert(filename.to_string(), file_tests);
    }
    Ok(files)
}
