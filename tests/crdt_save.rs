use clearhead_cli::crdt::ActionRepository;
use clearhead_cli::entities::Action;
use clearhead_cli::get_parsed_document;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_save_returns_formatted_content_without_writing_file() {
    // Setup: Create temp workspace
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.actions");

    // Write initial file
    fs::write(&file_path, "[ ] Initial action\n").unwrap();

    // Load repository using test constructor
    let mut repo =
        ActionRepository::test_repo(file_path.clone(), temp_dir.path().to_path_buf()).unwrap();

    // Create new action
    let mut action = Action::new("Buy milk");
    action.id = uuid::Uuid::now_v7();
    let actions = vec![action];

    // Save to CRDT (should return formatted content)
    let formatted = repo.save(&actions).unwrap();

    // Verify formatted content contains UUID
    assert!(formatted.contains("#"));
    assert!(formatted.contains("Buy milk"));

    // Verify file was NOT modified (still has old content)
    let file_content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(file_content, "[ ] Initial action\n");
    assert!(!file_content.contains("Buy milk"));
}

#[test]
fn test_uuid_stability_across_parses() {
    // Setup
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.actions");
    fs::write(&file_path, "[ ] Task\n").unwrap();

    // First save: assign UUID
    let mut repo =
        ActionRepository::test_repo(file_path.clone(), temp_dir.path().to_path_buf()).unwrap();
    let parsed1 = get_parsed_document("[ ] Task\n").unwrap();
    let formatted1 = repo.save(&parsed1.actions).unwrap();

    // Extract UUID from formatted output
    let uuid_start = formatted1.find('#').unwrap();
    let uuid_str1 = &formatted1[uuid_start + 1..uuid_start + 37];

    // Parse the formatted output and save again
    let parsed2 = get_parsed_document(&formatted1).unwrap();
    let formatted2 = repo.save(&parsed2.actions).unwrap();

    // Extract UUID again
    let uuid_start2 = formatted2.find('#').unwrap();
    let uuid_str2 = &formatted2[uuid_start2 + 1..uuid_start2 + 37];

    // Verify UUIDs are the same
    assert_eq!(uuid_str1, uuid_str2);
}

#[test]
fn test_whitespace_normalization() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.actions");
    fs::write(&file_path, "[ ]  Task   \n").unwrap();

    let mut repo =
        ActionRepository::test_repo(file_path.clone(), temp_dir.path().to_path_buf()).unwrap();
    let parsed = get_parsed_document("[ ]  Task   \n").unwrap();
    let formatted = repo.save(&parsed.actions).unwrap();

    // Verify normalized (no double spaces, no trailing spaces)
    assert!(!formatted.contains("  ")); // No double spaces
    assert!(!formatted.contains("Task   ")); // No trailing spaces
}

#[test]
fn test_project_to_file_still_works_for_cli() {
    // This ensures CLI commands can still write directly to files
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.actions");
    fs::write(&file_path, "[ ] Old content\n").unwrap();

    let repo =
        ActionRepository::test_repo(file_path.clone(), temp_dir.path().to_path_buf()).unwrap();
    let parsed = get_parsed_document("[ ] New task\n").unwrap();

    // Call project_to_file directly (for CLI usage)
    repo.project_to_file(&parsed.actions).unwrap();

    // Verify file WAS written
    let file_content = fs::read_to_string(&file_path).unwrap();
    assert!(file_content.contains("New task"));
}
