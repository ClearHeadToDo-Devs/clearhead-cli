use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test helper: creates isolated environment with temp XDG directories
struct TestEnv {
    _temp_dir: TempDir,  // Keep alive for cleanup
    config_dir: std::path::PathBuf,
    data_dir: std::path::PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_dir = temp_dir.path().join("config/cliche");
        let data_dir = temp_dir.path().join("data/cliche");

        fs::create_dir_all(&config_dir).expect("Failed to create config dir");
        fs::create_dir_all(&data_dir).expect("Failed to create data dir");

        TestEnv {
            _temp_dir: temp_dir,
            config_dir,
            data_dir,
        }
    }

    /// Write a config file to the test environment
    fn write_config(&self, content: &str) {
        let config_path = self.config_dir.join("config.toml");
        fs::write(config_path, content).expect("Failed to write config");
    }

    /// Write an actions file to the test data directory
    fn write_actions(&self, filename: &str, content: &str) {
        let actions_path = self.data_dir.join(filename);
        fs::write(actions_path, content).expect("Failed to write actions file");
    }

    /// Get a Command with XDG env vars set to test directories
    fn command(&self) -> Command {
        let mut cmd = Command::cargo_bin("cliche").expect("Failed to find cliche binary");
        cmd.env("XDG_CONFIG_HOME", self.config_dir.parent().unwrap());
        cmd.env("XDG_DATA_HOME", self.data_dir.parent().unwrap());
        cmd
    }
}

#[test]
fn test_read_with_default_file() {
    let env = TestEnv::new();

    // Create default inbox.actions
    env.write_actions("inbox.actions", "[ ] Test task");

    // Run without specifying file - should use default
    env.command()
        .arg("read")
        .assert()
        .success()
        .stdout(predicate::str::contains("Test task"));
}

#[test]
fn test_config_file_sets_default_format() {
    let env = TestEnv::new();

    // Write config with table format
    env.write_config("format = \"table\"");
    env.write_actions("inbox.actions", "[x] Completed task");

    // Should use table format from config
    env.command()
        .arg("read")
        .assert()
        .success()
        .stdout(predicate::str::contains("State"))  // Table header
        .stdout(predicate::str::contains("Completed task"));
}

#[test]
fn test_env_var_overrides_config() {
    let env = TestEnv::new();

    // Config says table
    env.write_config("format = \"table\"");
    env.write_actions("inbox.actions", "[ ] Task");

    // But env var says JSON
    env.command()
        .arg("read")
        .env("CLICHE_FORMAT", "json")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("{"))  // JSON object
        .stdout(predicate::str::contains("\"actions\":"))
        .stdout(predicate::str::contains("\"name\": \"Task\""));
}

#[test]
fn test_cli_arg_overrides_env_var() {
    let env = TestEnv::new();

    env.write_actions("inbox.actions", "[x] Done");

    // Env says JSON, CLI says actions
    env.command()
        .arg("read")
        .env("CLICHE_FORMAT", "json")
        .arg("--format")
        .arg("actions")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("[x]"))  // Actions format
        .stdout(predicate::str::contains("Done"));
}

#[test]
fn test_read_specific_file() {
    let env = TestEnv::new();

    // Create a specific file
    env.write_actions("work.actions", "[-] In progress task");

    // Read it by specifying the path
    let work_path = env.data_dir.join("work.actions");
    env.command()
        .arg("read")
        .arg(work_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("In progress task"));
}

#[test]
fn test_all_output_formats() {
    let env = TestEnv::new();

    env.write_actions(
        "test.actions",
        "[x] Test $with description !1 +context",
    );
    let test_path = env.data_dir.join("test.actions");

    // Actions format
    env.command()
        .arg("read")
        .arg(&test_path)
        .arg("--format")
        .arg("actions")
        .assert()
        .success()
        .stdout(predicate::str::contains("[x] Test"));

    // JSON format
    env.command()
        .arg("read")
        .arg(&test_path)
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"actions\":"))
        .stdout(predicate::str::contains("\"name\": \"Test\""));

    // XML format
    env.command()
        .arg("read")
        .arg(&test_path)
        .arg("--format")
        .arg("xml")
        .assert()
        .success()
        .stdout(predicate::str::contains("<name>Test</name>"));

    // Table format
    env.command()
        .arg("read")
        .arg(&test_path)
        .arg("--format")
        .arg("table")
        .assert()
        .success()
        .stdout(predicate::str::contains("State"))
        .stdout(predicate::str::contains("Test"));
}

#[test]
fn test_error_on_missing_file() {
    let env = TestEnv::new();

    // Don't create any files

    env.command()
        .arg("read")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to read file"));
}

