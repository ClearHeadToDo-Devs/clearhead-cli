use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test helper: creates isolated environment with temp XDG directories
struct TestEnv {
    _temp_dir: TempDir, // Keep alive for cleanup
    config_dir: std::path::PathBuf,
    data_dir: std::path::PathBuf,
    state_dir: std::path::PathBuf,
    work_dir: std::path::PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config_dir = temp_dir.path().join("config/clearhead");
        let data_dir = temp_dir.path().join("data/clearhead");
        let state_dir = temp_dir.path().join("state/clearhead");
        let work_dir = temp_dir.path().join("work");

        fs::create_dir_all(&config_dir).expect("Failed to create config dir");
        fs::create_dir_all(&data_dir).expect("Failed to create data dir");
        fs::create_dir_all(&state_dir).expect("Failed to create state dir");
        fs::create_dir_all(&work_dir).expect("Failed to create work dir");

        TestEnv {
            _temp_dir: temp_dir,
            config_dir,
            data_dir,
            state_dir,
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

    fn write_text(&self, relative_path: &str, content: &str) {
        let path = self.data_dir.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create parent dirs");
        }
        fs::write(path, content).expect("Failed to write file");
    }

    /// Get a Command with XDG env vars set to test directories and cwd in isolated work dir
    fn command(&self) -> Command {
        let bin = assert_cmd::cargo::cargo_bin!("clearhead_cli");
        let mut cmd = Command::new(bin);
        cmd.env("XDG_CONFIG_HOME", self.config_dir.parent().unwrap());
        cmd.env("XDG_DATA_HOME", self.data_dir.parent().unwrap());
        cmd.env("XDG_STATE_HOME", &self.state_dir);
        cmd.current_dir(&self.work_dir); // Run from isolated temp directory
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
        .arg("plans")
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
        .arg("plans")
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
        .arg("plans")
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
        .arg("plans")
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

    // Read it by specifying the path with --file flag
    let work_path = env.data_dir.join("work.actions");
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--file")
        .arg(work_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("In progress task"));
}

#[test]
fn test_all_output_formats() {
    let env = TestEnv::new();

    env.write_actions("test.actions", "[x] Test $with description$ !1 +context");
    let test_path = env.data_dir.join("test.actions");

    // Actions format
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--format")
        .arg("actions")
        .arg("--file")
        .arg(&test_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("[x] Test"));

    // JSON format
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--format")
        .arg("json")
        .arg("--file")
        .arg(&test_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"actions\":"))
        .stdout(predicate::str::contains("\"name\": \"Test\""));

    // XML format
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--format")
        .arg("xml")
        .arg("--file")
        .arg(&test_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("<name>Test</name>"));

    // Table format
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--format")
        .arg("table")
        .arg("--file")
        .arg(&test_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("State"))
        .stdout(predicate::str::contains("Test"));
}

#[test]
fn test_error_on_missing_file() {
    let env = TestEnv::new();

    // Reading a non-existent specific file should fail
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--file")
        .arg("/nonexistent/path.actions")
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
        .arg("plans")
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
        .arg("plans")
        .arg("--format")
        .arg("actions")
        .arg("--file")
        .arg(test_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("[x] Parent"))
        .stdout(predicate::str::contains(">[ ] Child"))
        .stdout(predicate::str::contains(">>[ ] Grandchild"));
}

#[test]
fn test_format_style_flags() {
    let env = TestEnv::new();

    // Topiary is the sole formatter now — --style is accepted but there is only one
    // output format. --indent-width and --indent-style ARE applied by Topiary.

    env.write_actions("compact.actions", "[ ] Root\n>[ ] Child");
    let compact_path = env.data_dir.join("compact.actions");

    // --style compact with --indent-width 2: Topiary indents child with 2 spaces
    env.command()
        .arg("format")
        .arg("file")
        .arg(&compact_path)
        .arg("--style")
        .arg("compact")
        .arg("--indent-width")
        .arg("2")
        .assert()
        .success()
        .stdout(predicate::str::contains(">[ ] Child"));

    // --style list is accepted (falls back to Topiary compact); description stays inline
    env.write_actions("list.actions", "[ ] Root $ Desc $");
    let list_path = env.data_dir.join("list.actions");

    env.command()
        .arg("format")
        .arg("file")
        .arg(&list_path)
        .arg("--style")
        .arg("list")
        .arg("--indent-width")
        .arg("4")
        .assert()
        .success()
        .stdout(predicate::str::contains("$ Desc"));

    // --indent-style tabs: Topiary uses tabs
    env.command()
        .arg("format")
        .arg("file")
        .arg(&compact_path)
        .arg("--indent-style")
        .arg("tabs")
        .arg("--indent-width")
        .arg("1")
        .assert()
        .success()
        .stdout(predicate::str::contains(">[ ] Child"));
}

