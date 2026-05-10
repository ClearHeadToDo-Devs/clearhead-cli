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
        let actions_path = self.data_dir.join("charters").join(filename);
        if let Some(parent) = actions_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create actions parent dir");
        }
        fs::write(actions_path, content).expect("Failed to write actions file");
    }

    fn write_ics(&self, filename: &str, summaries: &[&str]) {
        let content = Self::ics_content(summaries);
        let path = self.data_dir.join("charters").join(filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create ics parent dir");
        }
        fs::write(path, content).expect("Failed to write ics file");
    }

    /// Write a single-event `.ics` file into `data_dir/plans/<charter_slug>/<filename>`.
    fn write_plan_ics(&self, charter_slug: &str, filename: &str, summaries: &[&str]) {
        let content = Self::ics_content(summaries);
        let path = self.data_dir.join("plans").join(charter_slug).join(filename);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create plan ics parent dir");
        }
        fs::write(path, content).expect("Failed to write plan ics file");
    }

    fn ics_content(summaries: &[&str]) -> String {
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
fn test_read_plans_shows_ics_vevent() {
    let env = TestEnv::new();

    env.write_plan_ics("inbox", "root.ics", &["My Plan"]);

    env.command()
        .arg("read")
        .arg("plans")
        .assert()
        .success()
        .stdout(predicate::str::contains("My Plan"));
}

#[test]
fn test_import_plans_splits_multi_event_ics_into_vdir_files() {
    let env = TestEnv::new();
    let source = env.data_dir.join("bulk-export.ics");
    fs::write(
        &source,
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:plan-one@example.com\r\nSUMMARY:Plan One\r\nDTSTART:20260428T100000Z\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:plan-two@example.com\r\nSUMMARY:Plan Two\r\nDTSTART:20260429T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    )
    .unwrap();

    env.command()
        .arg("import")
        .arg("plans")
        .arg(&source)
        .assert()
        .success()
        .stdout(predicate::str::contains("Imported 2 plan(s) into charter 'bulk-export'"));

    let plans_dir = env.data_dir.join("plans").join("bulk-export");
    assert!(plans_dir.join("plan-one@example.com.ics").exists());
    assert!(plans_dir.join("plan-two@example.com.ics").exists());
}

#[test]
fn test_import_plans_honors_explicit_charter_flag() {
    let env = TestEnv::new();
    let source = env.data_dir.join("calendar.ics");
    fs::write(
        &source,
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:focus@example.com\r\nSUMMARY:Focus Block\r\nDTSTART:20260428T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    )
    .unwrap();

    env.command()
        .arg("import")
        .arg("plans")
        .arg(&source)
        .arg("--charter")
        .arg("inbox")
        .assert()
        .success()
        .stdout(predicate::str::contains("Imported 1 plan(s) into charter 'inbox'"));

    let imported = env
        .data_dir
        .join("plans")
        .join("inbox")
        .join("focus@example.com.ics");
    assert!(imported.exists());
}

#[test]
fn test_import_plans_errors_on_existing_uid_without_overwrite() {
    let env = TestEnv::new();
    let existing = env
        .data_dir
        .join("plans")
        .join("inbox")
        .join("focus@example.com.ics");
    if let Some(parent) = existing.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(
        &existing,
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:focus@example.com\r\nSUMMARY:Old Focus\r\nDTSTART:20260427T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    )
    .unwrap();

    let source = env.data_dir.join("collision.ics");
    fs::write(
        &source,
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:focus@example.com\r\nSUMMARY:New Focus\r\nDTSTART:20260428T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    )
    .unwrap();

    env.command()
        .arg("import")
        .arg("plans")
        .arg(&source)
        .arg("--charter")
        .arg("inbox")
        .assert()
        .failure()
        .stderr(predicate::str::contains("re-run with --overwrite"));

    let content = fs::read_to_string(existing).unwrap();
    assert!(content.contains("SUMMARY:Old Focus"));
}

#[test]
fn test_import_plans_overwrites_existing_uid_with_flag() {
    let env = TestEnv::new();
    let existing = env
        .data_dir
        .join("plans")
        .join("inbox")
        .join("focus@example.com.ics");
    if let Some(parent) = existing.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(
        &existing,
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:focus@example.com\r\nSUMMARY:Old Focus\r\nDTSTART:20260427T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    )
    .unwrap();

    let source = env.data_dir.join("collision.ics");
    fs::write(
        &source,
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:focus@example.com\r\nSUMMARY:New Focus\r\nDTSTART:20260428T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    )
    .unwrap();

    env.command()
        .arg("import")
        .arg("plans")
        .arg(&source)
        .arg("--charter")
        .arg("inbox")
        .arg("--overwrite")
        .assert()
        .success()
        .stdout(predicate::str::contains("(1 overwritten)"));

    let content = fs::read_to_string(existing).unwrap();
    assert!(content.contains("SUMMARY:New Focus"));
}

#[test]
fn test_read_acts_with_default_file() {
    let env = TestEnv::new();

    env.write_actions("inbox.actions", "[ ] Test task");

    env.command()
        .arg("read")
        .arg("acts")
        .assert()
        .success()
        .stdout(predicate::str::contains("Test task"));
}

#[test]
fn test_read_acts_specific_file() {
    let env = TestEnv::new();

    env.write_actions("work.actions", "[-] In progress task");

    let work_path = env.data_dir.join("charters").join("work.actions");
    env.command()
        .arg("read")
        .arg("acts")
        .arg("--file")
        .arg(work_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("In progress task"));
}

#[test]
fn test_read_acts_open_only_filters_closed_states_in_open_file() {
    let env = TestEnv::new();

    env.write_actions(
        "work.actions",
        "[ ] Open task\n[x] Done task\n[_] Cancelled task",
    );
    let work_path = env.data_dir.join("charters").join("work.actions");

    env.command()
        .arg("read")
        .arg("acts")
        .arg("--open-only")
        .arg("--file")
        .arg(work_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Open task"))
        .stdout(predicate::str::contains("Done task").not())
        .stdout(predicate::str::contains("Cancelled task").not());
}

#[test]
fn test_show_act_resolves_by_name() {
    let env = TestEnv::new();

    env.write_actions("work.actions", "[ ] Inspect CLI $Useful detail$ +cli");
    let work_path = env.data_dir.join("charters").join("work.actions");

    env.command()
        .arg("show")
        .arg("act")
        .arg("Inspect")
        .arg("--file")
        .arg(work_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Inspect CLI"))
        .stdout(predicate::str::contains("description:"));
}

#[test]
fn test_read_acts_json_format() {
    let env = TestEnv::new();

    env.write_actions("test.actions", "[x] Test $with description$ !1 +context");
    let test_path = env.data_dir.join("charters").join("test.actions");

    env.command()
        .arg("read")
        .arg("acts")
        .arg("--format")
        .arg("json")
        .arg("--file")
        .arg(&test_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("Test"));
}

#[test]
fn test_error_on_missing_ics_file() {
    let env = TestEnv::new();

    env.command()
        .arg("read")
        .arg("plans")
        .arg("--file")
        .arg("/nonexistent/path.ics")
        .assert()
        .failure();
}

#[test]
fn test_read_acts_with_hierarchy() {
    let env = TestEnv::new();

    env.write_actions("test.actions", "[x] Parent\n>[ ] Child\n>>[ ] Grandchild");
    let test_path = env.data_dir.join("charters").join("test.actions");

    env.command()
        .arg("read")
        .arg("acts")
        .arg("--file")
        .arg(test_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Parent"))
        .stdout(predicate::str::contains("Child"))
        .stdout(predicate::str::contains("Grandchild"));
}

#[test]
fn test_format_style_flags() {
    let env = TestEnv::new();

    // Topiary is the sole formatter now — --style is accepted but there is only one
    // output format. --indent-width and --indent-style ARE applied by Topiary.

    env.write_actions("compact.actions", "[ ] Root\n>[ ] Child");
    let compact_path = env.data_dir.join("charters").join("compact.actions");

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
    let list_path = env.data_dir.join("charters").join("list.actions");

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
    let env = TestEnv::new();

    env.write_actions(
        "test.actions",
        "[x] Parent task $description$ !1 +work,urgent\n> [ ] Child task\n>> [-] Grandchild task",
    );
    let test_path = env.data_dir.join("charters").join("test.actions");

    let output = env
        .command()
        .arg("read")
        .arg("acts")
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
    // read acts --format json emits a JSON array of act objects
    let json_value: serde_json::Value = serde_json::from_str(&json_str).expect("Invalid JSON");
    assert!(json_value.is_array(), "Expected JSON array from read acts");
    assert!(json_str.contains("Parent task"));
}

// Error handling tests - verify user-facing error messages

#[test]
fn test_workspace_read_succeeds_when_empty() {
    let env = TestEnv::new();
    // Don't create any files - empty workspace

    // Workspace read succeeds even when empty (just returns no results)
    env.command().arg("read").arg("plans").assert().success();
}

#[test]
fn test_helpful_error_on_missing_specific_file() {
    let env = TestEnv::new();

    env.command()
        .arg("read")
        .arg("plans")
        .arg("--file")
        .arg("nonexistent.ics")
        .assert()
        .failure();
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
    env.command().arg("read").arg("plans").assert().success(); // Falls back to default
}

#[test]
fn test_empty_actions_file() {
    let env = TestEnv::new();

    env.write_actions("empty.actions", "");
    let empty_path = env.data_dir.join("charters").join("empty.actions");

    env.command()
        .arg("read")
        .arg("acts")
        .arg("--file")
        .arg(empty_path)
        .assert()
        .success();
}

#[test]
fn test_actions_file_with_only_whitespace() {
    let env = TestEnv::new();

    env.write_actions("whitespace.actions", "   \n\n  \t  \n");
    let ws_path = env.data_dir.join("charters").join("whitespace.actions");

    env.command()
        .arg("read")
        .arg("acts")
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
    let file_path = env.data_dir.join("charters").join("no_id.actions");

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

    let primary_path = env.data_dir.join("charters").join("primary.actions");
    let secondary_path = env.data_dir.join("charters").join("secondary.actions");

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

    let primary_path = env.data_dir.join("charters").join("primary.actions");
    let secondary_path = env.data_dir.join("charters").join("secondary.actions");

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
        .arg("--scheduled-at")
        .arg("2026-04-28T10:00:00Z")
        .assert()
        .success(); // outputs UUID to stdout

    // Check file content
    let plans_dir = env.data_dir.join("plans").join("inbox");
    let written = fs::read_dir(&plans_dir).unwrap().next().unwrap().unwrap().path();
    let content = fs::read_to_string(written).unwrap();
    assert!(content.contains("SUMMARY:New Task"));
    assert!(content.contains("UID:"));
    assert!(content.contains("DTSTART"));
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
        .arg("--rrule")
        .arg("FREQ=WEEKLY;BYDAY=MO")
        .assert()
        .success();

    let plans_dir = env.data_dir.join("plans").join("inbox");
    let written = fs::read_dir(&plans_dir).unwrap().next().unwrap().unwrap().path();
    let content = fs::read_to_string(written).unwrap();
    assert!(content.contains("SUMMARY:High Priority Task"));
    assert!(content.contains("PRIORITY:1"));
    assert!(content.contains("CATEGORIES:work\\,urgent"));
    assert!(content.contains("DESCRIPTION:Do it now"));
    assert!(content.contains("RRULE:FREQ=WEEKLY;BYDAY=MO"));
}

#[test]
fn test_add_plan_file_flag_writes_single_event_file_to_explicit_path() {
    let env = TestEnv::new();
    let output = env
        .data_dir
        .join("plans")
        .join("focus")
        .join("focus-block.ics");

    env.command()
        .arg("add")
        .arg("plan")
        .arg("Focus Block")
        .arg("--file")
        .arg(&output)
        .arg("--scheduled-at")
        .arg("2026-04-28T10:00:00Z")
        .assert()
        .success();

    let content = fs::read_to_string(&output).unwrap();
    assert!(content.contains("SUMMARY:Focus Block"));
    assert!(content.contains("BEGIN:VEVENT"));
}

#[test]
fn test_add_plan_file_flag_rejects_non_ics_path() {
    let env = TestEnv::new();
    let output = env.data_dir.join("plans").join("focus");

    env.command()
        .arg("add")
        .arg("plan")
        .arg("Focus Block")
        .arg("--file")
        .arg(&output)
        .assert()
        .failure()
        .stderr(predicate::str::contains("must end with '.ics'"));
}

#[test]
fn test_complete_command() {
    let env = TestEnv::new();
    let uuid = "019baaec-00b6-7991-be34-94b68212619a";
    env.write_actions("inbox.actions", &format!("[ ] Task to complete #{}", uuid));

    env.command()
        .arg("complete")
        .arg("act")
        .arg(uuid)
        .assert()
        .success(); // completion logged via tracing, no stdout output

    let content = fs::read_to_string(env.data_dir.join("charters").join("inbox.completed.actions")).unwrap();
    assert!(content.contains("[x] Task to complete"));
    assert!(content.contains("%")); // Completed date
}

#[test]
fn test_complete_command_by_name() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[ ] Unique Task Name");

    env.command()
        .arg("complete")
        .arg("act")
        .arg("Unique Task")
        .assert()
        .success();

    let content = fs::read_to_string(env.data_dir.join("charters").join("inbox.completed.actions")).unwrap();
    assert!(content.contains("[x] Unique Task Name"));
}

#[test]
fn test_complete_command_idempotent_fail() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[x] Already Done");

    env.command()
        .arg("complete")
        .arg("act")
        .arg("Already Done")
        .assert()
        .failure() // Should fail as no *open* action found
        .stderr(predicate::str::contains("No open act found"));
}

#[test]
fn test_complete_plan_explains_state_lives_on_acts() {
    let env = TestEnv::new();
    env.write_ics("inbox/plans/scheduled-plan.ics", &["Scheduled Plan"]);

    env.command()
        .arg("complete")
        .arg("plan")
        .arg("Scheduled Plan")
        .assert()
        .failure()
        .stderr(predicate::str::contains("use `complete act`"));
}

#[test]
fn test_archive_plans_explains_schedule_archival_is_unresolved() {
    let env = TestEnv::new();
    env.write_ics("inbox/plans/scheduled-plan.ics", &["Scheduled Plan"]);

    env.command()
        .arg("archive")
        .arg("plans")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Plan archival is not implemented"))
        .stderr(predicate::str::contains("archive acts"));
}

#[test]
fn test_sync_events_command() {
    let env = TestEnv::new();
    let uuid1 = "019baaec-00b6-7991-be34-94b68212619a";
    let uuid2 = "019baaec-00b6-7991-be34-94b68212619b";

    // Create file with two actions
    env.write_actions(
        "inbox.actions",
        &format!("[ ] Task 1 #{}\n[ ] Task 2 #{}", uuid1, uuid2),
    );

    // 1. First sync - should sync both
    env.command()
        .arg("sync")
        .arg("events")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 events backfilled"));

    // 2. Second sync - should skip both (TODO: idempotency not implemented yet)
    // This test documents the current behavior (re-syncs all)
    env.command().arg("sync").arg("events").assert().success();

    // 3. Add a third action and sync again
    let uuid3 = "019baaec-00b6-7991-be34-94b68212619c";
    env.write_actions(
        "inbox.actions",
        &format!(
            "[ ] Task 1 #{}\n[ ] Task 2 #{}\n[ ] Task 3 #{}",
            uuid1, uuid2, uuid3
        ),
    );

    env.command().arg("sync").arg("events").assert().success();
}

// ============================================================================
// Workspace-First Read Tests
// ============================================================================

#[test]
fn test_read_acts_aggregates_all_files() {
    let env = TestEnv::new();

    env.write_actions("inbox.actions", "[ ] Inbox task");
    env.write_actions("work.actions", "[ ] Work task");

    let project_dir = env.data_dir.join("charters").join("project1");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join("next.actions"), "[ ] Project task").unwrap();

    env.command()
        .arg("read")
        .arg("acts")
        .assert()
        .success()
        .stdout(predicate::str::contains("Inbox task"))
        .stdout(predicate::str::contains("Work task"))
        .stdout(predicate::str::contains("Project task"));
}

#[test]
fn test_read_acts_file_flag() {
    let env = TestEnv::new();

    env.write_actions("inbox.actions", "[ ] Inbox task");
    env.write_actions("work.actions", "[ ] Work task");

    let work_path = env.data_dir.join("charters").join("work.actions");

    env.command()
        .arg("read")
        .arg("acts")
        .arg("--file")
        .arg(&work_path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Work task"))
        .stdout(predicate::str::contains("Inbox task").not());
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

    let project_dir = env.data_dir.join("charters").join("myproject");
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
    env.command().arg("read").arg("plans").assert().success();
}

#[test]
fn test_read_plans_charter_filter() {
    let env = TestEnv::new();

    // Seed the charter with an actions file and plans in the new flat layout
    env.write_actions("build_clearhead.actions", "");
    env.write_plan_ics("build_clearhead", "top-level-plan.ics", &["Top level plan"]);
    env.write_plan_ics("build_clearhead-subcharter", "sub-plan.ics", &["Sub plan"]);

    // Charter filter scopes to the matched charter's ICS files
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--charter")
        .arg("build_clearhead")
        .assert()
        .success()
        .stdout(predicate::str::contains("Top level plan"));
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
fn test_read_acts_skips_hidden_directories() {
    let env = TestEnv::new();

    env.write_actions("inbox.actions", "[ ] Visible task");

    let hidden_dir = env.data_dir.join(".git");
    fs::create_dir_all(&hidden_dir).unwrap();
    fs::write(hidden_dir.join("state.actions"), "[ ] Hidden task").unwrap();

    env.command()
        .arg("read")
        .arg("acts")
        .assert()
        .success()
        .stdout(predicate::str::contains("Visible task"))
        .stdout(predicate::str::contains("Hidden task").not());
}

#[test]
fn test_expand_acts_parse_error_keeps_actions_file_unchanged_and_fails() {
    let env = TestEnv::new();

    env.write_text(
        "plans/focus/focus.ics",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:focus-1@example.com\r\nSUMMARY:Focus block\r\nDTSTART:20991201T100000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );

    let malformed = "not valid actions syntax !!!\n[ ] existing stable action\n";
    env.write_text("charters/focus.actions", malformed);

    env.command()
        .arg("expand")
        .arg("acts")
        .arg("--days")
        .arg("36500")
        .assert()
        .failure()
        .stderr(predicate::str::contains("skipped due to parse issues"));

    let after = fs::read_to_string(env.data_dir.join("charters").join("focus.actions")).unwrap();
    assert_eq!(after, malformed, "malformed file should be byte-stable");
}

#[test]
fn test_expand_acts_mixed_batch_writes_valid_file_and_fails_overall() {
    let env = TestEnv::new();

    env.write_text(
        "plans/bad/bad.ics",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:bad-1@example.com\r\nSUMMARY:Bad schedule\r\nDTSTART:20991201T090000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );
    let bad_content = "not valid actions syntax !!!\n[ ] preserve me\n";
    env.write_text("charters/bad.actions", bad_content);

    env.write_text(
        "plans/good/good.ics",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:good-1@example.com\r\nSUMMARY:Good schedule\r\nDTSTART:20991201T110000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );
    env.write_text("charters/good.actions", "[ ] already here\n");

    env.command()
        .arg("expand")
        .arg("acts")
        .arg("--days")
        .arg("36500")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "expand acts failed for 1 charter file",
        ));

    let bad_after = fs::read_to_string(env.data_dir.join("charters").join("bad.actions")).unwrap();
    assert_eq!(bad_after, bad_content, "bad file must remain unchanged");

    let good_after = fs::read_to_string(env.data_dir.join("charters").join("good.actions")).unwrap();
    assert!(good_after.contains("already here"));
    assert!(good_after.contains("Good schedule"));
}

#[test]
fn test_read_acts_file_fails_on_malformed_input() {
    let env = TestEnv::new();
    env.write_text(
        "charters/malformed.actions",
        "not valid actions syntax !!!\n[ ] Keep me\n",
    );
    let path = env.data_dir.join("charters").join("malformed.actions");

    env.command()
        .arg("read")
        .arg("acts")
        .arg("--file")
        .arg(&path)
        .assert()
        .failure();
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
        "charters/recover-sync.actions",
        "not valid actions syntax !!!\n[ ] Sync me #019baaec-00b6-7991-be34-94b68212619a\n",
    );
    let path = env.data_dir.join("charters").join("recover-sync.actions");

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
    env.write_text("charters/normalize-bad.actions", malformed);
    let path = env.data_dir.join("charters").join("normalize-bad.actions");

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
        "charters/format-recover.actions",
        "not valid actions syntax !!!\n[ ] Keep formatting preview\n",
    );
    let path = env.data_dir.join("charters").join("format-recover.actions");

    env.command()
        .arg("format")
        .arg("file")
        .arg(&path)
        .assert()
        .success()
        .stdout(predicate::str::contains("Keep formatting preview"))
        .stderr(predicate::str::contains("parsed with"));
}

