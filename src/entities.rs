use chrono::{DateTime, Local};
use rrule::Tz;
use serde::{Deserialize, Serialize};
use autosurgeon::{Hydrate, Reconcile};
use crate::sync_utils::{hydrate_date, reconcile_date};
use std::fmt;
use std::collections::HashMap;

use crate::treesitter::{create_node_wrapper, get_node_text, NodeWrapper, TreeWrapper};
use uuid::Uuid;

pub type ActionList = Vec<Action>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceRange {
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceMetadata {
    pub root: SourceRange,
    pub line_range: SourceRange,
    pub do_date: Option<SourceRange>,
    pub completed_date: Option<SourceRange>,
    pub created_date: Option<SourceRange>,
    pub is_id_generated: bool,
    pub raw_id: Option<String>,
}

#[derive(Debug)]
pub struct ParsedDocument {
    pub actions: ActionList,
    pub source_map: HashMap<Uuid, SourceMetadata>,
    pub tag_index: HashMap<String, Vec<SourceRange>>,
}

/// Recurrence rule per RFC 5545 RRULE specification
#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Reconcile, Hydrate)]
pub struct Recurrence {
    pub frequency: String,           // FREQ: secondly, minutely, hourly, daily, weekly, monthly, yearly
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<u32>,       // INTERVAL: default 1
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,          // COUNT: max occurrences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,       // UNTIL: end date/time in ISO 8601
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_second: Option<Vec<u32>>, // BYSECOND: 0-59
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_minute: Option<Vec<u32>>, // BYMINUTE: 0-59
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_hour: Option<Vec<u32>>,   // BYHOUR: 0-23
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_day: Option<Vec<String>>, // BYDAY: MO,TU,WE,TH,FR,SA,SU (with optional modifiers like 1MO, -1FR)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_month_day: Option<Vec<i32>>, // BYMONTHDAY: 1-31 or -1 to -31
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_year_day: Option<Vec<i32>>,  // BYYEARDAY: 1-366 or -1 to -366
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_week_no: Option<Vec<i32>>,   // BYWEEKNO: 1-53 or -1 to -53
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_month: Option<Vec<u32>>,     // BYMONTH: 1-12
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_set_pos: Option<Vec<i32>>,   // BYSETPOS: limits to nth occurrence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub week_start: Option<String>,     // WKST: MO,TU,WE,TH,FR,SA,SU (default MO)
}

#[derive(PartialEq, Debug, Clone, Serialize, Deserialize, Reconcile, Hydrate)]
pub struct Action {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Uuid>,
    pub state: ActionState,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u32>,
    #[serde(rename = "contexts", skip_serializing_if = "Option::is_none")]
    pub context_list: Option<Vec<String>>,
    #[serde(rename = "doDateTime", skip_serializing_if = "Option::is_none")]
    #[autosurgeon(reconcile = "reconcile_date", hydrate = "hydrate_date")]
    pub do_date_time: Option<DateTime<Local>>,
    #[serde(rename = "doDuration", skip_serializing_if = "Option::is_none")]
    pub do_duration: Option<u32>, // Duration in minutes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence: Option<Recurrence>,
    #[serde(rename = "completedDate", skip_serializing_if = "Option::is_none")]
    #[autosurgeon(reconcile = "reconcile_date", hydrate = "hydrate_date")]
    pub completed_date_time: Option<DateTime<Local>>,
    #[serde(rename = "createdDate", skip_serializing_if = "Option::is_none")]
    #[autosurgeon(reconcile = "reconcile_date", hydrate = "hydrate_date")]
    pub created_date_time: Option<DateTime<Local>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story: Option<String>,
}

impl fmt::Display for Recurrence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "R:FREQ={}", self.frequency.to_uppercase())?;

        if let Some(interval) = self.interval {
            write!(f, ";INTERVAL={}", interval)?;
        }
        if let Some(count) = self.count {
            write!(f, ";COUNT={}", count)?;
        }
        if let Some(until) = &self.until {
            write!(f, ";UNTIL={}", until)?;
        }
        if let Some(by_second) = &self.by_second {
            write!(f, ";BYSECOND={}", by_second.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))?;
        }
        if let Some(by_minute) = &self.by_minute {
            write!(f, ";BYMINUTE={}", by_minute.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))?;
        }
        if let Some(by_hour) = &self.by_hour {
            write!(f, ";BYHOUR={}", by_hour.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))?;
        }
        if let Some(by_day) = &self.by_day {
            write!(f, ";BYDAY={}", by_day.join(","))?;
        }
        if let Some(by_month_day) = &self.by_month_day {
            write!(f, ";BYMONTHDAY={}", by_month_day.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))?;
        }
        if let Some(by_year_day) = &self.by_year_day {
            write!(f, ";BYYEARDAY={}", by_year_day.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))?;
        }
        if let Some(by_week_no) = &self.by_week_no {
            write!(f, ";BYWEEKNO={}", by_week_no.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))?;
        }
        if let Some(by_month) = &self.by_month {
            write!(f, ";BYMONTH={}", by_month.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))?;
        }
        if let Some(by_set_pos) = &self.by_set_pos {
            write!(f, ";BYSETPOS={}", by_set_pos.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))?;
        }
        if let Some(week_start) = &self.week_start {
            write!(f, ";WKST={}", week_start)?;
        }

        Ok(())
    }
}

