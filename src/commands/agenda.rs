use chrono::{DateTime, Duration, Local};
use clearhead_cli::{Action, ActionList, ActionState};

pub struct AgendaItem<'a> {
    pub datetime: DateTime<Local>,
    pub action: &'a Action,
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
