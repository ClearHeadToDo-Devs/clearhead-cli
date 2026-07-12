mod common;
use common::TestEnv;
use predicates::prelude::*;
use std::fs;

#[test]
fn test_read_acts_with_default_file() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[ ] Test task");
    env.command()
        .arg("read").arg("actions")
        .assert().success()
        .stdout(predicate::str::contains("Test task"));
}

#[test]
fn test_read_acts_specific_file() {
    let env = TestEnv::new();
    env.write_actions("work.actions", "[-] In progress task");
    let work_path = env.data_dir.join("charters").join("work.actions");
    env.command()
        .arg("read").arg("actions").arg("--file").arg(work_path)
        .assert().success()
        .stdout(predicate::str::contains("In progress task"));
}

#[test]
fn test_read_acts_open_only_filters_closed_states_in_open_file() {
    let env = TestEnv::new();
    env.write_actions("work.actions", "[ ] Open task\n[x] Done task\n[_] Cancelled task");
    let work_path = env.data_dir.join("charters").join("work.actions");
    env.command()
        .arg("read").arg("actions").arg("--open-only").arg("--file").arg(work_path)
        .assert().success()
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
        .arg("show").arg("action").arg("Inspect").arg("--file").arg(work_path)
        .assert().success()
        .stdout(predicate::str::contains("Inspect CLI"))
        .stdout(predicate::str::contains("description:"));
}

#[test]
fn test_read_acts_json_format() {
    let env = TestEnv::new();
    env.write_actions("test.actions", "[x] Test $with description$ !1 +context");
    let test_path = env.data_dir.join("charters").join("test.actions");
    env.command()
        .arg("read").arg("actions")
        .arg("--format").arg("json")
        .arg("--file").arg(&test_path)
        .assert().success()
        // `json` is an alias for the json-ld output mode.
        .stdout(predicate::str::contains("\"name\""))
        .stdout(predicate::str::contains("Test"));
}

#[test]
fn test_read_acts_with_hierarchy() {
    let env = TestEnv::new();
    env.write_actions("test.actions", "[x] Parent\n>[ ] Child\n>>[ ] Grandchild");
    let test_path = env.data_dir.join("charters").join("test.actions");
    env.command()
        .arg("read").arg("actions").arg("--file").arg(test_path)
        .assert().success()
        .stdout(predicate::str::contains("Parent"))
        .stdout(predicate::str::contains("Child"))
        .stdout(predicate::str::contains("Grandchild"));
}

#[test]
fn test_format_style_flags() {
    let env = TestEnv::new();
    env.write_actions("compact.actions", "[ ] Root\n>[ ] Child");
    let compact_path = env.data_dir.join("charters").join("compact.actions");
    env.command()
        .arg("format").arg("file").arg(&compact_path)
        .arg("--style").arg("compact").arg("--indent-width").arg("2")
        .assert().success()
        .stdout(predicate::str::contains(">[ ] Child"));
    env.write_actions("list.actions", "[ ] Root $ Desc $");
    let list_path = env.data_dir.join("charters").join("list.actions");
    env.command()
        .arg("format").arg("file").arg(&list_path)
        .arg("--style").arg("list").arg("--indent-width").arg("4")
        .assert().success()
        // Description hugs its $ markers (icon->value compact, like !1/#id); the
        // spaced input `$ Desc $` normalises to `$Desc$`.
        .stdout(predicate::str::contains("$Desc$"));
    env.command()
        .arg("format").arg("file").arg(&compact_path)
        .arg("--indent-style").arg("tabs").arg("--indent-width").arg("1")
        .assert().success()
        .stdout(predicate::str::contains(">[ ] Child"));
}