impl Action {
    /// Serialize the action content (state, name, metadata) to a formatter
    pub fn fmt_content(&self, f: &mut fmt::Formatter<'_>, include_id: bool) -> fmt::Result {
        // State and name (required)
        write!(f, "[{}] {}", self.state, self.name)?;

        // Add metadata with spec-compliant spacing:
        // - Space after $ (description icon only)
        // - No space after other icons
        if let Some(description) = &self.description {
            write!(f, " $ {}", description)?;
        }
        if let Some(priority) = &self.priority {
            write!(f, " !{}", priority)?;
        }
        if let Some(story) = &self.story {
            write!(f, " *{}", story)?;
        }
        if let Some(context_list) = &self.context_list {
            write!(f, " +{}", context_list.join(","))?;
        }
        if let Some(do_date_time) = &self.do_date_time {
            write!(f, " @{}", do_date_time.format("%Y-%m-%dT%H:%M"))?;
            if let Some(duration) = self.do_duration {
                write!(f, " D{}", duration)?;
            }
            if let Some(recurrence) = &self.recurrence {
                write!(f, " {}", recurrence)?;
            }
        }
        if let Some(completed_date_time) = &self.completed_date_time {
            write!(f, " %{}", completed_date_time.format("%Y-%m-%dT%H:%M"))?;
        }
        if let Some(created_date_time) = &self.created_date_time {
            write!(f, " ^{}", created_date_time.format("%Y-%m-%dT%H:%M"))?;
        }
        if include_id {
            write!(f, " #{}", self.id)?;
        }
        Ok(())
    }

    /// Compute the depth of this action by walking up the parent chain
    pub fn depth(&self, action_list: &ActionList) -> u32 {
        let mut depth = 0;
        let mut current_id = self.parent_id;
        while let Some(parent_id) = current_id {
            depth += 1;
            current_id = action_list
                .iter()
                .find(|a| a.id == parent_id)
                .and_then(|a| a.parent_id);
        }
        depth
    }