// Schema validation tests - verify JSON output matches canonical schema

#[test]
fn test_json_output_validates_against_schema() {
    use jsonschema::JSONSchema;

    let env = TestEnv::new();

    // Create a test file with various features
    env.write_actions(
        "test.actions",
        "[x] Parent task $description$ !1 +work,urgent\n> [ ] Child task\n>> [-] Grandchild task",
    );
    let test_path = env.data_dir.join("test.actions");

    // Get JSON output
    let output = env
        .command()
        .arg("read")
        .arg("plans")
        .arg("--format")
        .arg("json")
        .arg("--file")
        .arg(&test_path)
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
fn test_workspace_read_succeeds_when_empty() {
    let env = TestEnv::new();
    // Don't create any files - empty workspace

    // Workspace read succeeds even when empty (just returns no results)
    env.command()
        .arg("read")
        .arg("plans")
        .assert()
        .success();
}

#[test]
fn test_helpful_error_on_missing_specific_file() {
    let env = TestEnv::new();

    env.command()
        .arg("read")
        .arg("plans")
        .arg("--file")
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
        .arg("plans")
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
    env.command()
        .arg("read")
        .arg("plans")
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
        .arg("plans")
        .arg("--file")
        .arg(empty_path)
        .assert()
        .success();
}

#[test]
fn test_actions_file_with_only_whitespace() {
    let env = TestEnv::new();

    env.write_actions("whitespace.actions", "   \n\n  \t  \n");
    let ws_path = env.data_dir.join("whitespace.actions");

    env.command()
        .arg("read")
        .arg("plans")
        .arg("--file")
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
        .arg("plans")
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

    // Run normalize file --write
    env.command()
        .arg("normalize")
        .arg("file")
        .arg(&file_path)
        .arg("--write")
        .assert()
        .success();

    // Verify file content now has UUID
    let content = fs::read_to_string(&file_path).unwrap();
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
        .arg("file")
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
    env.write_actions(
        "secondary.actions",
        &format!("[ ] Task A #{}\n[ ] Task B #{}", uuid_a, uuid_b),
    );

    let primary_path = env.data_dir.join("primary.actions");
    let secondary_path = env.data_dir.join("secondary.actions");

    // 3. Run patch
    env.command()
        .arg("patch")
        .arg("file")
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

#[test]
fn test_add_command() {
    let env = TestEnv::new();

    env.command()
        .arg("add")
        .arg("plan")
        .arg("New Task")
        .assert()
        .success(); // outputs UUID to stdout

    // Check file content
    let content = fs::read_to_string(env.data_dir.join("inbox.actions")).unwrap();
    assert!(content.contains("[ ] New Task"));
    assert!(content.contains("#")); // UUID should be there
}

#[test]
fn test_add_command_with_options() {
    let env = TestEnv::new();

    env.command()
        .arg("add")
        .arg("plan")
        .arg("High Priority Task")
        .arg("--priority")
        .arg("1")
        .arg("--context")
        .arg("work")
        .arg("--context")
        .arg("urgent")
        .arg("--description")
        .arg("Do it now")
        .assert()
        .success();

    let content = fs::read_to_string(env.data_dir.join("inbox.actions")).unwrap();
    assert!(content.contains("[ ] High Priority Task"));
    assert!(content.contains("!1"));
    assert!(content.contains("+work,urgent"));
    assert!(content.contains("$ Do it now"));
}

#[test]
fn test_complete_command() {
    let env = TestEnv::new();
    let uuid = "019baaec-00b6-7991-be34-94b68212619a";
    env.write_actions("inbox.actions", &format!("[ ] Task to complete #{}", uuid));

    env.command()
        .arg("complete")
        .arg("plan")
        .arg(uuid)
        .assert()
        .success(); // completion logged via tracing, no stdout output

    let content = fs::read_to_string(env.data_dir.join("inbox.actions")).unwrap();
    assert!(content.contains("[x] Task to complete"));
    assert!(content.contains("%")); // Completed date
}

#[test]
fn test_complete_command_by_name() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[ ] Unique Task Name");

    env.command()
        .arg("complete")
        .arg("plan")
        .arg("Unique Task")
        .assert()
        .success();

    let content = fs::read_to_string(env.data_dir.join("inbox.actions")).unwrap();
    assert!(content.contains("[x] Unique Task Name"));
}

#[test]
fn test_complete_command_idempotent_fail() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[x] Already Done");

    env.command()
        .arg("complete")
        .arg("plan")
        .arg("Already Done")
        .assert()
        .failure() // Should fail as no *open* action found
        .stderr(predicate::str::contains("No open action found"));
}

