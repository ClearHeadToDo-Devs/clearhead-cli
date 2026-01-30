#[test]
fn test_json_output_includes_context() {
    use clearhead_cli::format::{format, OutputFormat};
    use clearhead_cli::entities::Action;

    let actions = vec![
        Action {
            priority: Some(1),
            ..Action::new("Test action")
        }
    ];

    let result = format(&actions, OutputFormat::Json, None, None).unwrap();

    // The JSON should contain @context reference
    assert!(result.contains("@context"));
    assert!(result.contains("actions.context.json"));
}