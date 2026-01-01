use clearhead_cli::{format, FormatConfig, FormatStyle, OutputFormat};

#[test]
fn format_compact() {
    let examples = [
        ("minimal", include_str!("../examples/minimal.actions")),
        ("with_priority", include_str!("../examples/with_priority.actions")),
        ("with_children", include_str!("../examples/with_children.actions")),
        ("with_description", include_str!("../examples/with_description.actions")),
        ("with_context", include_str!("../examples/with_context.actions")),
        ("with_do_date", include_str!("../examples/with_do_date.actions")),
        ("with_story", include_str!("../examples/with_story.actions")),
        ("with_links", include_str!("../examples/with_links.actions")),
        ("recurring_log_example", include_str!("../examples/recurring_log_example.actions")),
        ("with_everything", include_str!("../examples/with_everything.actions")),
    ];

    for (name, input) in examples {
        let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), input)
            .expect("Failed to parse input");

        let config = FormatConfig {
            style: FormatStyle::Compact,
            indent_width: 4,
            include_id: false,
        };

        let formatted = format(&actions, OutputFormat::Actions, Some(config))
            .expect("Failed to format");

        insta::assert_snapshot!(format!("compact_{}", name), formatted);
    }
}

#[test]
fn format_list() {
    let examples = [
        ("minimal", include_str!("../examples/minimal.actions")),
        ("with_priority", include_str!("../examples/with_priority.actions")),
        ("with_children", include_str!("../examples/with_children.actions")),
        ("with_description", include_str!("../examples/with_description.actions")),
        ("with_context", include_str!("../examples/with_context.actions")),
        ("with_do_date", include_str!("../examples/with_do_date.actions")),
        ("with_story", include_str!("../examples/with_story.actions")),
        ("with_links", include_str!("../examples/with_links.actions")),
        ("recurring_log_example", include_str!("../examples/recurring_log_example.actions")),
        ("with_everything", include_str!("../examples/with_everything.actions")),
    ];

    for (name, input) in examples {
        let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), input)
            .expect("Failed to parse input");

        let config = FormatConfig {
            style: FormatStyle::List,
            indent_width: 4,
            include_id: false,
        };

        let formatted = format(&actions, OutputFormat::Actions, Some(config))
            .expect("Failed to format");

        insta::assert_snapshot!(format!("list_{}", name), formatted);
    }
}
