#![allow(dead_code)]
use assert_cmd::Command;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Isolated environment with temp XDG directories for CLI integration tests.
pub struct TestEnv {
    pub _temp_dir: TempDir,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub work_dir: PathBuf,
}

impl TestEnv {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_dir = temp_dir.path().join("config/clearhead");
        let data_dir = temp_dir.path().join("data/clearhead");
        let state_dir = temp_dir.path().join("state/clearhead");
        let work_dir = temp_dir.path().join("work");

        fs::create_dir_all(&config_dir).expect("Failed to create config dir");
        fs::create_dir_all(&data_dir).expect("Failed to create data dir");
        fs::create_dir_all(&state_dir).expect("Failed to create state dir");
        fs::create_dir_all(&work_dir).expect("Failed to create work dir");

        TestEnv { _temp_dir: temp_dir, config_dir, data_dir, state_dir, work_dir }
    }

    pub fn write_config(&self, content: &str) {
        let config_path = self.config_dir.join("config.json");
        fs::write(config_path, content).expect("Failed to write config");
    }

    /// Write workspace identity config so qflist queries can emit ws:charterRoot triples.
    /// Required for any test that uses `query qflist` or other location-bearing queries.
    pub fn with_workspace_identity(&self) -> &Self {
        self.write_config(r#"{"workspace_id":"test-workspace","workspace_name":"test"}"#);
        self
    }

    pub fn write_actions(&self, filename: &str, content: &str) {
        let path = self.data_dir.join("charters").join(filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create actions parent dir");
        }
        fs::write(path, content).expect("Failed to write actions file");
    }

    pub fn write_ics(&self, filename: &str, summaries: &[&str]) {
        let content = Self::ics_content(summaries);
        let path = self.data_dir.join("charters").join(filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create ics parent dir");
        }
        fs::write(path, content).expect("Failed to write ics file");
    }

    pub fn write_plan_ics(&self, charter_slug: &str, filename: &str, summaries: &[&str]) {
        let content = Self::ics_content(summaries);
        let path = self.data_dir.join("plans").join(charter_slug).join(filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create plan ics parent dir");
        }
        fs::write(path, content).expect("Failed to write plan ics file");
    }

    pub fn write_text(&self, relative_path: &str, content: &str) {
        let path = self.data_dir.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent dirs");
        }
        fs::write(path, content).expect("Failed to write file");
    }

    pub fn ics_content(summaries: &[&str]) -> String {
        let mut content =
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\n".to_string();
        for (i, summary) in summaries.iter().enumerate() {
            content.push_str(&format!(
                "BEGIN:VEVENT\r\nUID:test-uid-{}@test\r\nSUMMARY:{}\r\nDTSTART:20260428T100000Z\r\nEND:VEVENT\r\n",
                i, summary
            ));
        }
        content.push_str("END:VCALENDAR\r\n");
        content
    }

    pub fn command(&self) -> Command {
        let bin = assert_cmd::cargo::cargo_bin!("clearhead");
        let mut cmd = Command::new(bin);
        cmd.env("XDG_CONFIG_HOME", self.config_dir.parent().unwrap());
        cmd.env("XDG_DATA_HOME", self.data_dir.parent().unwrap());
        cmd.env("XDG_STATE_HOME", &self.state_dir);
        cmd.current_dir(&self.work_dir);
        cmd
    }
}

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
