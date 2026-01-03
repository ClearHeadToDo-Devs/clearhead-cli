use crate::entities::{Action, ActionList};
use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
use serde::Serialize;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatStyle {
    /// Compact: metadata on same line
    Compact,
    /// List: metadata on separate indented lines
    List,
}

/// Configuration for .actions file formatting
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatConfig {
    /// Formatting style (compact or list)
    pub style: FormatStyle,
    /// Number of spaces per indentation level (for list style)
    pub indent_width: usize,
    /// Whether to include UUIDs in formatted output
    pub include_id: bool,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            style: FormatStyle::Compact,
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

/// Format ActionList as .actions file format with depth markers
///
/// Uses Topiary for formatting with support for both compact and list styles.
fn format_as_actions(list: &ActionList, config: Option<FormatConfig>) -> Result<String, String> {
    let config = config.unwrap_or_default();

    // First, serialize the ActionList to unformatted .actions text
    let unformatted = format_as_actions_basic(list, &config)?;

    // Then format it with Topiary
    format_with_topiary(&unformatted, &config)
}

/// Basic serialization of ActionList to .actions format (unformatted)
///
/// This produces valid .actions syntax but without any spacing.
/// Topiary will add all the spacing via the query.
fn format_as_actions_basic(list: &ActionList, config: &FormatConfig) -> Result<String, String> {
    use std::fmt::Write as _;

    let mut output = String::new();

    for action in list {
        let depth = action.depth(list);

        // Add depth markers (> for each level of nesting)
        if depth > 0 {
            output.push_str(&">".repeat(depth.try_into().unwrap()));
        }

        // Serialize action WITHOUT spacing (Topiary adds it)
        write!(output, "[{}]{}", action.state, action.name).unwrap();

        // Add metadata without spacing
        if let Some(description) = &action.description {
            write!(output, "${}", description).unwrap();
        }
        if let Some(priority) = &action.priority {
            write!(output, "!{}", priority).unwrap();
        }
        if let Some(story) = &action.story {
            write!(output, "*{}", story).unwrap();
        }
        if let Some(context_list) = &action.context_list {
            write!(output, "+{}", context_list.join(",")).unwrap();
        }
        if let Some(do_date_time) = &action.do_date_time {
            write!(output, "@{}", do_date_time.format("%Y-%m-%dT%H:%M")).unwrap();
            if let Some(duration) = action.do_duration {
                write!(output, "D{}", duration).unwrap();
            }
            if let Some(recurrence) = &action.recurrence {
                write!(output, "{}", recurrence).unwrap();
            }
        }
        if let Some(completed_date_time) = &action.completed_date_time {
            write!(output, "%{}", completed_date_time.format("%Y-%m-%dT%H:%M")).unwrap();
        }
        if config.include_id {
            write!(output, "#{}", action.id).unwrap();
        }
        output.push('\n');
    }

    Ok(output)
}

/// Format .actions text using Topiary
///
/// Applies the formatting rules defined in queries/actions/topiary.scm
pub fn format_with_topiary(input: &str, config: &FormatConfig) -> Result<String, String> {
    use topiary_core::{Language, Operation, TopiaryQuery, formatter};

    // Convert tree-sitter language to Topiary's facade
    let grammar = topiary_tree_sitter_facade::Language::from(tree_sitter_actions::LANGUAGE);

    // Create the query
    let query = TopiaryQuery::new(&grammar, tree_sitter_actions::TOPIARY_QUERY)
        .map_err(|e| format!("Failed to parse Topiary query: {}", e))?;

    // Create language configuration
    let language = Language {
        name: "actions".to_string(),
        query,
        grammar,
        indent: Some(" ".repeat(config.indent_width)),
    };

    // Create operation with formatting options
    // Note: Topiary's multi-line mode is controlled via query predicates, not operation flags
    // TODO: Fix remaining idempotence issue, then set skip_idempotence: false
    let operation = Operation::Format {
        skip_idempotence: true, // Skip while query has minor idempotence issues
        tolerate_parsing_errors: config.style == FormatStyle::List,
    };

    // Format the input
    let mut output = Vec::new();
    formatter(&mut input.as_bytes(), &mut output, &language, operation)
        .map_err(|e| format!("Topiary formatting failed: {}", e))?;

    String::from_utf8(output).map_err(|e| format!("Invalid UTF-8 in formatted output: {}", e))
}

/// Format ActionList as pretty-printed JSON
fn format_as_json(list: &ActionList) -> Result<String, String> {
    // Wrapper to match schema format: {"actions": [...]}
    #[derive(Serialize)]
    struct ActionsWrapper<'a> {
        actions: &'a [Action],
    }

    let wrapper = ActionsWrapper { actions: list };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{Action, ActionState};
    use uuid::Uuid;

    fn create_test_action(name: &str, state: ActionState, parent_id: Option<Uuid>) -> Action {
        Action {
            id: Uuid::new_v4(),
            parent_id,
            state,
            name: name.to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            story: None,
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
            formatted.contains("[ ] Child"),
            "Output doesn't contain '[ ] Child': {}",
            formatted
        );
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
}
