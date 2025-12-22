use crate::entities::{Action, ActionList};
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use serde::Serialize;

/// Output format options for ActionList serialization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// .actions file format with depth markers (>, >>, etc.)
    Actions,
    /// JSON format (pretty-printed)
    Json,
    /// XML format
    Xml,
    /// Human-readable table format
    Table,
}

/// Format an ActionList to the specified output format
///
/// This is the hub function that dispatches to format-specific implementations.
///
/// # Arguments
/// * `list` - The ActionList to format
/// * `format` - The desired output format
///
/// # Returns
/// A formatted string, or an error message if formatting fails
pub fn format(list: &ActionList, format: OutputFormat) -> Result<String, String> {
    match format {
        OutputFormat::Actions => format_as_actions(list),
        OutputFormat::Json => format_as_json(list),
        OutputFormat::Xml => format_as_xml(list),
        OutputFormat::Table => format_as_table(list),
    }
}

/// Format ActionList as .actions file format with depth markers
fn format_as_actions(list: &ActionList) -> Result<String, String> {
    let mut output = String::new();

    for action in list {
        let depth = action.depth(list);

        // Add depth markers (> for each level of nesting)
        if depth > 0 {
            output.push_str(&">".repeat(depth));
            output.push(' ');
        }

        // Use the Action's Display implementation for the content
        output.push_str(&format!("{}\n", action));
    }

    Ok(output)
}

/// Format ActionList as pretty-printed JSON
fn format_as_json(list: &ActionList) -> Result<String, String> {
    serde_json::to_string_pretty(list).map_err(|e| format!("JSON formatting failed: {}", e))
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
        Cell::new("Context").fg(Color::Cyan),
        Cell::new("Description").fg(Color::Cyan),
        Cell::new("ID").fg(Color::Cyan),
    ]);

    // Add rows for each action
    for action in list {
        let depth = action.depth(list);
        let indent = "  ".repeat(depth);

        // Format name with indentation to show hierarchy
        let name = format!("{}{}", indent, action.name);

        // Format state as character
        let state = format!("{}", action.state);

        // Format priority
        let priority = action
            .priority
            .map(|p| p.to_string())
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

        table.add_row(vec![state, name, priority, context, description, id]);
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
            completed_date_time: None,
            story: None,
        }
    }

    #[test]
    fn test_format_as_actions() {
        let mut actions = vec![create_test_action("Root", ActionState::Completed, None)];
        let root_id = actions[0].id;
        actions.push(create_test_action("Child", ActionState::NotStarted, Some(root_id)));

        let formatted = format_as_actions(&actions).unwrap();

        assert!(formatted.contains("[x] Root"));
        assert!(formatted.contains("> [ ] Child"));
    }

    #[test]
    fn test_format_as_json() {
        let actions = vec![create_test_action("Test", ActionState::NotStarted, None)];

        let formatted = format_as_json(&actions).unwrap();

        assert!(formatted.contains("\"name\": \"Test\""));
        assert!(formatted.contains("\"state\": \"NotStarted\""));
    }

    #[test]
    fn test_format_as_xml() {
        let actions = vec![create_test_action("Test", ActionState::NotStarted, None)];

        let formatted = format_as_xml(&actions).unwrap();

        assert!(formatted.contains("<name>Test</name>"));
        assert!(formatted.contains("<state>NotStarted</state>"));
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
