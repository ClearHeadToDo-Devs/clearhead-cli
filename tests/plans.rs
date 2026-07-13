mod common;
use chrono::{Local, TimeZone};
use common::TestEnv;
use predicates::prelude::*;
use std::fs;

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
    ).unwrap();
    env.command()
        .arg("import")
        .arg("plans")
        .arg(&source)
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Imported 2 plan(s) into charter 'bulk-export'",
        ));
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
    ).unwrap();
    env.command()
        .arg("import")
        .arg("plans")
        .arg(&source)
        .arg("--charter")
        .arg("inbox")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Imported 1 plan(s) into charter 'inbox'",
        ));
    assert!(
        env.data_dir
            .join("plans")
            .join("inbox")
            .join("focus@example.com.ics")
            .exists()
    );
}

#[test]
fn test_import_plans_errors_on_existing_uid_without_overwrite() {
    let env = TestEnv::new();
    let existing = env
        .data_dir
        .join("plans")
        .join("inbox")
        .join("focus@example.com.ics");
    fs::create_dir_all(existing.parent().unwrap()).unwrap();
    fs::write(&existing, "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:focus@example.com\r\nSUMMARY:Old Focus\r\nDTSTART:20260427T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n").unwrap();
    let source = env.data_dir.join("collision.ics");
    fs::write(&source, "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:focus@example.com\r\nSUMMARY:New Focus\r\nDTSTART:20260428T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n").unwrap();
    env.command()
        .arg("import")
        .arg("plans")
        .arg(&source)
        .arg("--charter")
        .arg("inbox")
        .assert()
        .failure()
        .stderr(predicate::str::contains("re-run with --overwrite"));
    assert!(
        fs::read_to_string(existing)
            .unwrap()
            .contains("SUMMARY:Old Focus")
    );
}

#[test]
fn test_import_plans_overwrites_existing_uid_with_flag() {
    let env = TestEnv::new();
    let existing = env
        .data_dir
        .join("plans")
        .join("inbox")
        .join("focus@example.com.ics");
    fs::create_dir_all(existing.parent().unwrap()).unwrap();
    fs::write(&existing, "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:focus@example.com\r\nSUMMARY:Old Focus\r\nDTSTART:20260427T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n").unwrap();
    let source = env.data_dir.join("collision.ics");
    fs::write(&source, "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:focus@example.com\r\nSUMMARY:New Focus\r\nDTSTART:20260428T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n").unwrap();
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
    assert!(
        fs::read_to_string(existing)
            .unwrap()
            .contains("SUMMARY:New Focus")
    );
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
    let written = fs::read_dir(&plans_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let content = fs::read_to_string(written).unwrap();
    assert!(content.contains("SUMMARY:High Priority Task"));
    assert!(content.contains("DESCRIPTION:Do it now"));
    assert!(content.contains("RRULE:FREQ=WEEKLY;BYDAY=MO"));
}

#[test]
fn test_add_plan_file_flag_writes_single_todo_file_to_explicit_path() {
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
        .arg("--rrule")
        .arg("FREQ=WEEKLY;BYDAY=TU")
        .arg("--scheduled-at")
        .arg("2026-04-28T10:00:00Z")
        .assert()
        .success();
    let content = fs::read_to_string(&output).unwrap();
    assert!(content.contains("SUMMARY:Focus Block"));
    assert!(content.contains("BEGIN:VTODO"));
}

#[test]
fn test_add_plan_file_flag_rejects_non_ics_path() {
    let env = TestEnv::new();
    env.command()
        .arg("add")
        .arg("plan")
        .arg("Focus Block")
        .arg("--file")
        .arg(env.data_dir.join("plans").join("focus"))
        .arg("--rrule")
        .arg("FREQ=WEEKLY;BYDAY=TU")
        .assert()
        .failure()
        .stderr(predicate::str::contains("must end with '.ics'"));
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
        .stderr(predicate::str::contains("use `complete action`"));
}

#[test]
fn test_archive_plans_no_plans_found() {
    let env = TestEnv::new();
    // With no plan files in the workspace, archive plans succeeds and reports nothing to do.
    env.command()
        .arg("archive")
        .arg("plans")
        .assert()
        .success()
        .stdout(predicate::str::contains("No plan files found"));
}

#[test]
fn test_archive_plans_dry_run() {
    let env = TestEnv::new();
    env.write_plan_ics("inbox", "test-uid-0.ics", &["Scheduled Plan"]);
    env.command()
        .arg("archive")
        .arg("plans")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("Would retire plan"));
}