    /// Expand recurrence rule into a list of occurrence dates
    /// 
    /// Uses the do_date_time as DTSTART and the recurrence field as RRULE.
    /// Returns an empty vector if no recurrence is present or if parsing fails.
    /// 
    /// # Arguments
    /// * `limit` - Maximum number of occurrences to generate
    pub fn expand_occurrences(&self, limit: u16) -> Vec<DateTime<Tz>> {
        let recurrence = match &self.recurrence {
            Some(r) => r,
            None => return Vec::new(),
        };

        let dt_start = match self.do_date_time {
            Some(dt) => dt,
            None => return Vec::new(),
        };

        // Construct the full RRULE string: "DTSTART:...\nRRULE:..."
        // Note: rrule crate expects specific format. 
        // We need to convert Local DateTime to a specific timezone or UTC for rrule.
        // For simplicity in this CLI, we'll try to treat it as local/unspecified or UTC.
        // RFC 5545 requires DTSTART.
        
        let recurrence_str = recurrence.to_string();
        let clean_recurrence = if recurrence_str.starts_with("R:") {
            &recurrence_str[2..]
        } else {
            &recurrence_str
        };

        let rrule_str = format!("DTSTART:{}\nRRULE:{}", dt_start.format("%Y%m%dT%H%M%S"), clean_recurrence);
        
        match rrule_str.parse::<rrule::RRuleSet>() {
            Ok(rrule_set) => {
                rrule_set.all(limit).dates
            },
            Err(e) => {
                eprintln!("Failed to parse recurrence rule: {}", e);
                Vec::new()
            }
        }
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_content(f, true)
    }
}

impl TryFrom<TreeWrapper> for ActionList {
    type Error = &'static str;
    fn try_from(value: TreeWrapper) -> Result<Self, Self::Error> {
        let parsed: ParsedDocument = value.try_into()?;
        Ok(parsed.actions)
    }
}

impl TryFrom<TreeWrapper> for ParsedDocument {
    type Error = &'static str;
    fn try_from(value: TreeWrapper) -> Result<Self, Self::Error> {
        let root = value.tree.root_node();
        let mut action_list = Vec::new();
        let mut source_map = HashMap::new();
        let mut tag_index = HashMap::new();
        let mut cursor = root.walk();

        // Iterate through all root actions
        for root_action in root.children(&mut cursor) {
            if root_action.kind() == "root_action" {
                let wrapper = create_node_wrapper(root_action, value.source.clone());
                action_list.extend(parse_action_recursive(wrapper, None, &mut source_map, &mut tag_index)?);
            }
        }

        Ok(ParsedDocument {
            actions: action_list,
            source_map,
            tag_index,
        })
    }
}

/// Parse ISO 8601 datetime string to DateTime<Local>
/// Supports formats: YYYY-MM-DD, YYYY-MM-DDTHH:MM, YYYY-MM-DDTHH:MM:SS
/// with optional timezone (Z or +/-HH:MM)
pub fn parse_iso8601_datetime(datetime_str: &str) -> Option<DateTime<Local>> {
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone};

    let trimmed = datetime_str.trim();

    // Try parsing with timezone first
    if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.with_timezone(&Local));
    }

    // Try YYYY-MM-DDTHH:MM:SS format (without timezone)
    if let Ok(naive_dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return Some(Local.from_local_datetime(&naive_dt).earliest()?);
    }

    // Try YYYY-MM-DDTHH:MM format (without timezone, no seconds)
    if let Ok(naive_dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M") {
        return Some(Local.from_local_datetime(&naive_dt).earliest()?);
    }

    // Try YYYY-MM-DD format (date only, default to start of day)
    if let Ok(naive_date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let naive_dt = naive_date.and_time(NaiveTime::from_hms_opt(0, 0, 0)?);
        return Some(Local.from_local_datetime(&naive_dt).earliest()?);
    }

    None
}

