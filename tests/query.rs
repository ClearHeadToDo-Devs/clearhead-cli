mod common;
use common::TestEnv;

// A past-dated action — always <= END_OF_TODAY regardless of when tests run.
const DATED_ACTION: &str =
    "[ ] past scheduled action @2000-01-01T00:00 #01900000-0000-7000-8000-000000000001\n";

// An undated action — should never appear in agenda results.
const UNDATED_ACTION: &str =
    "[ ] undated action #01900000-0000-7000-8000-000000000002\n";

fn parse_json(output: &[u8]) -> Vec<serde_json::Value> {
    serde_json::from_slice(output).expect("output is not valid JSON")
}

#[test]
fn agenda_returns_empty_when_no_dated_actions() {
    let env = TestEnv::new();
    env.with_workspace_identity().write_actions("next.actions", UNDATED_ACTION);

    let output = env
        .command()
        .args(["query", "index", "agenda"])
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let rows = parse_json(&output.stdout);
    assert!(rows.is_empty(), "expected no rows, got {rows:?}");
}

#[test]
fn agenda_returns_past_dated_action() {
    let env = TestEnv::new();
    env.with_workspace_identity().write_actions("next.actions", DATED_ACTION);

    let output = env
        .command()
        .args(["query", "index", "agenda"])
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let rows = parse_json(&output.stdout);
    assert_eq!(rows.len(), 1, "expected 1 row, got {rows:?}");
    assert_eq!(rows[0]["name"], "past scheduled action");
    assert_eq!(rows[0]["status"], "NotStarted");
}

#[test]
fn agenda_excludes_undated_actions() {
    let env = TestEnv::new();
    // Both actions present — only the dated one should appear.
    let content = format!("{DATED_ACTION}{UNDATED_ACTION}");
    env.with_workspace_identity().write_actions("next.actions", &content);

    let output = env
        .command()
        .args(["query", "index", "agenda"])
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let rows = parse_json(&output.stdout);
    assert_eq!(rows.len(), 1, "expected only dated action, got {rows:?}");
    assert_eq!(rows[0]["name"], "past scheduled action");
}

#[test]
fn agenda_row_satisfies_index_contract() {
    let env = TestEnv::new();
    env.with_workspace_identity().write_actions("next.actions", DATED_ACTION);

    let output = env
        .command()
        .args(["query", "index", "agenda"])
        .output()
        .expect("failed to run");

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let rows = parse_json(&output.stdout);
    assert_eq!(rows.len(), 1);

    let row = &rows[0];
    // id is the canonical node IRI — the address mutation verbs target.
    assert_eq!(row["id"], "urn:uuid:01900000-0000-7000-8000-000000000001");
    assert!(row.get("name").is_some(), "missing: name");
    assert!(row.get("status").is_some(), "missing: status");
    assert!(row.get("source_file").is_some(), "missing: source_file");
    assert!(row.get("source_line").is_some(), "missing: source_line");
    assert!(row.get("charter_root").is_some(), "missing: charter_root");
    // Sort keys travel as properties so order survives an RDF round-trip.
    assert!(row.get("scheduled_at").is_some(), "missing sort key: scheduled_at");
}

#[test]
fn agenda_query_listed_under_index_type() {
    let env = TestEnv::new();
    env.command()
        .args(["query", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("agenda"))
        .stdout(predicates::str::contains("index"));
}
