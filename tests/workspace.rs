mod common;
use common::TestEnv;
use predicates::prelude::*;
use std::fs;

#[test]
fn test_workspace_read_succeeds_when_empty() {
    let env = TestEnv::new();
    env.command().arg("read").arg("plans").assert().success();
}

#[test]
fn test_helpful_error_on_missing_specific_file() {
    let env = TestEnv::new();
    env.command()
        .arg("read").arg("plans")
        .arg("--file").arg("nonexistent.ics")
        .assert().failure();
}

#[test]
fn test_error_on_malformed_config() {
    let env = TestEnv::new();
    let config_path = env.config_dir.join("config.json");
    fs::write(config_path, "{this is not valid json}").expect("Failed to write config");
    env.write_actions("inbox.actions", "[ ] Task");
    env.command()
        .arg("read").arg("plans")
        .assert().failure()
        .stderr(predicate::str::contains("Failed to load config"));
}

#[test]
fn test_error_on_invalid_format_in_config() {
    let env = TestEnv::new();
    env.write_config(r#"{"cli_format": "invalid_format"}"#);
    env.write_actions("inbox.actions", "[ ] Task");
    // Falls back to default format — still succeeds
    env.command().arg("read").arg("plans").assert().success();
}

#[test]
fn test_invalid_cli_format_argument() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[ ] Task");
    env.command()
        .arg("read").arg("plans")
        .arg("--format").arg("invalid")
        .assert().failure()
        .stderr(predicate::str::contains("invalid value 'invalid'"));
}

#[test]
#[ignore = "--where SQL syntax is being removed; use --sparql with ontology-native SPARQL instead"]
fn test_read_workspace_with_sql_filter() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[ ] Low priority !3");
    env.write_actions("work.actions", "[ ] High priority !1");
    env.command()
        .arg("read").arg("plans")
        .arg("--where").arg("priority = 1")
        .assert().success()
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
    env.command()
        .arg("read").arg("plans")
        .arg("--where").arg("project = 'myproject'")
        .assert().success()
        .stdout(predicate::str::contains("Project task"))
        .stdout(predicate::str::contains("Inbox task").not());
}

#[test]
#[ignore = "file_path is a storage-layer detail, not a domain concept; does not belong in SPARQL queries against the ontology"]
fn test_read_workspace_filter_by_file_path() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[ ] Inbox task");
    env.write_actions("work.actions", "[ ] Work task");
    env.command()
        .arg("read").arg("plans")
        .arg("--where").arg("file_path LIKE '%work%'")
        .assert().success()
        .stdout(predicate::str::contains("Work task"))
        .stdout(predicate::str::contains("Inbox task").not());
}

#[test]
fn test_query_sparql_and_where_conflict() {
    let env = TestEnv::new();
    env.command()
        .arg("query").arg("run")
        .arg("SELECT ?s WHERE { ?s a <urn:x> }")
        .arg("--where").arg("?s a <urn:x>")
        .assert().failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn test_read_empty_workspace() {
    let env = TestEnv::new();
    env.command().arg("read").arg("plans").assert().success();
}