#[test]
fn test_sync_events_command() {
    let env = TestEnv::new();
    let uuid1 = "019baaec-00b6-7991-be34-94b68212619a";
    let uuid2 = "019baaec-00b6-7991-be34-94b68212619b";

    // Create file with two actions
    env.write_actions("inbox.actions", &format!("[ ] Task 1 #{}\n[ ] Task 2 #{}", uuid1, uuid2));

    // 1. First sync - should sync both
    env.command()
        .arg("sync")
        .arg("events")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 events backfilled"));

    // 2. Second sync - should skip both (TODO: idempotency not implemented yet)
    // This test documents the current behavior (re-syncs all)
    env.command()
        .arg("sync")
        .arg("events")
        .assert()
        .success();

    // 3. Add a third action and sync again
    let uuid3 = "019baaec-00b6-7991-be34-94b68212619c";
    env.write_actions("inbox.actions", &format!("[ ] Task 1 #{}\n[ ] Task 2 #{}\n[ ] Task 3 #{}", uuid1, uuid2, uuid3));

    env.command()
        .arg("sync")
        .arg("events")
        .assert()
        .success();
}

// ============================================================================
// Workspace-First Read Tests
// ============================================================================

#[test]
fn test_read_workspace_aggregates_all_files() {
    let env = TestEnv::new();

    // Create multiple action files
    env.write_actions("inbox.actions", "[ ] Inbox task");
    env.write_actions("work.actions", "[ ] Work task");

    // Create a project subdirectory
    let project_dir = env.data_dir.join("project1");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join("next.actions"), "[ ] Project task").unwrap();

    // Read workspace (no flags) should find all tasks
    env.command()
        .arg("read")
        .arg("plans")
        .assert()
        .success()
        .stdout(predicate::str::contains("Inbox task"))
        .stdout(predicate::str::contains("Work task"))
        .stdout(predicate::str::contains("Project task"));
}

#[test]
fn test_read_file_flag() {
    let env = TestEnv::new();

    env.write_actions("inbox.actions", "[ ] Inbox task");
    env.write_actions("work.actions", "[ ] Work task");

    let work_path = env.data_dir.join("work.actions");

    // Read specific file should only show that file's tasks
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--file")
        .arg(&work_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Work task"))
        .stdout(predicate::str::contains("Inbox task").not());
}

#[test]
fn test_read_stdio_flag() {
    let env = TestEnv::new();

    // Read from stdin
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--stdio")
        .write_stdin("[ ] Stdin task !1\n[ ] Another stdin task")
        .assert()
        .success()
        .stdout(predicate::str::contains("Stdin task"))
        .stdout(predicate::str::contains("Another stdin task"));
}

#[test]
#[ignore = "--where SQL syntax is being removed; use --sparql with ontology-native SPARQL instead"]
fn test_read_workspace_with_sql_filter() {
    let env = TestEnv::new();

    env.write_actions("inbox.actions", "[ ] Low priority !3");
    env.write_actions("work.actions", "[ ] High priority !1");

    // Filter by priority across workspace
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--where")
        .arg("priority = 1")
        .assert()
        .success()
        .stdout(predicate::str::contains("High priority"))
        .stdout(predicate::str::contains("Low priority").not());
}

#[test]
#[ignore = "--where SQL syntax is being removed; project filtering belongs in SPARQL via actions:hasObjective"]
fn test_read_workspace_filter_by_project() {
    let env = TestEnv::new();

    env.write_actions("inbox.actions", "[ ] Inbox task");

    let project_dir = env.data_dir.join("myproject");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join("next.actions"), "[ ] Project task").unwrap();

    // Filter by inferred project name
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--where")
        .arg("project = 'myproject'")
        .assert()
        .success()
        .stdout(predicate::str::contains("Project task"))
        .stdout(predicate::str::contains("Inbox task").not());
}

