use crate::entities::{Action, ActionList};
use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use topiary_core::{Operation, formatter, Language, TopiaryQuery};
use topiary_tree_sitter_facade::Language as TreeSitterLanguage;

/// Output format options for ActionList serialization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// .actions file format with depth markers
    Actions,
    /// JSON format (pretty-printed)
    Json,
    /// XML format
    Xml,
    /// Human-readable table format
    Table,
}

/// Formatting style for .actions files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormatStyle {
    /// Compact: metadata on same line
    Compact,
    /// List: metadata on separate indented lines
    /// Note: List style is deprecated in spec v2.0.0
    List,
}

/// Indentation style for .actions files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndentStyle {
    /// Use spaces for indentation
    Spaces,
    /// Use tabs for indentation
    Tabs,
}

/// Configuration for .actions file formatting
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatConfig {
    /// Formatting style (compact or list)
    pub style: FormatStyle,
    /// Indentation style (spaces or tabs)
    pub indent_style: IndentStyle,
    /// Number of units per indentation level
    pub indent_width: usize,
    /// Whether to include UUIDs in formatted output
    pub include_id: bool,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            style: FormatStyle::Compact,
            indent_style: IndentStyle::Spaces,
            indent_width: 4,
            include_id: true,
        }
    }
}

/// Format an ActionList to the specified output format
///
/// This is the hub function that dispatches to format-specific implementations.
///
/// # Arguments
/// * `list` - The ActionList to format
/// * `format` - The desired output format
/// * `config` - Optional formatting configuration (only used for OutputFormat::Actions)
///
/// # Returns
/// A formatted string, or an error message if formatting fails
pub fn format(
    list: &ActionList,
    format: OutputFormat,
    config: Option<FormatConfig>,
) -> Result<String, String> {
    match format {
        OutputFormat::Actions => format_as_actions(list, config),
        OutputFormat::Json => format_as_json(list),
        OutputFormat::Xml => format_as_xml(list),
        OutputFormat::Table => format_as_table(list),
    }
}

/// Format ActionList as .actions file format using Topiary
fn format_as_actions(list: &ActionList, config: Option<FormatConfig>) -> Result<String, String> {
    let config = config.unwrap_or_default();
    
    // 1. Generate basic content (Canonical style: Compact + Indentation)
    // We strictly use Compact style now as List style is removed in spec v2.0.0
    let raw_content = serialize_actions_canonical(list, &config);

    // 2. Pass through Topiary for structural enforcement (vertical spacing)
    format_with_topiary(&raw_content)
}

/// Serialize actions to the canonical string format (compact, no indentation)
fn serialize_actions_canonical(list: &ActionList, config: &FormatConfig) -> String {
    use std::collections::HashMap;
    use std::fmt::Write as _;
    use std::fmt;

    let mut output = String::new();

    // Build a map for O(1) parent lookups
    let parent_map: HashMap<uuid::Uuid, Option<uuid::Uuid>> =
        list.iter().map(|a| (a.id, a.parent_id)).collect();

    // Helper for Compact content formatting
    struct ActionContent<'a>(&'a Action, bool);
    impl<'a> fmt::Display for ActionContent<'a> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            self.0.fmt_content(f, self.1)
        }
    }

    for action in list {
        // Calculate depth
        let mut depth = 0;
        let mut current_parent = action.parent_id;
        while let Some(parent_id) = current_parent {
            depth += 1;
            current_parent = parent_map.get(&parent_id).copied().flatten();
            if depth > 100 { break; } // Safety break
        }

        // Add depth markers
        if depth > 0 {
            output.push_str(&">".repeat(depth));
        }

        // Serialize content (Compact style)
        // Note: fmt_content handles the "[x] Name ..." part
        write!(output, "{}", ActionContent(action, config.include_id)).unwrap();
        output.push('\n');
    }

    output
}