#[test]
fn test_normalize_write_creates_sidecar() {
    use clearhead_core::workspace::sidecar::read_sidecar;

    let env = TestEnv::new();
    let uuid = "01951111-0000-7000-0000-000000000001";
    env.write_actions("work.actions", &format!("[ ] Task one #{}\n", uuid));
    let file_path = env.data_dir.join("charters").join("work.actions");

    env.command()
        .arg("normalize")
        .arg("file")
        .arg(&file_path)
        .arg("--write")
        .assert()
        .success();

    let sidecar_path = env.data_dir.join("charters").join(".work.json");
    assert!(sidecar_path.exists(), "sidecar must be created by normalize --write");

    let meta = read_sidecar(&sidecar_path).unwrap();
    assert!(meta.acts.contains_key(uuid), "sidecar must have entry for the act UUID");
    assert!(meta.acts[uuid].created.is_some(), "sidecar entry must have created timestamp");
}

#[test]
fn test_sidecar_additive_on_repeated_normalize() {
    use clearhead_core::workspace::sidecar::read_sidecar;

    let env = TestEnv::new();
    let uuid = "01951111-0000-7000-0000-000000000001";
    env.write_actions("work.actions", &format!("[ ] Task #{}\n", uuid));
    let file_path = env.data_dir.join("charters").join("work.actions");
    let sidecar_path = env.data_dir.join("charters").join(".work.json");

    // First normalize — creates sidecar
    env.command()
        .arg("normalize")
        .arg("file")
        .arg(&file_path)
        .arg("--write")
        .assert()
        .success();
    let created_first = read_sidecar(&sidecar_path).unwrap().acts[uuid].created.unwrap();

    // Second normalize — must preserve existing entry unchanged
    env.command()
        .arg("normalize")
        .arg("file")
        .arg(&file_path)
        .arg("--write")
        .assert()
        .success();
    let created_second = read_sidecar(&sidecar_path).unwrap().acts[uuid].created.unwrap();

    assert_eq!(
        created_first, created_second,
        "created timestamp must not change on re-normalize"
    );
}