/// Recursively parse an action node and all its children into a flat list
pub fn parse_action_recursive(
    node: NodeWrapper,
    parent_id: Option<Uuid>,
    source_map: &mut HashMap<Uuid, SourceMetadata>,
    tag_index: &mut HashMap<String, Vec<SourceRange>>,
) -> Result<Vec<Action>, &'static str> {
    let mut actions = Vec::new();

    let start_pos = node.node.start_position();
    let end_pos = node.node.end_position();
    let action_range = SourceRange {
        start_row: start_pos.row,
        start_col: start_pos.column,
        end_row: end_pos.row,
        end_col: end_pos.column,
    };

    // Parse state using field access
    let state_node = node
        .node
        .child_by_field_name("state")
        .ok_or("Missing state field")?;
    let state_value_node = state_node
        .child_by_field_name("value")
        .ok_or("Missing state value field")?;
    let state = match state_value_node.kind() {
        "state_not_started" => ActionState::NotStarted,
        "state_completed" => ActionState::Completed,
        "state_in_progress" => ActionState::InProgress,
        "state_blocked" => ActionState::BlockedorAwaiting,
        "state_cancelled" => ActionState::Cancelled,
        _ => return Err("Unknown state type"),
    };

    // Parse name using field access
    let name_node = node
        .node
        .child_by_field_name("name")
        .ok_or("Missing name field")?;
    let name = get_node_text(&name_node, &node.source).trim().to_string();

    let mut line_end_pos = name_node.end_position();

    // Parse metadata fields
    let mut description = None;
    let mut priority = None;
    let mut context_list = None;
    let mut id = None;
    let mut story = None;
    let mut do_date_time = None;
    let mut do_duration = None;
    let mut recurrence = None;
    let mut completed_date_time = None;
    let mut created_date_time = None;

    let mut do_date_range = None;
    let mut completed_date_range = None;
    let mut created_date_range = None;
    let mut raw_id = None;

    let mut metadata_cursor = node.node.walk();
    for metadata_node in node
        .node
        .children_by_field_name("metadata", &mut metadata_cursor)
    {
        if metadata_node.end_position().row > line_end_pos.row 
           || (metadata_node.end_position().row == line_end_pos.row && metadata_node.end_position().column > line_end_pos.column) 
        {
            line_end_pos = metadata_node.end_position();
        }

        match metadata_node.kind() {
            "description" => {
                // Get the text of the description node (skip the $ prefix)
                let desc_text = get_node_text(&metadata_node, &node.source);
                if desc_text.starts_with('$') {
                    description = Some(desc_text[1..].trim().to_string());
                }
            }
            "priority" => {
                // Get the text of the priority node (skip the ! prefix)
                let prio_text = get_node_text(&metadata_node, &node.source);
                if prio_text.starts_with('!') {
                    if let Ok(prio) = prio_text[1..].trim().parse::<u32>() {
                        priority = Some(prio);
                    }
                }
            }
            "story" => {
                // Get the text of the story node (skip the * prefix)
                let story_text = get_node_text(&metadata_node, &node.source);
                if story_text.starts_with('*') {
                    story = Some(story_text[1..].trim().to_string());
                    
                    // Index tag
                    let range = SourceRange {
                        start_row: metadata_node.start_position().row,
                        start_col: metadata_node.start_position().column,
                        end_row: metadata_node.end_position().row,
                        end_col: metadata_node.end_position().column,
                    };
                    tag_index.entry(story_text).or_default().push(range);
                }
            }
            "context" => {
                // Get the text of the context node (skip the + prefix)
                let context_text = get_node_text(&metadata_node, &node.source);
                if context_text.starts_with('+') {
                    let tags: Vec<String> = context_text[1..]
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                    if !tags.is_empty() {
                        context_list = Some(tags);
                    }

                    // Index tag (full text including +)
                    let range = SourceRange {
                        start_row: metadata_node.start_position().row,
                        start_col: metadata_node.start_position().column,
                        end_row: metadata_node.end_position().row,
                        end_col: metadata_node.end_position().column,
                    };
                    tag_index.entry(context_text).or_default().push(range);
                }
            }
            "do_date" => {
                if let Some(datetime_node) = metadata_node.child_by_field_name("datetime") {
                    let datetime_str = get_node_text(&datetime_node, &node.source);
                    do_date_time = parse_iso8601_datetime(&datetime_str);
                    
                    let start = metadata_node.start_position();
                    let end = metadata_node.end_position();
                    do_date_range = Some(SourceRange {
                        start_row: start.row,
                        start_col: start.column,
                        end_row: end.row,
                        end_col: end.column,
                    });
                }

                // Parse duration
                if let Some(duration_node) = metadata_node.child_by_field_name("duration") {
                    if let Some(minutes_node) = duration_node.child_by_field_name("minutes") {
                        let minutes_str = get_node_text(&minutes_node, &node.source);
                        if let Ok(minutes) = minutes_str.parse::<u32>() {
                            do_duration = Some(minutes);
                        }
                    }
                }

                // Parse recurrence
                if let Some(recurrence_node) = metadata_node.child_by_field_name("recurrence") {
                    if let Some(rrule_node) = recurrence_node.child_by_field_name("rrule") {
                        let rrule_str = get_node_text(&rrule_node, &node.source);
                        recurrence = parse_rrule(&rrule_str);
                    }
                }
            }
            "completed_date" => {
                if let Some(datetime_node) = metadata_node.child_by_field_name("datetime") {
                    let datetime_str = get_node_text(&datetime_node, &node.source);
                    completed_date_time = parse_iso8601_datetime(&datetime_str);

                    let start = metadata_node.start_position();
                    let end = metadata_node.end_position();
                    completed_date_range = Some(SourceRange {
                        start_row: start.row,
                        start_col: start.column,
                        end_row: end.row,
                        end_col: end.column,
                    });
                }
            }
            "created_date" => {
                if let Some(datetime_node) = metadata_node.child_by_field_name("datetime") {
                    let datetime_str = get_node_text(&datetime_node, &node.source);
                    created_date_time = parse_iso8601_datetime(&datetime_str);

                    let start = metadata_node.start_position();
                    let end = metadata_node.end_position();
                    created_date_range = Some(SourceRange {
                        start_row: start.row,
                        start_col: start.column,
                        end_row: end.row,
                        end_col: end.column,
                    });
                }
            }
            "id" => {
                // Get the text of the id node (skip the # prefix)
                let id_text = get_node_text(&metadata_node, &node.source);
                if id_text.starts_with('#') {
                    let id_val = id_text[1..].trim();
                    if let Ok(uuid) = Uuid::parse_str(id_val) {
                        id = Some(uuid);
                    } else {
                        raw_id = Some(id_val.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    let is_id_generated = id.is_none();
    // Generate ID if not present
    let action_id = id.unwrap_or_else(|| Uuid::new_v4());

    // Record metadata
    source_map.insert(action_id, SourceMetadata {
        root: action_range,
        line_range: SourceRange {
            start_row: action_range.start_row,
            start_col: action_range.start_col,
            end_row: line_end_pos.row,
            end_col: line_end_pos.column,
        },
        do_date: do_date_range,
        completed_date: completed_date_range,
        created_date: created_date_range,
        is_id_generated,
        raw_id,
    });

    // Create the action
    actions.push(Action {
        id: action_id,
        parent_id,
        state,
        name,
        description,
        priority,
        context_list,
        do_date_time,
        do_duration,
        recurrence,
        completed_date_time,
        created_date_time,
        story,
    });

    // Recursively parse children using field access
    let mut child_cursor = node.node.walk();
    for child_node in node.node.children_by_field_name("child", &mut child_cursor) {
        let child_wrapper = create_node_wrapper(child_node, node.source.clone());
        actions.extend(parse_action_recursive(child_wrapper, Some(action_id), source_map, tag_index)?);
    }

    Ok(actions)
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Reconcile, Hydrate)]
#[serde(rename_all = "snake_case")]
pub enum ActionState {
    #[default]
    NotStarted,
    Completed,
    InProgress,
    #[serde(rename = "blocked")]
    BlockedorAwaiting,
    Cancelled,
}

impl fmt::Display for ActionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state_char = match self {
            ActionState::NotStarted => " ",
            ActionState::Completed => "x",
            ActionState::InProgress => "-",
            ActionState::BlockedorAwaiting => "=",
            ActionState::Cancelled => "_",
        };
        write!(f, "{}", state_char)
    }
}

fn parse_rrule(rrule_str: &str) -> Option<Recurrence> {
    let mut frequency = String::new();
    let mut interval = None;
    let mut count = None;
    let mut until = None;
    let mut by_second = None;
    let mut by_minute = None;
    let mut by_hour = None;
    let mut by_day = None;
    let mut by_month_day = None;
    let mut by_year_day = None;
    let mut by_week_no = None;
    let mut by_month = None;
    let mut by_set_pos = None;
    let mut week_start = None;

    // Remove "R:" prefix if present (it should be handled by tree-sitter but just in case)
    let clean_rrule = if rrule_str.trim().starts_with("R:") {
        &rrule_str.trim()[2..]
    } else {
        rrule_str.trim()
    };

    for part in clean_rrule.split(';') {
        let mut kv = part.splitn(2, '=');
        let key = kv.next()?;
        let value = kv.next()?;

        match key {
            "FREQ" => frequency = value.to_lowercase(),
            "INTERVAL" => interval = value.parse().ok(),
            "COUNT" => count = value.parse().ok(),
            "UNTIL" => until = Some(value.to_string()),
            "BYSECOND" => by_second = Some(parse_int_list(value)),
            "BYMINUTE" => by_minute = Some(parse_int_list(value)),
            "BYHOUR" => by_hour = Some(parse_int_list(value)),
            "BYDAY" => by_day = Some(value.split(',').map(|s| s.to_string()).collect()),
            "BYMONTHDAY" => by_month_day = Some(parse_int_list(value)),
            "BYYEARDAY" => by_year_day = Some(parse_int_list(value)),
            "BYWEEKNO" => by_week_no = Some(parse_int_list(value)),
            "BYMONTH" => by_month = Some(parse_int_list(value)),
            "BYSETPOS" => by_set_pos = Some(parse_int_list(value)),
            "WKST" => week_start = Some(value.to_string()),
            _ => {}
        }
    }

    if frequency.is_empty() {
        return None;
    }

    Some(Recurrence {
        frequency,
        interval,
        count,
        until,
        by_second,
        by_minute,
        by_hour,
        by_day,
        by_month_day,
        by_year_day,
        by_week_no,
        by_month,
        by_set_pos,
        week_start,
    })
}

fn parse_int_list<T: std::str::FromStr>(s: &str) -> Vec<T> {
    s.split(',')
        .filter_map(|x| x.parse().ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_actions(source: &str) -> ActionList {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_actions::LANGUAGE.into())
            .expect("Failed to set language");

        let tree = parser.parse(source, None).expect("Failed to parse");
        let tree_wrapper = TreeWrapper {
            tree,
            source: source.to_string(),
        };

        tree_wrapper.try_into().expect("Failed to convert to ActionList")
    }

    #[test]
    fn test_parse_simple_action() {
        let source = "[ ] Buy milk";
        let actions = parse_actions(source);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Buy milk");
        assert_eq!(actions[0].state, ActionState::NotStarted);
        assert_eq!(actions[0].parent_id, None);
    }

    #[test]
    fn test_parse_with_metadata() {
        let source = "[x] Buy groceries $from the store !1 +shopping";
        let actions = parse_actions(source);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].name, "Buy groceries");
        assert_eq!(actions[0].state, ActionState::Completed);
        assert_eq!(actions[0].description.as_ref().map(|s| s.trim()), Some("from the store"));
        assert_eq!(actions[0].priority, Some(1));
        assert!(actions[0].context_list.is_some());
    }

    #[test]
    fn test_parse_with_children() {
        let source = "[ ] Parent action\n> [ ] Child action\n>> [ ] Grandchild action";
        let actions = parse_actions(source);

        assert_eq!(actions.len(), 3);

        // Check parent
        assert_eq!(actions[0].name, "Parent action");
        assert_eq!(actions[0].parent_id, None);

        // Check child
        assert_eq!(actions[1].name, "Child action");
        assert_eq!(actions[1].parent_id, Some(actions[0].id));

        // Check grandchild
        assert_eq!(actions[2].name, "Grandchild action");
        assert_eq!(actions[2].parent_id, Some(actions[1].id));
    }

    #[test]
    fn test_parse_recurrence_and_duration() {
        let source = "[ ] Recurring Task @2025-01-20T14:00 D60 R:FREQ=WEEKLY;BYDAY=MO;INTERVAL=2";
        let actions = parse_actions(source);

        assert_eq!(actions.len(), 1);
        let action = &actions[0];

        assert_eq!(action.name, "Recurring Task");
        assert!(action.do_date_time.is_some());
        assert_eq!(action.do_duration, Some(60));

        let recurrence = action.recurrence.as_ref().expect("Recurrence should be present");
        assert_eq!(recurrence.frequency, "weekly");
        assert_eq!(recurrence.by_day, Some(vec!["MO".to_string()]));
        assert_eq!(recurrence.interval, Some(2));
    }

    #[test]
    fn test_format_recurrence_and_duration() {
        let recurrence = Recurrence {
            frequency: "weekly".to_string(),
            interval: Some(2),
            count: None,
            until: None,
            by_second: None,
            by_minute: None,
            by_hour: None,
            by_day: Some(vec!["MO".to_string()]),
            by_month_day: None,
            by_year_day: None,
            by_week_no: None,
            by_month: None,
            by_set_pos: None,
            week_start: None,
        };

        let action = Action {
            id: Uuid::new_v4(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Recurring".to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: Some(Local::now()),
            do_duration: Some(60),
            recurrence: Some(recurrence),
            completed_date_time: None,
            created_date_time: None,
            story: None,
        };
        
        let formatted = format!("{}", action);
        assert!(formatted.contains("D60"));
        assert!(formatted.contains("R:FREQ=WEEKLY"));
        assert!(formatted.contains(";INTERVAL=2"));
        assert!(formatted.contains(";BYDAY=MO"));
    }

    #[test]
    fn test_expand_occurrences() {
        use chrono::TimeZone;
        
        // Create a fixed start date: 2025-01-01 09:00:00 (Wednesday)
        let dt_start = Local.with_ymd_and_hms(2025, 1, 1, 9, 0, 0).unwrap();
        
        let recurrence = Recurrence {
            frequency: "daily".to_string(),
            interval: Some(1),
            count: Some(3), // Expect 3 occurrences
            until: None,
            by_second: None,
            by_minute: None,
            by_hour: None,
            by_day: None,
            by_month_day: None,
            by_year_day: None,
            by_week_no: None,
            by_month: None,
            by_set_pos: None,
            week_start: None,
        };

        let action = Action {
            id: Uuid::new_v4(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Daily Standup".to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: Some(dt_start),
            do_duration: None,
            recurrence: Some(recurrence),
            completed_date_time: None,
            created_date_time: None,
            story: None,
        };

        let occurrences = action.expand_occurrences(10);
        
        assert_eq!(occurrences.len(), 3);
        
        // Verify dates (ignoring time zone specifics for this simple test, just checking sequence)
        // Note: rrule returns DateTime<Tz>, we can format to check
        assert_eq!(occurrences[0].format("%Y-%m-%d").to_string(), "2025-01-01");
        assert_eq!(occurrences[1].format("%Y-%m-%d").to_string(), "2025-01-02");
        assert_eq!(occurrences[2].format("%Y-%m-%d").to_string(), "2025-01-03");
    }
}