/// Format a string using Topiary with the .actions grammar
fn format_with_topiary(input_text: &str) -> Result<String, String> {
    let mut input = Cursor::new(input_text.as_bytes());
    let mut output = Vec::new();

    // Load query from tree-sitter-actions crate
    let query_content = tree_sitter_actions::FORMATTING_QUERY;

    // Initialize Grammar and Query
    let grammar: TreeSitterLanguage = tree_sitter_actions::LANGUAGE.into();
    let query = TopiaryQuery::new(&grammar, query_content)
        .map_err(|e| format!("Failed to compile Topiary query: {}", e))?;

    // Initialize Language
    let language = Language {
        name: "actions".to_string(),
        grammar, // consumes grammar
        indent: None, // No auto-indentation config
        query,
    };

    // Configure Operation
    let operation = Operation::Format {
        skip_idempotence: true,
        tolerate_parsing_errors: true,
    };

    // Run Formatter
    formatter(&mut input, &mut output, &language, operation)
        .map_err(|e| format!("Topiary formatting failed: {}", e))?;

    String::from_utf8(output)
        .map_err(|e| format!("Topiary output was not valid UTF-8: {}", e))
}

fn format_as_json(list: &ActionList) -> Result<String, String> {
    // Wrapper to match schema format with JSON-LD context: {"@context": "...", "actions": [...]}
    #[derive(Serialize)]
    struct ActionsWrapper<'a> {
        #[serde(rename = "@context")]
        context: &'a str,
        actions: &'a [Action],
    }

    let wrapper = ActionsWrapper {
        context: "https://clearhead.to/schemas/actions.context.json",
        actions: list
    };
    serde_json::to_string_pretty(&wrapper).map_err(|e| format!("JSON formatting failed: {}", e))
}

/// Format ActionList as XML
fn format_as_xml(list: &ActionList) -> Result<String, String> {
    // Wrapper struct to provide a root element for XML serialization
    #[derive(Serialize)]
    struct ActionListWrapper<'a> {
        #[serde(rename = "action")]
        actions: &'a [Action],
    }

    let wrapper = ActionListWrapper { actions: list };
    quick_xml::se::to_string(&wrapper).map_err(|e| format!("XML formatting failed: {}", e))
}

/// Format ActionList as a human-readable table using comfy-table
fn format_as_table(list: &ActionList) -> Result<String, String> {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Add header
    table.set_header(vec![
        Cell::new("State").fg(Color::Cyan),
        Cell::new("Name").fg(Color::Cyan),
        Cell::new("Priority").fg(Color::Cyan),
        Cell::new("Due").fg(Color::Cyan),
        Cell::new("Dur").fg(Color::Cyan),
        Cell::new("Recurrence").fg(Color::Cyan),
        Cell::new("Context").fg(Color::Cyan),
        Cell::new("Description").fg(Color::Cyan),
        Cell::new("ID").fg(Color::Cyan),
    ]);

    // Add rows for each action
    for action in list {
        let depth = action.depth(list);
        let indent = "  ".repeat(depth.try_into().unwrap());

        // Format name with indentation to show hierarchy
        let name = format!("{}{}", indent, action.name);

        // Format state as character
        let state = format!("{}", action.state);

        // Format priority
        let priority = action
            .priority
            .map(|p| p.to_string())
            .unwrap_or_else(|| "-".to_string());

        // Format due date
        let due = action
            .do_date_time
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "-".to_string());

        // Format duration
        let duration = action
            .do_duration
            .map(|d| format!("{}m", d))
            .unwrap_or_else(|| "-".to_string());

        // Format recurrence
        let recurrence = action
            .recurrence
            .as_ref()
            .map(|r| r.frequency.to_uppercase())
            .unwrap_or_else(|| "-".to_string());

        // Format context list
        let context = action
            .context_list
            .as_ref()
            .map(|c| c.join(", "))
            .unwrap_or_else(|| "-".to_string());

        // Format description
        let description = action
            .description
            .as_ref()
            .map(|d| d.to_string())
            .unwrap_or_else(|| "-".to_string());

        // Format ID (short version - first 8 chars)
        let id = action.id.to_string()[..8].to_string();

        table.add_row(vec![
            state,
            name,
            priority,
            due,
            duration,
            recurrence,
            context,
            description,
            id,
        ]);
    }

    Ok(table.to_string())
}

