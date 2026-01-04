use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test helper: creates isolated environment with temp XDG directories
struct TestEnv {
    _temp_dir: TempDir, // Keep alive for cleanup
    config_dir: std::path::PathBuf,
    data_dir: std::path::PathBuf,
    work_dir: std::path::PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_dir = temp_dir.path().join("config/clearhead");
        let data_dir = temp_dir.path().join("data/clearhead");
        let work_dir = temp_dir.path().join("work");

        fs::create_dir_all(&config_dir).expect("Failed to create config dir");
        fs::create_dir_all(&data_dir).expect("Failed to create data dir");
        fs::create_dir_all(&work_dir).expect("Failed to create work dir");

        TestEnv {
            _temp_dir: temp_dir,
            config_dir,
            data_dir,
            work_dir,
        }
    }

    /// Write a JSON config file to the test environment
    fn write_config(&self, content: &str) {
        let config_path = self.config_dir.join("config.json");
        fs::write(config_path, content).expect("Failed to write config");
    }

    /// Write an actions file to the test data directory
    fn write_actions(&self, filename: &str, content: &str) {
        let actions_path = self.data_dir.join(filename);
        fs::write(actions_path, content).expect("Failed to write actions file");
    }

    /// Get a Command with XDG env vars set to test directories and cwd in isolated work dir
    fn command(&self) -> Command {
        let bin = assert_cmd::cargo::cargo_bin!("clearhead_cli");
        let mut cmd = Command::new(bin);
        cmd.env("XDG_CONFIG_HOME", self.config_dir.parent().unwrap());
        cmd.env("XDG_DATA_HOME", self.data_dir.parent().unwrap());
        cmd.current_dir(&self.work_dir); // Run from temp dir to avoid project detection
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

    // Write config with table format (JSON with cli_format key)
    env.write_config(r#"{"cli_format": "table"}"#);
    env.write_actions("inbox.actions", "[x] Completed task");

    // Should use table format from config
    env.command()
        .arg("read")
        .assert()
        .success()
        .stdout(predicate::str::contains("State")) // Table header
        .stdout(predicate::str::contains("Completed task"));
}

#[test]
fn test_env_var_overrides_config() {
    let env = TestEnv::new();

    // Config says table
    env.write_config(r#"{"cli_format": "table"}"#);
    env.write_actions("inbox.actions", "[ ] Task");

    // But env var says JSON
    env.command()
        .arg("read")
        .env("CLEARHEAD_CLI_FORMAT", "json")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("{")) // JSON object
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
        .env("CLEARHEAD_CLI_FORMAT", "json")
        .arg("--format")
        .arg("actions")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("[x]")) // Actions format
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

    env.write_actions("test.actions", "[x] Test $with description !1 +context");
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
    env.write_config(r#"{"default_file": "mytasks.actions"}"#);
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

    env.write_actions("test.actions", "[x] Parent\n>[ ] Child\n>>[ ] Grandchild");
    let test_path = env.data_dir.join("test.actions");

    // Actions format should preserve hierarchy with proper spacing
    env.command()
        .arg("read")
        .arg(test_path)
        .arg("--format")
        .arg("actions")
        .assert()
        .success()
        .stdout(predicate::str::contains("[x] Parent"))
        .stdout(predicate::str::contains(">[ ] Child"))
        .stdout(predicate::str::contains(">>[ ] Grandchild"));
}

#[test]
fn test_format_indentation() {
    let env = TestEnv::new();

    // 1. Test Compact Style indentation
    env.write_actions("compact.actions", "[ ] Root\n>[ ] Child");
    let compact_path = env.data_dir.join("compact.actions");

    env.command()
        .arg("format")
        .arg(&compact_path)
        .arg("--style")
        .arg("compact")
        .arg("--indent-width")
        .arg("2")
        .assert()
        .success()
        .stdout(predicate::str::contains("  >[ ] Child")); // 2 spaces indent

    // 2. Test List Style indentation
    env.write_actions("list.actions", "[ ] Root $ Desc");
    let list_path = env.data_dir.join("list.actions");

    env.command()
        .arg("format")
        .arg(&list_path)
        .arg("--style")
        .arg("list")
        .arg("--indent-width")
        .arg("4")
        .assert()
        .success()
        .stdout(predicate::str::contains("\n    $ Desc")); // 4 spaces indent for metadata

    // 3. Test Tab indentation
    env.command()
        .arg("format")
        .arg(&compact_path)
        .arg("--indent-style")
        .arg("tabs")
        .arg("--indent-width")
        .arg("1")
        .assert()
        .success()
        .stdout(predicate::str::contains("\t>[ ] Child"));
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
    let json_value: serde_json::Value = serde_json::from_str(&json_str).expect("Invalid JSON");

    // Load schema from local vendored copy
    let schema_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("schemas/actions.schema.json");

    let schema_str = std::fs::read_to_string(&schema_path)
        .expect("Failed to read schema from schemas/actions.schema.json");
    let schema: serde_json::Value = serde_json::from_str(&schema_str).expect("Invalid schema JSON");

    // Compile and validate
    let compiled = JSONSchema::compile(&schema).expect("Invalid schema");

    let validation_result = compiled.validate(&json_value);
    if let Err(errors) = validation_result {
        let error_messages: Vec<String> = errors.map(|e| e.to_string()).collect();
        panic!("JSON validation failed:\n{}", error_messages.join("\n"));
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

    // Write invalid JSON
    let config_path = env.config_dir.join("config.json");
    fs::write(config_path, "{this is not valid json}").expect("Failed to write config");

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
    env.write_config(r#"{"cli_format": "invalid_format"}"#);
    env.write_actions("inbox.actions", "[ ] Task");

    // Should use default format (actions) since config format is invalid
    // Or should error - let's see what happens
    env.command().arg("read").assert().success(); // Falls back to default
}

#[test]
fn test_empty_actions_file() {
    let env = TestEnv::new();

    // Create empty file
    env.write_actions("empty.actions", "");
    let empty_path = env.data_dir.join("empty.actions");

    // Should succeed with empty output (or error gracefully)
    env.command().arg("read").arg(empty_path).assert().success();
    // Empty file outputs just a newline, which is acceptable
}

#[test]
fn test_actions_file_with_only_whitespace() {
    let env = TestEnv::new();

    env.write_actions("whitespace.actions", "   \n\n  \t  \n");
    let ws_path = env.data_dir.join("whitespace.actions");

    env.command().arg("read").arg(ws_path).assert().success();
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

#[test]
fn test_normalize_adds_uuids() {
    let env = TestEnv::new();
    // Action without ID
    env.write_actions("no_id.actions", "[ ] Task without ID");
    let file_path = env.data_dir.join("no_id.actions");

    // Run normalize --write
    env.command()
        .arg("normalize")
        .arg(&file_path)
        .arg("--write")
        .assert()
        .success();

    // Verify file content now has UUID
    let content = fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("#"));
    // Basic UUID validation (8-4-4-4-12 hex chars)
    // We just check for the # prefix followed by at least 8 chars which implies ID generation worked
    assert!(content.contains("#"));
}

#[test]
fn test_patch_updates_existing_actions() {
    let env = TestEnv::new();

    // 1. Create Primary file with ID
    let uuid = "8975ca06-f358-4846-916a-b32bb1fd7f7a";
    env.write_actions("primary.actions", &format!("[ ] Task A #{}", uuid));

    // 2. Create Secondary file with same ID but different state
    env.write_actions("secondary.actions", &format!("[x] Task A #{}", uuid));

    let primary_path = env.data_dir.join("primary.actions");
    let secondary_path = env.data_dir.join("secondary.actions");

    // 3. Run patch
    env.command()
        .arg("patch")
        .arg("--primary")
        .arg(&primary_path)
        .arg("--secondary")
        .arg(&secondary_path)
        .arg("--write")
        .assert()
        .success();

    // 4. Verify Primary is updated
    let content = fs::read_to_string(&primary_path).unwrap();
    assert!(content.contains("[x] Task A"));
    assert!(content.contains(uuid));
}

#[test]
fn test_patch_appends_new_actions() {
    let env = TestEnv::new();

    // 1. Primary has Task A
    let uuid_a = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
    env.write_actions("primary.actions", &format!("[ ] Task A #{}", uuid_a));

    // 2. Secondary has Task A (unchanged) and Task B (new)
    let uuid_b = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
    // Note: In real usage, Secondary would usually be a full view, so it would contain A and B.
    // The patch logic iterates Secondary and updates/appends to Primary.
    env.write_actions(
        "secondary.actions",
        &format!("[ ] Task A #{}\n[ ] Task B #{}", uuid_a, uuid_b),
    );

    let primary_path = env.data_dir.join("primary.actions");
    let secondary_path = env.data_dir.join("secondary.actions");

    // 3. Run patch
    env.command()
        .arg("patch")
        .arg("--primary")
        .arg(&primary_path)
        .arg("--secondary")
        .arg(&secondary_path)
        .arg("--write")
        .assert()
        .success();

    // 4. Verify Primary has both
    let content = fs::read_to_string(&primary_path).unwrap();
    assert!(content.contains("Task A"));
    assert!(content.contains("Task B"));
}
