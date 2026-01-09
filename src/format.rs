use crate::entities::{Action, ActionList, ActionState};
use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
use serde::{Deserialize, Serialize};
use icalendar::{Calendar, Component, Event, EventLike, EventStatus};
use chrono::Utc;

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

/// Format ActionList as .actions file format with depth markers
///
/// Serializes actions with spec-compliant horizontal and vertical spacing.
/// Note: Topiary is not used because it cannot preserve horizontal spacing
/// due to grammar limitations (whitespace is captured as `extras`).
fn format_as_actions(list: &ActionList, config: Option<FormatConfig>) -> Result<String, String> {
    let config = config.unwrap_or_default();
    format_as_actions_basic(list, &config)
}

/// Basic serialization of ActionList to .actions format with proper spacing
///
/// This produces valid .actions syntax with spec-compliant horizontal spacing.
/// Topiary then handles vertical spacing (newlines, indentation in list mode).
fn format_as_actions_basic(list: &ActionList, config: &FormatConfig) -> Result<String, String> {
    use std::collections::HashMap;
    use std::fmt::Write as _;

    let mut output = String::new();

    // Build a map for O(1) parent lookups
    let parent_map: HashMap<uuid::Uuid, Option<uuid::Uuid>> =
        list.iter().map(|a| (a.id, a.parent_id)).collect();

    for action in list {
        // Calculate depth using the map instead of linear search
        let mut depth = 0;
        let mut current_parent = action.parent_id;
        while let Some(parent_id) = current_parent {
            depth += 1;
            current_parent = parent_map.get(&parent_id).copied().flatten();

            // Safety break for cycles (though tree-sitter shouldn't produce them)
            if depth > 100 {
                break;
            }
        }

        // Add leading indentation based on depth
        let indent_char = match config.indent_style {
            IndentStyle::Spaces => " ",
            IndentStyle::Tabs => "\t",
        };
        let indent_units = depth * config.indent_width;
        output.push_str(&indent_char.repeat(indent_units));

        // Add depth markers (> for each level of nesting)
        if depth > 0 {
            output.push_str(&">".repeat(depth));
        }

        // Serialize action WITH proper spacing (per formatting specification)
        // Space after state brackets: `[x] Task` not `[x]Task`

        if config.style == FormatStyle::List {
            // In List mode, we handle the metadata lines manually to control indentation
            write!(output, "[{}] {}", action.state, action.name).unwrap();

            let indent_char = match config.indent_style {
                IndentStyle::Spaces => " ",
                IndentStyle::Tabs => "\t",
            };
            let metadata_indent = format!(
                "\n{}",
                indent_char.repeat((depth + 1) * config.indent_width)
            );

            if let Some(description) = &action.description {
                output.push_str(&metadata_indent);
                write!(output, "$ {}", description).unwrap();
            }
            if let Some(priority) = &action.priority {
                output.push_str(&metadata_indent);
                write!(output, "!{}", priority).unwrap();
            }
            if let Some(story) = &action.story {
                output.push_str(&metadata_indent);
                write!(output, "*{}", story).unwrap();
            }
            if let Some(context_list) = &action.context_list {
                output.push_str(&metadata_indent);
                write!(output, "+{}", context_list.join(",")).unwrap();
            }
            if let Some(do_date_time) = &action.do_date_time {
                output.push_str(&metadata_indent);
                write!(output, "@{}", do_date_time.format("%Y-%m-%dT%H:%M")).unwrap();
                if let Some(duration) = action.do_duration {
                    output.push_str(&metadata_indent);
                    write!(output, "D{}", duration).unwrap();
                }
                if let Some(recurrence) = &action.recurrence {
                    output.push_str(&metadata_indent);
                    write!(output, "{}", recurrence).unwrap();
                }
            }
            if let Some(completed_date_time) = &action.completed_date_time {
                output.push_str(&metadata_indent);
                write!(output, "%{}", completed_date_time.format("%Y-%m-%dT%H:%M")).unwrap();
            }
            if let Some(created_date_time) = &action.created_date_time {
                output.push_str(&metadata_indent);
                write!(output, "^{}", created_date_time.format("%Y-%m-%dT%H:%M")).unwrap();
            }
            if config.include_id {
                output.push_str(&metadata_indent);
                write!(output, "#{}", action.id).unwrap();
            }
        } else {
            // In Compact mode, we can use the centralized fmt_content method
            use std::fmt;
            struct ActionContent<'a>(&'a Action, bool);
            impl<'a> fmt::Display for ActionContent<'a> {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    self.0.fmt_content(f, self.1)
                }
            }
            write!(output, "{}", ActionContent(action, config.include_id)).unwrap();
        }
        output.push('\n');
    }

    Ok(output)
}

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