// Note: iCalendar export has been moved to src/export.rs for better modularity and testability.
// Re-exported from lib.rs as `pub use export::format_as_icalendar;`

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{Action, ActionState};
    use uuid::Uuid;

    fn create_test_action(name: &str, state: ActionState, parent_id: Option<Uuid>) -> Action {
        Action {
            parent_id,
            state,
            ..Action::new(name)
        }
    }

    #[test]
    fn test_format_as_actions() {
        let mut actions = vec![create_test_action("Root", ActionState::Completed, None)];
        let root_id = actions[0].id;
        actions.push(create_test_action(
            "Child",
            ActionState::NotStarted,
            Some(root_id),
        ));

        let formatted = format_as_actions(&actions, None).unwrap();

        // Debug: print the output
        eprintln!("Formatted output:\n{}", formatted);

        assert!(
            formatted.contains("[x] Root"),
            "Output doesn't contain '[x] Root': {}",
            formatted
        );
        assert!(
            formatted.contains(">[ ] Child"),
            "Output doesn't contain '>[ ] Child': {}",
            formatted
        );
        // Verify newlines (Topiary effect)
        assert!(formatted.contains("\n"));
    }

    #[test]
    fn test_format_as_json() {
        let actions = vec![create_test_action("Test", ActionState::NotStarted, None)];

        let formatted = format_as_json(&actions).unwrap();

        assert!(formatted.contains("\"actions\":"));
        assert!(formatted.contains("\"name\": \"Test\""));
        assert!(formatted.contains("\"state\": \"not_started\""));
    }

    #[test]
    fn test_format_as_xml() {
        let actions = vec![create_test_action("Test", ActionState::NotStarted, None)];

        let formatted = format_as_xml(&actions).unwrap();

        assert!(formatted.contains("<name>Test</name>"));
        assert!(formatted.contains("<state>not_started</state>"));
    }

    #[test]
    fn test_format_as_table() {
        let actions = vec![
            create_test_action("Task 1", ActionState::Completed, None),
            create_test_action("Task 2", ActionState::NotStarted, None),
        ];

        let formatted = format_as_table(&actions).unwrap();

        // Should contain header and task names
        assert!(formatted.contains("State"));
        assert!(formatted.contains("Name"));
        assert!(formatted.contains("Task 1"));
        assert!(formatted.contains("Task 2"));
    }

    #[test]
    fn test_indentation_removed() {
        let mut actions = vec![create_test_action("Root", ActionState::NotStarted, None)];
        let root_id = actions[0].id;
        actions.push(create_test_action(
            "Child",
            ActionState::NotStarted,
            Some(root_id),
        ));

        // Test Spaces - Should be ignored and flat
        let config = FormatConfig {
            style: FormatStyle::Compact,
            indent_style: IndentStyle::Spaces,
            indent_width: 2,
            ..Default::default()
        };
        let formatted = format(&actions, OutputFormat::Actions, Some(config)).unwrap();
        // Expect NO indentation
        assert!(formatted.contains(">[ ] Child"));
        assert!(!formatted.contains("  >[ ] Child"));

        // Test Tabs - Should be ignored and flat
        let config_tabs = FormatConfig {
            style: FormatStyle::Compact,
            indent_style: IndentStyle::Tabs,
            indent_width: 1,
            ..Default::default()
        };
        let formatted_tabs = format(&actions, OutputFormat::Actions, Some(config_tabs)).unwrap();
        assert!(formatted_tabs.contains(">[ ] Child"));
        assert!(!formatted_tabs.contains("\t>[ ] Child"));
    }
}