#[test]
#[ignore = "file_path is a storage-layer detail, not a domain concept; does not belong in SPARQL queries against the ontology"]
fn test_read_workspace_filter_by_file_path() {
    let env = TestEnv::new();

    env.write_actions("inbox.actions", "[ ] Inbox task");
    env.write_actions("work.actions", "[ ] Work task");

    // Filter by file path pattern
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--where")
        .arg("file_path LIKE '%work%'")
        .assert()
        .success()
        .stdout(predicate::str::contains("Work task"))
        .stdout(predicate::str::contains("Inbox task").not());
}

#[test]
fn test_query_sparql_and_where_conflict() {
    let env = TestEnv::new();

    // positional SPARQL query and --where should conflict on `query run`
    env.command()
        .arg("query")
        .arg("run")
        .arg("SELECT ?s WHERE { ?s a <urn:x> }")
        .arg("--where")
        .arg("?s a <urn:x>")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn test_read_plans_rejects_where_flag() {
    let env = TestEnv::new();

    // --where is no longer a valid flag on `read plans` — belongs on `query`
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--where")
        .arg("?s a cco:Plan")
        .assert()
        .failure();
}

#[test]
fn test_read_empty_workspace() {
    let env = TestEnv::new();

    // Empty workspace should succeed (may output just a newline)
    env.command()
        .arg("read")
        .arg("plans")
        .assert()
        .success();
}

// Helper: write an actions file, creating parent dirs as needed
fn write_nested_actions(env: &TestEnv, relative_path: &str, content: &str) {
    let full_path = env.data_dir.join(relative_path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).expect("Failed to create nested dir");
    }
    fs::write(&full_path, content).expect("Failed to write nested actions file");
}

#[test]
fn test_charter_filter_strict_excludes_subcharters() {
    let env = TestEnv::new();

    // Top-level charter
    write_nested_actions(&env, "build_clearhead/next.actions", "[ ] Top level plan");
    // Sub-charter
    write_nested_actions(&env, "build_clearhead/subcharter.actions", "[ ] Sub plan");

    // Strict filter: only top-level plans
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--charter")
        .arg("build_clearhead")
        .arg("--format")
        .arg("table")
        .assert()
        .success()
        .stdout(predicate::str::contains("Top level plan"))
        .stdout(predicate::str::contains("Sub plan").not());
}

#[test]
fn test_charter_filter_recursive_includes_subcharters() {
    let env = TestEnv::new();

    write_nested_actions(&env, "build_clearhead/next.actions", "[ ] Top level plan");
    write_nested_actions(&env, "build_clearhead/subcharter.actions", "[ ] Sub plan");

    // Recursive filter: both plans
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--charter")
        .arg("build_clearhead")
        .arg("--recursive")
        .arg("--format")
        .arg("table")
        .assert()
        .success()
        .stdout(predicate::str::contains("Top level plan"))
        .stdout(predicate::str::contains("Sub plan"));
}

#[test]
fn test_recursive_requires_charter() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[ ] Task");

    // --recursive without --charter should fail
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--recursive")
        .assert()
        .failure();
}

#[test]
fn test_read_skips_hidden_directories() {
    let env = TestEnv::new();

    env.write_actions("inbox.actions", "[ ] Visible task");

    // Create a non-.clearhead hidden directory with actions (should be skipped)
    let hidden_dir = env.data_dir.join(".git");
    fs::create_dir_all(&hidden_dir).unwrap();
    fs::write(hidden_dir.join("state.actions"), "[ ] Hidden task").unwrap();

    env.command()
        .arg("read")
        .arg("plans")
        .assert()
        .success()
        .stdout(predicate::str::contains("Visible task"))
        .stdout(predicate::str::contains("Hidden task").not());
}