#[test]
fn test_sync_events_command() {
    let env = TestEnv::new();
    let uuid1 = "019baaec-00b6-7991-be34-94b68212619a";
    let uuid2 = "019baaec-00b6-7991-be34-94b68212619b";
    env.write_actions(
        "inbox.actions",
        &format!("[ ] Task 1 #{}\n[ ] Task 2 #{}", uuid1, uuid2),
    );
    env.command()
        .arg("sync")
        .arg("events")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 events backfilled"));
    env.command().arg("sync").arg("events").assert().success();
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

#[test]
fn test_sync_calendar_creates_action_mirror_and_stamps_sidecar() {
    let env = TestEnv::new();
    let uuid = "019baaec-00b6-7991-be34-94b68212619a";
    env.write_actions(
        "inbox.actions",
        &format!("[ ] Sync me @2026-04-28T10:00 #{}", uuid),
    );

    env.command()
        .arg("sync")
        .arg("calendar")
        .assert()
        .success()
        .stdout(predicate::str::contains("push action → calendar"))
        .stdout(predicate::str::contains(
            "Sync complete. 1 push, 0 pull, 0 converged, 0 conflict.",
        ));

    let ics_path = env
        .data_dir
        .join("plans")
        .join("inbox")
        .join(format!("{}.ics", uuid));
    let ics = fs::read_to_string(&ics_path).unwrap();
    assert!(ics.contains(&format!("UID:{}", uuid)));
    assert!(ics.contains("SUMMARY:Sync me"));

    let sidecar = fs::read_to_string(env.data_dir.join("charters").join(".inbox.json")).unwrap();
    assert!(sidecar.contains("scheduled_at_sync"));
}

#[test]
fn test_sync_calendar_pulls_calendar_edit_into_action_file() {
    let env = TestEnv::new();
    let uuid = "019baaec-00b6-7991-be34-94b68212619a";
    env.write_actions(
        "inbox.actions",
        &format!("[ ] Pull me @2026-04-28T10:00 #{}", uuid),
    );

    let base = Local
        .with_ymd_and_hms(2026, 4, 28, 10, 0, 0)
        .unwrap()
        .to_rfc3339();
    env.write_text(
        "charters/.inbox.json",
        &format!(
            "{{\n  \"acts\": {{\n    \"{}\": {{\n      \"scheduled_at_sync\": \"{}\"\n    }}\n  }}\n}}",
            uuid, base
        ),
    );
    env.write_text(
        &format!("plans/inbox/{}.ics", uuid),
        &format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:{}\r\nSUMMARY:Pull me\r\nDTSTART:20260429T100000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            uuid
        ),
    );

    env.command()
        .arg("sync")
        .arg("calendar")
        .assert()
        .success()
        .stdout(predicate::str::contains("pull calendar → action"))
        .stdout(predicate::str::contains(
            "Sync complete. 0 push, 1 pull, 0 converged, 0 conflict.",
        ));

    let actions = fs::read_to_string(env.data_dir.join("charters").join("inbox.actions")).unwrap();
    assert!(actions.contains("@2026-04-29T10:00"));

    let sidecar = fs::read_to_string(env.data_dir.join("charters").join(".inbox.json")).unwrap();
    assert!(sidecar.contains("scheduled_at_sync"));
    assert!(sidecar.contains("2026-04-29T10:00:00"));
}

