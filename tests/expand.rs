mod common;
use common::TestEnv;
use predicates::prelude::*;
use std::fs;

#[test]
fn test_expand_acts_writes_primary_and_upcoming_files() {
    let env = TestEnv::new();
    env.write_text(
        "plans/work/work.ics",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:work-standup@example.com\r\nSUMMARY:Weekly Standup\r\nDTSTART:20260518T090000\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );
    env.write_text("charters/work.actions", "");
    env.command()
        .arg("expand")
        .arg("actions")
        .assert()
        .success();
    let primary = fs::read_to_string(env.data_dir.join("charters").join("work.actions")).unwrap();
    let upcoming =
        fs::read_to_string(env.data_dir.join("charters").join("work.upcoming.actions")).unwrap();
    assert!(
        primary.contains("Weekly Standup"),
        "primary should have one instance"
    );
    assert!(
        upcoming.contains("Weekly Standup"),
        "upcoming should have one instance"
    );
    assert_ne!(
        primary, upcoming,
        "primary and upcoming should be different occurrences"
    );
}

#[test]
fn test_expand_acts_idempotent_across_runs() {
    let env = TestEnv::new();
    env.write_text(
        "plans/work/work.ics",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:work-standup-idem@example.com\r\nSUMMARY:Weekly Standup\r\nDTSTART:20260518T090000\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );
    env.write_text("charters/work.actions", "");
    env.command()
        .arg("expand")
        .arg("actions")
        .assert()
        .success();
    env.command()
        .arg("expand")
        .arg("actions")
        .assert()
        .success();
    let primary = fs::read_to_string(env.data_dir.join("charters").join("work.actions")).unwrap();
    let upcoming =
        fs::read_to_string(env.data_dir.join("charters").join("work.upcoming.actions")).unwrap();
    assert_eq!(
        primary.matches("Weekly Standup").count(),
        1,
        "primary must not duplicate"
    );
    assert_eq!(
        upcoming.matches("Weekly Standup").count(),
        1,
        "upcoming must not duplicate"
    );
}

#[test]
fn test_expand_acts_parse_error_keeps_actions_file_unchanged_and_fails() {
    let env = TestEnv::new();
    env.write_text(
        "plans/focus/focus.ics",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:focus-1@example.com\r\nSUMMARY:Focus block\r\nDTSTART:20260601T100000\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );
    let malformed = "not valid actions syntax !!!\n[ ] existing stable action\n";
    env.write_text("charters/focus.actions", malformed);
    env.command()
        .arg("expand")
        .arg("actions")
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
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:bad-1@example.com\r\nSUMMARY:Bad schedule\r\nDTSTART:20260601T090000\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );
    let bad_content = "not valid actions syntax !!!\n[ ] preserve me\n";
    env.write_text("charters/bad.actions", bad_content);
    env.write_text(
        "plans/good/good.ics",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:good-1@example.com\r\nSUMMARY:Good schedule\r\nDTSTART:20260601T110000\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );
    env.write_text("charters/good.actions", "[ ] already here\n");
    env.command()
        .arg("expand")
        .arg("actions")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "expand actions failed for 1 charter file",
        ));
    let bad_after = fs::read_to_string(env.data_dir.join("charters").join("bad.actions")).unwrap();
    assert_eq!(bad_after, bad_content, "bad file must remain unchanged");
    let good_after =
        fs::read_to_string(env.data_dir.join("charters").join("good.actions")).unwrap();
    assert!(good_after.contains("already here"));
    assert!(good_after.contains("Good schedule"));
}

#[test]
fn test_expand_acts_applies_global_template_to_recurring_event() {
    let env = TestEnv::new();

    // ICS with a recurring plan that references a global template.
    // SUMMARY is "Weekly Review" — this should NOT appear; template content replaces it.
    env.write_text(
        "plans/review/review.ics",
        "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:weekly-review-tpl@example.com\r\nSUMMARY:Weekly Review\r\nDTSTART:20260518T100000\r\nRRULE:FREQ=WEEKLY\r\nDESCRIPTION:template: weekly-review\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n",
    );

    // Template has its own named root — structurally replaces the VEVENT flat action.
    env.write_text(
        "templates/weekly-review.actions",
        "[ ] Review Root\n\t[ ] Get Clear\n\t\t[ ] Collect loose papers\n\t[ ] Get Current\n",
    );

    env.write_text("charters/review.actions", "");

    env.command()
        .arg("expand")
        .arg("actions")
        .assert()
        .success();

    let primary = fs::read_to_string(env.data_dir.join("charters").join("review.actions")).unwrap();

    // Template content must be present
    assert!(
        primary.contains("Review Root"),
        "template root must be present"
    );
    assert!(
        primary.contains("Get Clear"),
        "template child must be present"
    );
    assert!(
        primary.contains("Collect loose papers"),
        "nested template child must be present"
    );

    // The VEVENT SUMMARY must NOT appear as a separate wrapper action
    assert!(
        !primary.contains("Weekly Review"),
        "VEVENT flat wrapper must not be written when template is applied"
    );
}