#[test]
fn test_expand_acts_parse_error_keeps_actions_file_unchanged_and_fails() {
    let env = TestEnv::new();

    env.write_text(
        "focus.ics",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:focus-1@example.com\r\nSUMMARY:Focus block\r\nDTSTART:20991201T100000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );

    let malformed = "not valid actions syntax !!!\n[ ] existing stable action\n";
    env.write_text("focus.actions", malformed);

    env.command()
        .arg("expand")
        .arg("acts")
        .arg("--days")
        .arg("36500")
        .assert()
        .failure()
        .stderr(predicate::str::contains("skipped due to parse issues"));

    let after = fs::read_to_string(env.data_dir.join("focus.actions")).unwrap();
    assert_eq!(after, malformed, "malformed file should be byte-stable");
}

#[test]
fn test_expand_acts_mixed_batch_writes_valid_file_and_fails_overall() {
    let env = TestEnv::new();

    env.write_text(
        "bad.ics",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:bad-1@example.com\r\nSUMMARY:Bad schedule\r\nDTSTART:20991201T090000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );
    let bad_content = "not valid actions syntax !!!\n[ ] preserve me\n";
    env.write_text("bad.actions", bad_content);

    env.write_text(
        "good.ics",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:good-1@example.com\r\nSUMMARY:Good schedule\r\nDTSTART:20991201T110000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );
    env.write_text("good.actions", "[ ] already here\n");

    env.command()
        .arg("expand")
        .arg("acts")
        .arg("--days")
        .arg("36500")
        .assert()
        .failure()
        .stderr(predicate::str::contains("expand acts failed for 1 charter file"));

    let bad_after = fs::read_to_string(env.data_dir.join("bad.actions")).unwrap();
    assert_eq!(bad_after, bad_content, "bad file must remain unchanged");

    let good_after = fs::read_to_string(env.data_dir.join("good.actions")).unwrap();
    assert!(good_after.contains("already here"));
    assert!(good_after.contains("Good schedule"));
}

#[test]
fn test_read_plans_file_recover_mode_warns_and_succeeds() {
    let env = TestEnv::new();
    let malformed = "not valid actions syntax !!!\n[ ] Keep me\n";
    env.write_text("recover-read.actions", malformed);
    let path = env.data_dir.join("recover-read.actions");

    env.command()
        .arg("read")
        .arg("plans")
        .arg("--file")
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Keep me"))
        .stderr(predicate::str::contains("parsed with"));
}

#[test]
fn test_export_plans_stdin_recover_mode_warns_and_succeeds() {
    let env = TestEnv::new();

    env.command()
        .arg("export")
        .arg("plans")
        .arg("-")
        .write_stdin("not valid actions syntax !!!\n[ ] Keep export\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("BEGIN:VCALENDAR"))
        .stderr(predicate::str::contains("parsed with"));
}

#[test]
fn test_sync_events_file_recover_mode_warns_and_succeeds() {
    let env = TestEnv::new();
    env.write_text(
        "recover-sync.actions",
        "not valid actions syntax !!!\n[ ] Sync me #019baaec-00b6-7991-be34-94b68212619a\n",
    );
    let path = env.data_dir.join("recover-sync.actions");

    env.command()
        .arg("sync")
        .arg("events")
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("1 events backfilled"))
        .stderr(predicate::str::contains("parsed with"));
}

#[test]
fn test_normalize_file_write_parse_error_keeps_file_unchanged_and_fails() {
    let env = TestEnv::new();
    let malformed = "not valid actions syntax !!!\n[ ] Keep normalize file\n";
    env.write_text("normalize-bad.actions", malformed);
    let path = env.data_dir.join("normalize-bad.actions");

    env.command()
        .arg("normalize")
        .arg("file")
        .arg(&path)
        .arg("--write")
        .assert()
        .failure()
        .stderr(predicate::str::contains("file not modified"));

    let after = fs::read_to_string(&path).unwrap();
    assert_eq!(after, malformed, "malformed file should remain byte-stable");
}

#[test]
fn test_format_file_read_recover_mode_warns_and_succeeds() {
    let env = TestEnv::new();
    env.write_text(
        "format-recover.actions",
        "not valid actions syntax !!!\n[ ] Keep formatting preview\n",
    );
    let path = env.data_dir.join("format-recover.actions");

    env.command()
        .arg("format")
        .arg("file")
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Keep formatting preview"))
        .stderr(predicate::str::contains("parsed with"));
}