#[test]
fn test_sync_calendar_conflict_can_be_resolved_toward_action() {
    let env = TestEnv::new();
    let uuid = "019baaec-00b6-7991-be34-94b68212619a";
    env.write_actions(
        "inbox.actions",
        &format!("[ ] Clash @2026-04-29T10:00 #{}", uuid),
    );

    let base = Local
        .with_ymd_and_hms(2026, 4, 28, 10, 0, 0)
        .unwrap()
        .to_rfc3339();
    env.write_text(
        "charters/.inbox.json",
        &format!(
            "{{\n  \"acts\": {{\n    \"{}\": {{\n      \"scheduled_at_sync\": \"{}\"\n    }}\n  }}\n}}",
            uuid, base
        ),
    );
    env.write_text(
        &format!("plans/inbox/{}.ics", uuid),
        &format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:{}\r\nSUMMARY:Clash\r\nDTSTART:20260430T100000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            uuid
        ),
    );

    env.command()
        .arg("sync")
        .arg("calendar")
        .arg("--conflict")
        .arg("action")
        .assert()
        .success()
        .stdout(predicate::str::contains("push action → calendar"))
        .stdout(predicate::str::contains(
            "Sync complete. 1 push, 0 pull, 0 converged, 0 conflict.",
        ));

    let ics_path = env
        .data_dir
        .join("plans")
        .join("inbox")
        .join(format!("{}.ics", uuid));
    let plans = clearhead_core::workspace::calendar::ics::parse_ics_file(&ics_path).unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].plan.external_id.as_deref(), Some(uuid));
    let dt = plans[0].plan.dtstart.unwrap();
    assert_eq!(dt.format("%Y-%m-%dT%H:%M").to_string(), "2026-04-29T10:00");
}

#[test]
fn test_sync_calendar_conflict_can_be_resolved_toward_calendar() {
    let env = TestEnv::new();
    let uuid = "019baaec-00b6-7991-be34-94b68212619a";
    env.write_actions(
        "inbox.actions",
        &format!("[ ] Clash @2026-04-29T10:00 #{}", uuid),
    );

    let base = Local
        .with_ymd_and_hms(2026, 4, 28, 10, 0, 0)
        .unwrap()
        .to_rfc3339();
    env.write_text(
        "charters/.inbox.json",
        &format!(
            "{{\n  \"acts\": {{\n    \"{}\": {{\n      \"scheduled_at_sync\": \"{}\"\n    }}\n  }}\n}}",
            uuid, base
        ),
    );
    env.write_text(
        &format!("plans/inbox/{}.ics", uuid),
        &format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Test//Test//EN\r\nBEGIN:VEVENT\r\nUID:{}\r\nSUMMARY:Clash\r\nDTSTART:20260430T100000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
            uuid
        ),
    );

    env.command()
        .arg("sync")
        .arg("calendar")
        .arg("--conflict")
        .arg("calendar")
        .assert()
        .success()
        .stdout(predicate::str::contains("pull calendar → action"))
        .stdout(predicate::str::contains(
            "Sync complete. 0 push, 1 pull, 0 converged, 0 conflict.",
        ));

    let actions = fs::read_to_string(env.data_dir.join("charters").join("inbox.actions")).unwrap();
    assert!(actions.contains("@2026-04-30T10:00"));
}

#[test]
fn test_read_plans_rejects_where_flag() {
    let env = TestEnv::new();
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--where")
        .arg("?s a cco:Plan")
        .assert()
        .failure();
}

#[test]
fn test_read_plans_charter_filter() {
    let env = TestEnv::new();
    env.write_actions("build_clearhead.actions", "");
    env.write_plan_ics("build_clearhead", "top-level-plan.ics", &["Top level plan"]);
    env.write_plan_ics("build_clearhead-subcharter", "sub-plan.ics", &["Sub plan"]);
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
    env.command()
        .arg("read")
        .arg("plans")
        .arg("--recursive")
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
