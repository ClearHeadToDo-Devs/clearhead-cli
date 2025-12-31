use clearhead_cli::get_action_list_struct;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

#[test]
fn test_snapshots_from_grammar_examples() {
    let config = json!({});
    
    // Get the project root directory
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let examples_dir = manifest_dir.join("examples");
    
    // Read all files in the examples directory
    let entries = fs::read_dir(&examples_dir)
        .expect("Failed to read examples directory");
    
    for entry in entries {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();
        
        // Only process .actions files
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("actions") {
            continue;
        }
        
        let example_name = path.file_stem()
            .and_then(|n| n.to_str())
            .expect("Invalid filename");
        
        // Read the file content
        let content = fs::read_to_string(&path)
            .expect(&format!("Failed to read example file: {}", example_name));
        
        // Parse the content into our ActionList struct
        let actions = get_action_list_struct(&config, &content)
            .expect(&format!("Failed to parse example: {}", example_name));
        
        // Use insta to assert against a RON snapshot
        insta::with_settings!({
            sort_maps => true,
        }, {
            insta::assert_ron_snapshot!(example_name, actions, {
                "[].id" => "[uuid]",
                "[].parent_id" => "[uuid]"
            });
        });
    }
}
