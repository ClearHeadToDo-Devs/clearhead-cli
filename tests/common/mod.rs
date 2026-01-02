use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
pub fn get_examples() -> HashMap<String, String> {
    let mut examples = HashMap::new();
    // Get the project root directory
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let examples_dir = manifest_dir.join("examples");

    // Read all files in the examples directory
    let entries = fs::read_dir(&examples_dir).expect("Failed to read examples directory");

    for entry in entries {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();

        // Only process .actions files
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("actions") {
            continue;
        }

        let example_name = path
            .file_stem()
            .and_then(|n| n.to_str())
            .expect("Invalid filename");

        // Read the file content
        let content = fs::read_to_string(&path)
            .expect(&format!("Failed to read example file: {}", example_name));

        examples.insert(example_name.to_string(), content);
    }
    return examples;
}
