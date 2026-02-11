use chrono::{DateTime, Duration, Local};
use clearhead_cli::{Action, ActionList, ActionState};
use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};
use tracing::{debug, info};

use crate::commands::CommandContext;

pub struct AgendaItem<'a> {
    pub datetime: DateTime<Local>,
    pub action: &'a Action,
}

/// CLI handler: read plans and display agenda table.
pub fn run_agenda(
    ctx: &CommandContext,
    file: &Option<std::path::PathBuf>,
    days: u32,
) -> Result<(), String> {
    let input_file = ctx.resolve_action_file(file.as_ref());
    debug!(input_file = %input_file.display(), days = days, "Executing Read Agenda");

    let content = crate::commands::read_input(Some(&input_file))?;
    let all_actions =
        clearhead_cli::get_action_list_struct(&serde_json::json!({}), &content)?;

    let agenda_items = project_agenda(&all_actions, days);

    info!(item_count = agenda_items.len(), "Projected agenda items");

    if agenda_items.is_empty() {
        println!("No actions scheduled for the next {} days.", days);
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("Date").fg(Color::Cyan),
        Cell::new("Time").fg(Color::Cyan),
        Cell::new("Action").fg(Color::Cyan),
        Cell::new("Context").fg(Color::Cyan),
        Cell::new("Description").fg(Color::Cyan),
    ]);

    for item in agenda_items {
        let date_str = item.datetime.format("%Y-%m-%d (%a)").to_string();
        let time_str = item.datetime.format("%H:%M").to_string();
        let name = item.action.name.clone();
        let contexts = item
            .action
            .context_list
            .as_ref()
            .map(|c| c.join(", "))
            .unwrap_or_else(|| "-".to_string());
        let desc = item
            .action
            .description
            .as_ref()
            .map(|d| d.to_string())
            .unwrap_or_else(|| "-".to_string());

        table.add_row(vec![
            Cell::new(date_str),
            Cell::new(time_str),
            Cell::new(name),
            Cell::new(contexts),
            Cell::new(desc),
        ]);
    }

    println!("Agenda for the next {} days:", days);
    println!("{}", table);

    Ok(())
}

/// Project recurring and one-shot actions into a flat agenda over `days` days.
pub fn project_agenda<'a>(actions: &'a ActionList, days: u32) -> Vec<AgendaItem<'a>> {
    let now = Local::now();
    let start_of_day = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_local_timezone(Local)
        .unwrap();
    let end_date = start_of_day + Duration::days(days as i64);

    // First pass: identify completed instances by (name, date)
    let mut completed_instances = std::collections::HashSet::new();
    for action in actions {
        if action.state == ActionState::Completed {
            if let Some(do_dt) = action.do_date_time {
                completed_instances.insert((action.name.clone(), do_dt.date_naive()));
            }
        }
    }

    let mut items = Vec::new();

    for action in actions {
        // Skip non-recurring completed entries
        if action.state == ActionState::Completed && action.recurrence.is_none() {
            continue;
        }

        if action.recurrence.is_some() {
            let occurrences = action.expand_occurrences(100);
            for occ in occurrences {
                let occ_local = occ.with_timezone(&Local);
                if occ_local >= start_of_day && occ_local <= end_date {
                    if !completed_instances.contains(&(action.name.clone(), occ_local.date_naive()))
                    {
                        items.push(AgendaItem {
                            datetime: occ_local,
                            action,
                        });
                    }
                }
            }
        } else if let Some(do_dt) = action.do_date_time {
            if do_dt >= start_of_day && do_dt <= end_date {
                items.push(AgendaItem {
                    datetime: do_dt,
                    action,
                });
            }
        }
    }

    items.sort_by_key(|item| item.datetime);
    items
}
