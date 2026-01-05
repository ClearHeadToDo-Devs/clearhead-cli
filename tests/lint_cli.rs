use std::fs;
use assert_cmd::Command; // Changed import
use predicates::prelude::*;

#[test]
fn test_lint_cli_errors() -> Result<(), Box<dyn std::error::Error>> {
    let file_content = "[ ] Action with no ID and invalid date @2025-99-99";
    let file_path = "test_lint_error.actions";
    
    fs::write(file_path, file_content)?;

    let mut cmd = Command::cargo_bin("clearhead_cli")?;

    cmd.arg("lint")
        .arg(file_path); // Positional argument

    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("E014"))
        .stdout(predicate::str::contains("ERROR"));

    fs::remove_file(file_path)?;

    Ok(())
}

#[test]
fn test_lint_cli_success() -> Result<(), Box<dyn std::error::Error>> {
    let file_content = "[ ] Valid Action ^2025-01-01T12:00 #01942d99-4c27-77f6-9316-107024843939";
    let file_path = "test_lint_success.actions";
    
    fs::write(file_path, file_content)?;

    let mut cmd = Command::cargo_bin("clearhead_cli")?;

    cmd.arg("lint")
        .arg(file_path); // Positional argument

    cmd.assert()
        .success()
        .stdout(predicate::str::is_empty());

    fs::remove_file(file_path)?;

    Ok(())
}
