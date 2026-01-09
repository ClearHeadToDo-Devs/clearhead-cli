mod common;
use clearhead_cli::get_action_list_struct;
use common::get_examples;
use serde_json::json;

#[test]
fn test_snapshots_from_grammar_examples() {
    let config = json!({});

    for (example_name, content) in get_examples() {
        // Parse the content into our ActionList struct
        let actions = get_action_list_struct(&config, &content)
            .expect(&format!("Failed to parse example: {}", example_name));

        // Use insta to assert against a RON snapshot
        insta::with_settings!({
            sort_maps => true,
        }, {
            insta::assert_ron_snapshot!(example_name, actions, {
                "[].id" => "[uuid]",
                "[].parent_id" => "[uuid]",
                "[].createdDate" => "[timestamp]",
                "[].predecessors[].resolved_uuid" => "[uuid]",
                "[].predecessors[].raw_ref" => "[predecessor_ref]"
            });
        });
    }
}
