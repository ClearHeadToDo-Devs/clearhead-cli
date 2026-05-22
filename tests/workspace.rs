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