#[test]
fn test_json_output_validates_against_schema() {
    let env = TestEnv::new();
    env.write_actions(
        "test.actions",
        "[x] Parent task $description$ !1 +work,urgent\n> [ ] Child task\n>> [-] Grandchild task",
    );
    let test_path = env.data_dir.join("charters").join("test.actions");
    let output = env.command()
        .arg("read").arg("actions")
        .arg("--format").arg("json-ld")
        .arg("--file").arg(&test_path)
        .assert().success()
        .get_output().stdout.clone();
    let json_str = String::from_utf8(output).expect("Invalid UTF-8");
    let json_value: serde_json::Value = serde_json::from_str(&json_str).expect("Invalid JSON");
    // read actions emits a JSON-LD document: an object carrying @context and a @graph array of nodes.
    assert!(json_value.is_object(), "Expected a JSON-LD document object from read actions");
    assert!(
        json_value.get("@graph").and_then(|g| g.as_array()).is_some(),
        "Expected a @graph array in the JSON-LD document",
    );
    assert!(json_str.contains("Parent task"));
}

#[test]
fn test_complete_command() {
    let env = TestEnv::new();
    let uuid = "019baaec-00b6-7991-be34-94b68212619a";
    env.write_actions("inbox.actions", &format!("[ ] Task to complete #{}", uuid));
    env.command()
        .arg("complete").arg("action").arg(uuid)
        .assert().success();
    let content = fs::read_to_string(env.data_dir.join("charters").join("inbox.completed.actions")).unwrap();
    assert!(content.contains("[x] Task to complete"));
    assert!(content.contains("%")); // Completed date
}

#[test]
fn test_complete_command_by_name() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[ ] Unique Task Name");
    env.command()
        .arg("complete").arg("action").arg("Unique Task")
        .assert().success();
    let content = fs::read_to_string(env.data_dir.join("charters").join("inbox.completed.actions")).unwrap();
    assert!(content.contains("[x] Unique Task Name"));
}

#[test]
fn test_complete_command_project_root_next_actions_uses_project_name() {
    let env = TestEnv::new();
    let project_root = env.work_dir.join("sample-project");
    let charters_dir = project_root.join(".clearhead").join("charters");
    fs::create_dir_all(&charters_dir).unwrap();

    let next_path = charters_dir.join("next.actions");
    let uuid = "019baaec-00b6-7991-be34-94b68212619b";
    fs::write(&next_path, format!("[ ] Project root task #{}", uuid)).unwrap();

    let mut cmd = env.command();
    cmd.current_dir(&project_root)
        .arg("complete").arg("action").arg(uuid)
        .arg("--file").arg(&next_path)
        .assert().success();

    let completed_path = charters_dir.join("sample-project.completed.actions");
    assert!(completed_path.exists(), "expected {} to exist", completed_path.display());
    let content = fs::read_to_string(&completed_path).unwrap();
    assert!(content.contains("[x] Project root task"));
    assert!(!charters_dir.join("charters.completed.actions").exists());
}

#[test]
fn test_archive_actions_project_root_next_actions_uses_project_name() {
    let env = TestEnv::new();
    let project_root = env.work_dir.join("sample-project");
    let charters_dir = project_root.join(".clearhead").join("charters");
    fs::create_dir_all(&charters_dir).unwrap();

    let next_path = charters_dir.join("next.actions");
    fs::write(&next_path, "[x] Already done").unwrap();

    let mut cmd = env.command();
    cmd.current_dir(&project_root)
        .arg("archive").arg("actions")
        .arg("--file").arg(&next_path)
        .assert().success();

    let completed_path = charters_dir.join("sample-project.completed.actions");
    assert!(completed_path.exists(), "expected {} to exist", completed_path.display());
    let content = fs::read_to_string(&completed_path).unwrap();
    assert!(content.contains("[x] Already done"));
    assert!(!charters_dir.join("charters.completed.actions").exists());
}

