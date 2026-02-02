#![allow(dead_code)]
use clearhead_cli::{Action, ActionState};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

/// Helper to read a specific example file content
pub fn read_example(filename: &str) -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let example_path = manifest_dir.join("examples").join(filename);
    fs::read_to_string(&example_path).expect(&format!("Failed to read example file: {}", filename))
}

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

pub struct ActionBuilder {
    action: Action,
}

impl ActionBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            action: Action::new(name),
        }
    }

    pub fn id(mut self, id: Uuid) -> Self {
        self.action.id = id;
        self
    }

    pub fn parent(mut self, parent_id: Uuid) -> Self {
        self.action.parent_id = Some(parent_id);
        self
    }

    pub fn state(mut self, state: ActionState) -> Self {
        self.action.state = state;
        self
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.action.description = Some(desc.to_string());
        self
    }

    pub fn priority(mut self, priority: u32) -> Self {
        self.action.priority = Some(priority);
        self
    }

    pub fn context(mut self, contexts: Vec<&str>) -> Self {
        self.action.context_list = Some(contexts.iter().map(|s| s.to_string()).collect());
        self
    }

    pub fn story(mut self, story: &str) -> Self {
        self.action.story = Some(story.to_string());
        self
    }

    pub fn build(self) -> Action {
        self.action
    }
}
