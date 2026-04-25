use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test helper: creates isolated environment
struct TestEnv {
    _temp_dir: TempDir,
    work_dir: std::path::PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let work_dir = temp_dir.path().join("work");
        fs::create_dir_all(&work_dir).expect("Failed to create work dir");

        TestEnv {
            _temp_dir: temp_dir,
            work_dir,
        }
    }

    fn write_actions(&self, filename: &str, content: &str) -> std::path::PathBuf {
        let actions_path = self.work_dir.join(filename);
        fs::write(&actions_path, content).expect("Failed to write actions file");
        actions_path
    }

    fn command(&self) -> Command {
        let bin = assert_cmd::cargo::cargo_bin!("clearhead_cli");
        let mut cmd = Command::new(bin);
        cmd.current_dir(&self.work_dir);
        cmd
    }
}

#[test]
fn test_export_basic_action_with_date() {
    let env = TestEnv::new();
    let actions = r#"[ ] Meeting @2026-01-20T14:00 D30"#;
    let file = env.write_actions("test.actions", actions);

    env.command()
        .arg("export")
        .arg("plans")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("BEGIN:VCALENDAR"))
        .stdout(predicate::str::contains("BEGIN:VEVENT"))
        .stdout(predicate::str::contains("SUMMARY:Meeting"))
        .stdout(predicate::str::contains("DTSTART:"))
        .stdout(predicate::str::contains("DTEND:"))
        .stdout(predicate::str::contains("END:VEVENT"))
        .stdout(predicate::str::contains("END:VCALENDAR"));
}

#[test]
fn test_export_with_description_and_priority() {
    let env = TestEnv::new();
    let actions = r#"[ ] Important meeting
    $ Discuss project roadmap $
    !1
    @2026-01-20T14:00"#;
    let file = env.write_actions("test.actions", actions);

    env.command()
        .arg("export")
        .arg("plans")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("SUMMARY:Important meeting"))
        .stdout(predicate::str::contains(
            "DESCRIPTION:Discuss project roadmap",
        ))
        .stdout(predicate::str::contains("PRIORITY:1"));
}

#[test]
fn test_export_with_categories() {
    let env = TestEnv::new();
    let actions = r#"[ ] Team sync @2026-01-20T14:00 +Work,Meeting,Team"#;
    let file = env.write_actions("test.actions", actions);

    env.command()
        .arg("export")
        .arg("plans")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::contains("CATEGORIES:Work\\,Meeting\\,Team"));
}

#[test]
fn test_export_status_mapping() {
    let env = TestEnv::new();
    let actions = r#"
[ ] Not started @2026-01-20T10:00
[-] In progress @2026-01-20T11:00
[=] Blocked @2026-01-20T12:00
[x] Completed @2026-01-20T13:00 %2026-01-20T14:00
[_] Cancelled @2026-01-20T15:00
"#;
    let file = env.write_actions("status.actions", actions);

    let output = env
        .command()
        .arg("export")
        .arg("plans")
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8_lossy(&output);

    assert!(output_str.contains("SUMMARY:Not started"));
    assert!(output_str.contains("SUMMARY:In progress"));
    assert!(output_str.contains("SUMMARY:Blocked"));
    assert!(output_str.contains("SUMMARY:Completed"));
    assert!(output_str.contains("SUMMARY:Cancelled"));

    let event_count = output_str.matches("BEGIN:VEVENT").count();
    assert_eq!(event_count, 5);
}

#[test]
fn test_export_open_only_filter() {
    let env = TestEnv::new();
    let actions = r#"
[ ] Open task @2026-01-20T10:00
[-] In progress task @2026-01-20T11:00
[=] Blocked task @2026-01-20T12:00
[x] Completed task @2026-01-20T13:00 %2026-01-20T14:00
[_] Cancelled task @2026-01-20T15:00
"#;
    let file = env.write_actions("filter.actions", actions);

    let output = env
        .command()
        .arg("export")
        .arg("plans")
        .arg(&file)
        .arg("--open-only")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8_lossy(&output);

    assert!(output_str.contains("SUMMARY:Open task"));
    assert!(output_str.contains("SUMMARY:In progress task"));
    assert!(output_str.contains("SUMMARY:Blocked task"));

    assert!(!output_str.contains("SUMMARY:Completed task"));
    assert!(!output_str.contains("SUMMARY:Cancelled task"));

    let event_count = output_str.matches("BEGIN:VEVENT").count();
    assert_eq!(event_count, 3);
}

#[test]
fn test_export_skips_actions_without_dates() {
    let env = TestEnv::new();
    let actions = r#"
[ ] Task with date @2026-01-20T10:00
[ ] Task without date
[ ] Another task with date @2026-01-20T11:00
"#;
    let file = env.write_actions("nodates.actions", actions);

    let output = env
        .command()
        .arg("export")
        .arg("plans")
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8_lossy(&output);

    assert!(output_str.contains("SUMMARY:Task with date"));
    assert!(output_str.contains("SUMMARY:Another task with date"));
    assert!(!output_str.contains("SUMMARY:Task without date"));

    let event_count = output_str.matches("BEGIN:VEVENT").count();
    assert_eq!(event_count, 2);
}

#[test]
fn test_export_to_file() {
    let env = TestEnv::new();
    let actions = r#"[ ] Meeting @2026-01-20T14:00"#;
    let input_file = env.write_actions("test.actions", actions);
    let output_file = env.work_dir.join("calendar.ics");

    env.command()
        .arg("export")
        .arg("plans")
        .arg(&input_file)
        .arg("-o")
        .arg(&output_file)
        .assert()
        .success();

    let content = fs::read_to_string(&output_file).expect("Failed to read output file");
    assert!(content.contains("BEGIN:VCALENDAR"));
    assert!(content.contains("SUMMARY:Meeting"));
    assert!(content.contains("END:VCALENDAR"));
}

#[test]
fn test_export_from_stdin() {
    let env = TestEnv::new();
    let actions = r#"[ ] Standup @2026-01-20T09:00 D15"#;

    env.command()
        .arg("export")
        .arg("plans")
        .arg("-")
        .write_stdin(actions)
        .assert()
        .success()
        .stdout(predicate::str::contains("BEGIN:VCALENDAR"))
        .stdout(predicate::str::contains("SUMMARY:Standup"));
}

#[test]
fn test_export_default_duration() {
    let env = TestEnv::new();
    let actions = r#"[ ] Meeting without duration @2026-01-20T14:00"#;
    let file = env.write_actions("test.actions", actions);

    let output = env
        .command()
        .arg("export")
        .arg("plans")
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8_lossy(&output);

    // Should have both DTSTART and DTEND (15 minute default)
    assert!(output_str.contains("DTSTART:"));
    assert!(output_str.contains("DTEND:"));
}

#[test]
fn test_export_empty_calendar() {
    let env = TestEnv::new();
    let actions = r#"
[ ] Task without date
[ ] Another task without date
"#;
    let file = env.write_actions("nodates.actions", actions);

    let output = env
        .command()
        .arg("export")
        .arg("plans")
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let output_str = String::from_utf8_lossy(&output);

    assert!(output_str.contains("BEGIN:VCALENDAR"));
    assert!(output_str.contains("END:VCALENDAR"));
    assert_eq!(output_str.matches("BEGIN:VEVENT").count(), 0);
}