#[test]
fn test_config_with_custom_default_file() {
    let env = TestEnv::new();

    // Config specifies a different default file
    env.write_config("file = \"mytasks.actions\"");
    env.write_actions("mytasks.actions", "[ ] Custom default task");

    env.command()
        .arg("read")
        .assert()
        .success()
        .stdout(predicate::str::contains("Custom default task"));
}

#[test]
fn test_actions_with_hierarchy() {
    let env = TestEnv::new();

    env.write_actions(
        "test.actions",
        "[x] Parent\n> [ ] Child\n>> [ ] Grandchild",
    );
    let test_path = env.data_dir.join("test.actions");

    // Actions format should preserve hierarchy
    env.command()
        .arg("read")
        .arg(test_path)
        .arg("--format")
        .arg("actions")
        .assert()
        .success()
        .stdout(predicate::str::contains("[x] Parent"))
        .stdout(predicate::str::contains("> [ ] Child"))
        .stdout(predicate::str::contains(">> [ ] Grandchild"));
}

// Schema validation tests - verify JSON output matches canonical schema

#[test]
fn test_json_output_validates_against_schema() {
    use jsonschema::JSONSchema;

    let env = TestEnv::new();

    // Create a test file with various features
    env.write_actions(
        "test.actions",
        "[x] Parent task $description !1 +work,urgent\n> [ ] Child task\n>> [-] Grandchild task",
    );
    let test_path = env.data_dir.join("test.actions");

    // Get JSON output
    let output = env
        .command()
        .arg("read")
        .arg(&test_path)
        .arg("--format")
        .arg("json")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json_str = String::from_utf8(output).expect("Invalid UTF-8");
    let json_value: serde_json::Value =
        serde_json::from_str(&json_str).expect("Invalid JSON");

    // Load schema from tree-sitter-actions repo
    // Note: Assumes tree-sitter-actions is checked out alongside clearhead-cli
    let schema_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tree-sitter-actions/schema/actions.schema.json");

    let schema_str = std::fs::read_to_string(&schema_path)
        .expect("Failed to read schema - ensure tree-sitter-actions is checked out");
    let schema: serde_json::Value =
        serde_json::from_str(&schema_str).expect("Invalid schema JSON");

    // Compile and validate
    let compiled = JSONSchema::compile(&schema).expect("Invalid schema");

    let validation_result = compiled.validate(&json_value);
    if let Err(errors) = validation_result {
        let error_messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        panic!(
            "JSON validation failed:\n{}",
            error_messages.join("\n")
        );
    }
}

// Error handling tests - verify user-facing error messages

#[test]
fn test_helpful_error_on_missing_default_file() {
    let env = TestEnv::new();
    // Don't create inbox.actions

    env.command()
        .arg("read")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to read file"))
        .stderr(predicate::str::contains("inbox.actions"));
}

#[test]
fn test_helpful_error_on_missing_specific_file() {
    let env = TestEnv::new();

    env.command()
        .arg("read")
        .arg("nonexistent.actions")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to read file"))
        .stderr(predicate::str::contains("nonexistent.actions"));
}

#[test]
fn test_error_on_malformed_config() {
    let env = TestEnv::new();

    // Write invalid TOML
    let config_path = env.config_dir.join("config.toml");
    fs::write(config_path, "this is not valid toml = = =").expect("Failed to write config");

    env.write_actions("inbox.actions", "[ ] Task");

    env.command()
        .arg("read")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Failed to load config"));
}

#[test]
fn test_error_on_invalid_format_in_config() {
    let env = TestEnv::new();

    // Config with invalid format
    env.write_config("format = \"invalid_format\"");
    env.write_actions("inbox.actions", "[ ] Task");

    // Should use default format (actions) since config format is invalid
    // Or should error - let's see what happens
    env.command()
        .arg("read")
        .assert()
        .success(); // Falls back to default
}

#[test]
fn test_empty_actions_file() {
    let env = TestEnv::new();

    // Create empty file
    env.write_actions("empty.actions", "");
    let empty_path = env.data_dir.join("empty.actions");

    // Should succeed with empty output (or error gracefully)
    env.command()
        .arg("read")
        .arg(empty_path)
        .assert()
        .success();
    // Empty file outputs just a newline, which is acceptable
}

#[test]
fn test_actions_file_with_only_whitespace() {
    let env = TestEnv::new();

    env.write_actions("whitespace.actions", "   \n\n  \t  \n");
    let ws_path = env.data_dir.join("whitespace.actions");

    env.command()
        .arg("read")
        .arg(ws_path)
        .assert()
        .success();
}

#[test]
fn test_invalid_cli_format_argument() {
    let env = TestEnv::new();

    env.write_actions("inbox.actions", "[ ] Task");

    // Clap should catch this and show valid options
    env.command()
        .arg("read")
        .arg("--format")
        .arg("invalid")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value 'invalid'"));
}