#[test]
fn test_complete_command_already_closed_is_typed_data() {
    // Verb errors are data (query_output.md): with stdout piped, an
    // already-completed target comes back as a branchable JSON result,
    // not stderr prose.
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[x] Already Done");
    env.command()
        .arg("complete").arg("action").arg("Already Done")
        .assert().failure()
        .stdout(predicate::str::contains(r#""kind":"already-closed""#))
        .stdout(predicate::str::contains(r#""state":"Completed""#));
}

#[test]
fn test_complete_command_unknown_target_is_typed_not_found() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[ ] Something else");
    env.command()
        .arg("complete").arg("action").arg("urn:uuid:01951111-dead-7000-8000-000000000009")
        .assert().failure()
        .stdout(predicate::str::contains(r#""kind":"not-found""#));
}

#[test]
fn test_read_acts_aggregates_all_files() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[ ] Inbox task");
    env.write_actions("work.actions", "[ ] Work task");
    let project_dir = env.data_dir.join("charters").join("project1");
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join("next.actions"), "[ ] Project task").unwrap();
    env.command()
        .arg("read").arg("actions")
        .assert().success()
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
        .arg("read").arg("actions").arg("--file").arg(&work_path)
        .assert().success()
        .stdout(predicate::str::contains("Work task"))
        .stdout(predicate::str::contains("Inbox task").not());
}

#[test]
fn test_read_acts_skips_hidden_directories() {
    let env = TestEnv::new();
    env.write_actions("inbox.actions", "[ ] Visible task");
    let hidden_dir = env.data_dir.join(".git");
    fs::create_dir_all(&hidden_dir).unwrap();
    fs::write(hidden_dir.join("state.actions"), "[ ] Hidden task").unwrap();
    env.command()
        .arg("read").arg("actions")
        .assert().success()
        .stdout(predicate::str::contains("Visible task"))
        .stdout(predicate::str::contains("Hidden task").not());
}

#[test]
fn test_read_acts_file_recovers_from_malformed_input() {
    // `read` now goes through the same workspace loader every other command uses
    // (Decision 34's relaxed reader): a malformed file becomes a warning plus
    // whatever recovered, not a hard failure — consistent with `doctor`, `sync`, etc.
    let env = TestEnv::new();
    env.write_text("charters/malformed.actions", "not valid actions syntax !!!\n[ ] Keep me\n");
    let path = env.data_dir.join("charters").join("malformed.actions");
    env.command()
        .arg("read").arg("actions").arg("--file").arg(&path)
        .assert().success()
        .stderr(predicate::str::contains("recoverable action"))
        .stdout(predicate::str::contains("Keep me"));
}

#[test]
fn test_read_actions_context_filter_exact_match() {
    let env = TestEnv::new();
    env.write_actions(
        "inbox.actions",
        "[ ] Write tests +work\n[ ] Buy milk +personal\n",
    );
    env.command()
        .arg("read").arg("actions")
        .arg("--context").arg("work")
        .assert().success()
        .stdout(predicate::str::contains("Write tests"))
        .stdout(predicate::str::contains("Buy milk").not());
}

#[test]
fn test_read_actions_context_filter_expands_hierarchy() {
    let env = TestEnv::new();
    // Config: computer → terminal → neovim
    env.write_config(r#"{"tag_hierarchies": {"computer": ["terminal"], "terminal": ["neovim"]}}"#);
    env.write_actions(
        "inbox.actions",
        "[ ] Edit config +neovim\n[ ] Browse web +browser\n[ ] Read a book +personal\n",
    );
    // Filtering by +computer should match +neovim (child of terminal, which is child of computer)
    env.command()
        .arg("read").arg("actions")
        .arg("--context").arg("computer")
        .assert().success()
        .stdout(predicate::str::contains("Edit config"))
        .stdout(predicate::str::contains("Browse web").not())
        .stdout(predicate::str::contains("Read a book").not());
}

#[test]
fn test_read_actions_context_filter_multiple_flags() {
    let env = TestEnv::new();
    env.write_actions(
        "inbox.actions",
        "[ ] Work task +work\n[ ] Personal task +personal\n[ ] Other task +other\n",
    );
    env.command()
        .arg("read").arg("actions")
        .arg("--context").arg("work")
        .arg("--context").arg("personal")
        .assert().success()
        .stdout(predicate::str::contains("Work task"))
        .stdout(predicate::str::contains("Personal task"))
        .stdout(predicate::str::contains("Other task").not());
}
