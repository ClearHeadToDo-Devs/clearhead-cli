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
fn project_pwd_overrides_configured_data_dir() {
    let env = TestEnv::new();
    // Personal workspace with its own action, explicitly pinned as data_dir in
    // the global config — the setting that used to defeat project detection.
    env.write_actions(
        "inbox.actions",
        "[ ] personal task #019e0000-0000-7000-0000-000000000001\n",
    );
    env.write_config(&format!(r#"{{"data_dir":"{}"}}"#, env.data_dir.display()));

    // A project rooted at the pwd. Most-local context must win (workspace.md).
    let project_charters = env.work_dir.join(".clearhead").join("charters");
    fs::create_dir_all(&project_charters).expect("create project charters");
    fs::write(
        project_charters.join("next.actions"),
        "[ ] project task #019e0000-0000-7000-0000-000000000002\n",
    )
    .expect("write project actions");

    env.command()
        .args(["read", "actions", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("project task"))
        .stdout(predicate::str::contains("personal task").not());
}

#[test]
fn default_to_user_scope_bypasses_project_detection() {
    let env = TestEnv::new();
    // The sanctioned opt-out: inside a project, the flag pins the invocation
    // to the user workspace (specifications/configuration.md).
    env.write_actions(
        "inbox.actions",
        "[ ] personal task #019e0000-0000-7000-0000-000000000001\n",
    );
    env.write_config(r#"{"default_to_user_scope": true}"#);

    let project_charters = env.work_dir.join(".clearhead").join("charters");
    fs::create_dir_all(&project_charters).expect("create project charters");
    fs::write(
        project_charters.join("next.actions"),
        "[ ] project task #019e0000-0000-7000-0000-000000000002\n",
    )
    .expect("write project actions");

    env.command()
        .args(["read", "actions", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("personal task"))
        .stdout(predicate::str::contains("project task").not());
}

#[test]
fn configured_data_dir_used_outside_any_project() {
    let env = TestEnv::new();
    // No .clearhead anywhere up from work_dir — the configured data_dir is the
    // fallback personal workspace and must still be honored.
    env.write_actions(
        "inbox.actions",
        "[ ] personal task #019e0000-0000-7000-0000-000000000001\n",
    );
    env.write_config(&format!(r#"{{"data_dir":"{}"}}"#, env.data_dir.display()));

    env.command()
        .args(["read", "actions", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("personal task"));
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

#[test]
fn action_verbs_resolve_across_additional_workspaces() {
    let env = TestEnv::new();

    // Project at pwd whose config links a sibling workspace (workspace.md).
    let project_ch = env.work_dir.join(".clearhead");
    fs::create_dir_all(project_ch.join("charters")).expect("create project charters");
    fs::write(
        project_ch.join("config.json"),
        r#"{"additional_workspaces":["../../other"]}"#,
    )
    .expect("write project config");

    // The target lives in a non-default actions file of the sibling workspace.
    let other_root = env._temp_dir.path().join("other");
    let other_ch = other_root.join(".clearhead/charters");
    fs::create_dir_all(&other_ch).expect("create sibling charters");
    fs::write(
        other_root.join(".clearhead/workspace.json"),
        r#"{"workspace_name":"sibling-space"}"#,
    )
    .expect("write sibling manifest");
    fs::write(
        other_ch.join("lsp.actions"),
        "[ ] sibling task #019e0000-0000-7000-0000-000000000003\n",
    )
    .expect("write sibling actions");

    // Bare uuid-prefix resolution must reach the sibling workspace without -f.
    env.command()
        .args(["update", "action", "019e0000", "-p", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("updated"));

    let updated = fs::read_to_string(other_ch.join("lsp.actions")).expect("read back");
    assert!(updated.contains("!1"), "priority written to sibling file: {updated}");
}

#[test]
fn workspace_filter_honors_additional_workspace_config_name() {
    let env = TestEnv::new();

    let project_ch = env.work_dir.join(".clearhead/charters");
    fs::create_dir_all(&project_ch).expect("create project charters");
    fs::write(
        env.work_dir.join(".clearhead/config.json"),
        r#"{"additional_workspaces":["../../other"]}"#,
    )
    .expect("write project config");
    fs::write(
        env.work_dir.join(".clearhead/workspace.json"),
        r#"{"workspace_name":"primary-home"}"#,
    )
    .expect("write project manifest");
    fs::write(
        project_ch.join("next.actions"),
        "[ ] primary task #019e0000-0000-7000-0000-000000000010\n",
    )
    .expect("write primary actions");

    let other_root = env._temp_dir.path().join("other");
    let other_ch = other_root.join(".clearhead/charters");
    fs::create_dir_all(&other_ch).expect("create sibling charters");
    fs::write(
        other_root.join(".clearhead/workspace.json"),
        r#"{"workspace_name":"sibling-space"}"#,
    )
    .expect("write sibling manifest");
    fs::write(
        other_ch.join("lsp.actions"),
        "[ ] sibling task #019e0000-0000-7000-0000-000000000011\n",
    )
    .expect("write sibling actions");

    env.command()
        .args(["--workspace", "sibling-space", "read", "actions", "--format", "table"])
        .assert()
        .success()
        .stdout(predicate::str::contains("sibling task"))
        .stdout(predicate::str::contains("primary task").not());
}
