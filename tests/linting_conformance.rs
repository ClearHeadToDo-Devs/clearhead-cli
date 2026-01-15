mod common;
use clearhead_cli::{get_parsed_document, lint_document};
use std::fs;
use std::path::PathBuf;

/// Get all linting test cases from the examples/linting directory
fn get_linting_examples() -> Vec<(String, String, String)> {
    let mut examples = Vec::new();
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let linting_dir = manifest_dir.join("examples").join("linting");

    // Read all rule directories
    let entries = fs::read_dir(&linting_dir)
        .expect("Failed to read linting examples directory");

    for entry in entries {
        let entry = entry.expect("Failed to read directory entry");
        let path = entry.path();

        // Only process directories (skip README.md)
        if !path.is_dir() {
            continue;
        }

        let rule_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("Invalid directory name")
            .to_string();

        // Read error.actions and fixed.actions
        let error_path = path.join("error.actions");
        let fixed_path = path.join("fixed.actions");

        if error_path.exists() && fixed_path.exists() {
            let error_content = fs::read_to_string(&error_path)
                .expect(&format!("Failed to read {}/error.actions", rule_name));
            let fixed_content = fs::read_to_string(&fixed_path)
                .expect(&format!("Failed to read {}/fixed.actions", rule_name));

            examples.push((rule_name, error_content, fixed_content));
        }
    }

    // Sort for consistent test ordering
    examples.sort_by(|a, b| a.0.cmp(&b.0));
    examples
}

#[test]
fn test_linting_error_detection() {
    // List of currently implemented rules (updated to match spec codes)
    // Note: E001/E002 (duration/recurrence without do-date) are implemented but
    // can't be tested via parsing - the grammar requires D/R: to be inside do_date.
    // These are tested via unit tests with constructed Action objects.
    // E003, E006: Errors (parser correctness)
    // W002-W006: Warnings (temporal/semantic issues)
    // I001-I004: Info (style preferences)
    let implemented_rules = vec![
        "E003", "E006",  // Errors (E001, E002 tested via unit tests only)
        "W002", "W003", "W004", "W005", "W006",  // Warnings
        "I001", "I002", "I003", "I004",  // Info
    ];

    let examples = get_linting_examples();

    for (rule_name, error_content, _fixed_content) in &examples {
        // Extract the rule code from the directory name (e.g., "E001" from "E001_completed_without_date")
        let expected_code = rule_name.split('_').next().unwrap();

        // Skip unimplemented rules for now
        if !implemented_rules.contains(&expected_code) {
            eprintln!("Skipping unimplemented rule: {}", rule_name);
            continue;
        }

        // Parse the error.actions file
        let parsed = get_parsed_document(error_content)
            .expect(&format!("Failed to parse {}/error.actions", rule_name));

        // Lint the document
        let diagnostics = lint_document(&parsed);

        // Assert that we got diagnostics
        assert!(
            !diagnostics.is_empty(),
            "Rule {} should produce diagnostics for error.actions",
            rule_name
        );

        // Assert that at least one diagnostic has the expected code
        let has_expected_code = diagnostics.iter().any(|d| d.code == expected_code);
        assert!(
            has_expected_code,
            "Rule {} should produce diagnostic with code '{}', but got codes: {:?}",
            rule_name,
            expected_code,
            diagnostics.iter().map(|d| &d.code).collect::<Vec<_>>()
        );

        // Snapshot the diagnostics for review
        insta::with_settings!({
            snapshot_suffix => rule_name,
        }, {
            insta::assert_debug_snapshot!("lint_error", diagnostics);
        });
    }
}

#[test]
fn test_linting_fixed_version_passes() {
    // List of currently implemented rules (same as above)
    let implemented_rules = vec![
        "E003", "E006",  // Errors (E001, E002 tested via unit tests only)
        "W002", "W003", "W004", "W005", "W006",  // Warnings
        "I001", "I002", "I003", "I004",  // Info
    ];

    let examples = get_linting_examples();

    for (rule_name, _error_content, fixed_content) in &examples {
        // Extract the rule code from the directory name
        let rule_code = rule_name.split('_').next().unwrap();

        // Skip unimplemented rules for now
        if !implemented_rules.contains(&rule_code) {
            continue;
        }

        // Parse the fixed.actions file
        let parsed = get_parsed_document(fixed_content)
            .expect(&format!("Failed to parse {}/fixed.actions", rule_name));

        // Lint the document
        let diagnostics = lint_document(&parsed);

        // Assert that we got NO diagnostics for this specific rule
        // (other rules might still fire, which is okay)
        let has_rule_diagnostic = diagnostics.iter().any(|d| d.code == rule_code);
        assert!(
            !has_rule_diagnostic,
            "Rule {}/fixed.actions should not produce diagnostic with code '{}', but got: {:?}",
            rule_name,
            rule_code,
            diagnostics
        );
    }
}