/// Format ActionList as iCalendar (.ics) format
///
/// Converts actions with do_date_time into VEVENT components following RFC 5545.
/// Only includes actions that have a do_date_time set.
///
/// # Arguments
/// * `list` - The ActionList to export
/// * `open_only` - If true, only export actions with state: NotStarted, InProgress, or BlockedorAwaiting
///
/// # Returns
/// A formatted iCalendar string, or an error message if formatting fails
pub fn format_as_icalendar(list: &ActionList, open_only: bool) -> Result<String, String> {
    let mut calendar = Calendar::new()
        .name("ClearHead Actions")
        .description("Actions exported from ClearHead")
        .done();

    for action in list {
        // Skip actions without do_date_time (can't create calendar event without a time)
        if action.do_date_time.is_none() {
            continue;
        }

        // Filter by state if open_only is true
        if open_only {
            match action.state {
                ActionState::NotStarted | ActionState::InProgress | ActionState::BlockedorAwaiting => {
                    // Include these states
                }
                ActionState::Completed | ActionState::Cancelled => {
                    // Skip completed and cancelled actions
                    continue;
                }
            }
        }

        // Create event
        let mut event = Event::new();

        // UID: required by iCalendar spec
        event.uid(&action.id.to_string());

        // SUMMARY: action name
        event.summary(&action.name);

        // DESCRIPTION: optional description
        if let Some(description) = &action.description {
            event.description(description);
        }

        // DTSTART: do_date_time (required, we already checked it exists)
        // Convert DateTime<Local> to DateTime<Utc> for icalendar compatibility
        let start_time_utc = action.do_date_time.unwrap().with_timezone(&Utc);
        event.starts(start_time_utc);

        // DTEND: calculate end time based on duration (default 15 minutes if not specified)
        let duration_minutes = action.do_duration.unwrap_or(15);
        let end_time_utc = start_time_utc + chrono::Duration::minutes(duration_minutes as i64);
        event.ends(end_time_utc);

        // RRULE: recurrence rule
        if let Some(recurrence) = &action.recurrence {
            // Convert our Recurrence struct to RRULE string
            let rrule_str = format!("{}", recurrence);
            // Strip the "R:" prefix that our format uses
            let rrule = rrule_str.strip_prefix("R:").unwrap_or(&rrule_str);
            // Add as property using add_property
            event.add_property("RRULE", rrule);
        }

        // STATUS: map action state to iCalendar status
        let status = match action.state {
            ActionState::NotStarted => EventStatus::Tentative,
            ActionState::InProgress => EventStatus::Confirmed,
            ActionState::Completed => EventStatus::Confirmed, // Completed events are still CONFIRMED
            ActionState::BlockedorAwaiting => EventStatus::Tentative,
            ActionState::Cancelled => EventStatus::Cancelled,
        };
        event.status(status);

        // COMPLETED: completion date for completed actions
        if action.state == ActionState::Completed {
            if let Some(completed_date_time) = action.completed_date_time {
                event.timestamp(completed_date_time.with_timezone(&Utc));
            }
        }

        // PRIORITY: map our 1-4 priority to iCalendar 1-9 scale
        // Our priority: 1 (highest) to 4 (lowest)
        // iCalendar: 1 (highest) to 9 (lowest), 0 = undefined
        // Mapping: our 1→iCal 1, our 2→iCal 3, our 3→iCal 5, our 4→iCal 7
        if let Some(priority) = action.priority {
            let ical_priority = match priority {
                1 => 1,
                2 => 3,
                3 => 5,
                4 => 7,
                _ => 5, // default to medium
            };
            event.priority(ical_priority);
        }

        // CATEGORIES: context list (comma-separated per RFC 5545)
        if let Some(context_list) = &action.context_list {
            let categories = context_list.join(",");
            event.add_property("CATEGORIES", &categories);
        }

        // Add event to calendar
        calendar.push(event);
    }

    Ok(calendar.to_string())
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
            created_date_time: None,
            predecessors: None,
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
            "Output doesn't contain '[x] Root' (with space): {}",
            formatted
        );
        assert!(
            formatted.contains(">[ ] Child"),
            "Output doesn't contain '>[ ] Child' (with space): {}",
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

    #[test]
    fn test_indentation() {
        let mut actions = vec![create_test_action("Root", ActionState::NotStarted, None)];
        let root_id = actions[0].id;
        actions.push(create_test_action(
            "Child",
            ActionState::NotStarted,
            Some(root_id),
        ));

        // Test Spaces
        let config = FormatConfig {
            style: FormatStyle::Compact,
            indent_style: IndentStyle::Spaces,
            indent_width: 2,
            ..Default::default()
        };
        let formatted = format(&actions, OutputFormat::Actions, Some(config)).unwrap();
        assert!(formatted.contains("  >[ ] Child"));

        // Test Tabs
        let config_tabs = FormatConfig {
            style: FormatStyle::Compact,
            indent_style: IndentStyle::Tabs,
            indent_width: 1,
            ..Default::default()
        };
        let formatted_tabs = format(&actions, OutputFormat::Actions, Some(config_tabs)).unwrap();
        assert!(formatted_tabs.contains("\t>[ ] Child"));

        // Test List Style Indent
        let mut actions_meta = vec![create_test_action("Root", ActionState::NotStarted, None)];
        actions_meta[0].description = Some("Desc".to_string());

        let config_list = FormatConfig {
            style: FormatStyle::List,
            indent_style: IndentStyle::Spaces,
            indent_width: 4,
            ..Default::default()
        };
        let formatted_list =
            format(&actions_meta, OutputFormat::Actions, Some(config_list)).unwrap();
        assert!(formatted_list.contains("\n    $ Desc"));
    }
}
