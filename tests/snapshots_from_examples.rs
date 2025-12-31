use clearhead_cli::get_action_list_struct;
use tree_sitter_actions::get_test_data;
use serde_json::json;

#[test]
fn test_snapshots_from_grammar_examples() {
    let test_data = get_test_data();
    let config = json!({});

    // We iterate through categories (children, properties, actions)
    for (category_name, category) in test_data {
        for (example_name, example) in category {
            let content = example.get("content").unwrap();
            
            // Parse the content into our ActionList struct
            let actions = get_action_list_struct(&config, content)
                .expect(&format!("Failed to parse example: {}/{}", category_name, example_name));

            // Generate a unique name for the snapshot
            let snapshot_name = format!("{}_{}", category_name, example_name);

            // Use insta to assert against a RON snapshot
            insta::with_settings!({
                sort_maps => true,
            }, {
                insta::assert_ron_snapshot!(snapshot_name, actions, {
                    "[].id" => "[uuid]",
                    "[].parent_id" => "[uuid]"
                });
            });
        }
    }
}