mod common;
use clearhead_cli::{FormatConfig, FormatStyle, OutputFormat, format};
use common::get_examples;
#[test]
fn format_compact() {
    for (name, input) in get_examples() {
        let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &input)
            .expect("Failed to parse input");

        let config = FormatConfig {
            style: FormatStyle::Compact,
            indent_width: 4,
            include_id: false,
        };

        let formatted =
            format(&actions, OutputFormat::Actions, Some(config)).expect("Failed to format");

        insta::assert_snapshot!(format!("compact_{}", name), formatted);
    }
}

#[test]
fn format_list() {
    for (name, input) in get_examples() {
        let actions = clearhead_cli::get_action_list_struct(&serde_json::json!({}), &input)
            .expect("Failed to parse input");

        let config = FormatConfig {
            style: FormatStyle::List,
            indent_width: 4,
            include_id: false,
        };

        let formatted =
            format(&actions, OutputFormat::Actions, Some(config)).expect("Failed to format");

        insta::assert_snapshot!(format!("list_{}", name), formatted);
    }
}
