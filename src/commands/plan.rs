use std::fs;
use std::path::{Path, PathBuf};
use chrono::{DateTime, Local, Utc};
use icalendar::{Calendar, Component, Event, EventLike};
use tracing::{debug, info};

use crate::argparser;
use crate::commands::{
    CommandContext, load_file_for_read, parse_content_for_read, read_input, try_emit,
};
use clearhead_cli::telemetry::TelemetryEvent;

/// Find the index at which to insert a child action so it appears immediately
/// after the last existing descendant of `parent_id`.
///
/// Walks forward from the parent's position, collecting all actions whose
/// ancestor chain leads back to `parent_id`. Returns the index after the last
/// one, or `actions.len()` if the parent is not found.
#[cfg(test)]
fn insert_index_after_descendants(actions: &[clearhead_cli::Action], parent_id: uuid::Uuid) -> usize {
    let parent_idx = match actions.iter().position(|a| a.id == parent_id) {
        Some(idx) => idx,
        None => return actions.len(),
    };

    let mut descendant_ids: std::collections::HashSet<uuid::Uuid> =
        std::collections::HashSet::from([parent_id]);
    let mut last = parent_idx;

    for (offset, action) in actions[parent_idx + 1..].iter().enumerate() {
        if action.parent_id.map_or(false, |pid| descendant_ids.contains(&pid)) {
            descendant_ids.insert(action.id);
            last = parent_idx + 1 + offset;
        }
    }

    last + 1
}

/// The canonical machine key for a charter — alias if present, otherwise title.
///
/// `charter.parent` always stores a machine key, so this is the right value to
/// use for any identity comparison or graph edge.
fn charter_key(charter: &clearhead_core::Charter) -> &str {
    charter.alias.as_deref().unwrap_or(&charter.title)
}

/// Returns the key name used in the workspace graph for a charter (owned).
fn charter_graph_name(charter: &clearhead_core::Charter) -> String {
    charter_key(charter).to_string()
}

/// All charters whose `parent` field matches `parent_key` (case-insensitive).
fn direct_children<'a>(
    charters: &'a [clearhead_core::Charter],
    parent_key: &str,
) -> Vec<&'a clearhead_core::Charter> {
    charters
        .iter()
        .filter(|c| {
            c.parent
                .as_deref()
                .map(|p| p.eq_ignore_ascii_case(parent_key))
                .unwrap_or(false)
        })
        .collect()
}

/// Collect the charter matching `root_key` plus all descendants (transitively).
///
/// Uses machine keys throughout — `charter.parent` always stores a machine key,
/// never a display title. A `visited` set guards against cyclic parent data.
fn collect_charter_tree(
    charters: &[clearhead_core::Charter],
    root_key: &str,
) -> Vec<clearhead_core::Charter> {
    let mut result = Vec::new();
    let mut queue = vec![root_key.to_string()];
    let mut visited = std::collections::HashSet::new();

    while let Some(current) = queue.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        if let Some(node) = charters
            .iter()
            .find(|c| charter_key(c).eq_ignore_ascii_case(&current))
        {
            result.push(node.clone());
        }
        for child in direct_children(charters, &current) {
            queue.push(charter_key(child).to_string());
        }
    }
    result
}

pub fn read_plans(
    ctx: &CommandContext,
    _format: &Option<argparser::Format>,
    charter: &Option<String>,
    recursive: bool,
    file: &Option<std::path::PathBuf>,
    _stdio: bool,
    _table_options: &argparser::CliTableOptions,
) -> Result<(), String> {
    use clearhead_core::workspace::ics::parse_ics_file;
    use clearhead_core::workspace::plans::collect_plan_files;
    use comfy_table::{Cell, Table};

    let plans: Vec<(String, clearhead_core::Plan)> = if let Some(path) = file {
        let charter_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        parse_ics_file(path)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|p| (charter_name.clone(), p))
            .collect()
    } else {
        let entries = collect_plan_files(&ctx.data_dir).map_err(|e| e.to_string())?;

        let allowed: Option<std::collections::HashSet<String>> = if let Some(query) = charter {
            let model =
                clearhead_core::load_domain_model(&ctx.data_dir).map_err(|e| e.to_string())?;
            let found = crate::commands::charter::resolve_charter(&model.charters, query)
                .ok_or_else(|| format!("No charter found matching '{}'", query))?;
            let key = charter_graph_name(found);
            let names = if recursive {
                collect_charter_tree(&model.charters, &key)
                    .iter()
                    .map(|c| charter_key(c).to_lowercase())
                    .collect()
            } else {
                std::iter::once(key.to_lowercase()).collect()
            };
            Some(names)
        } else {
            None
        };

        let mut result = Vec::new();
        for entry in entries {
            if let Some(ref allowed) = allowed {
                if !allowed.contains(&entry.charter_name.to_lowercase()) {
                    continue;
                }
            }
            match parse_ics_file(&entry.path) {
                Ok(ps) => result.extend(ps.into_iter().map(|p| (entry.charter_name.clone(), p))),
                Err(e) => eprintln!("Warning: skipping {}: {}", entry.path.display(), e),
            }
        }
        result
    };

    if plans.is_empty() {
        if let Some(query) = charter {
            println!("No plans found for charter '{}'.", query);
        } else {
            println!("No ICS plan files found in workspace.");
        }
        return Ok(());
    }

    let mut table = Table::new();
    table.set_header(vec!["name", "charter", "dtstart", "recurrence"]);
    for (charter_name, plan) in &plans {
        let dtstart = plan
            .dtstart
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "—".to_string());
        let recurrence = plan
            .recurrence
            .as_ref()
            .map(|r| r.frequency.to_lowercase())
            .unwrap_or_else(|| "—".to_string());
        table.add_row(vec![
            Cell::new(&plan.name),
            Cell::new(charter_name),
            Cell::new(&dtstart),
            Cell::new(&recurrence),
        ]);
    }
    println!("{}", table);
    Ok(())
}

pub fn show_plan(
    ctx: &CommandContext,
    query: &str,
    file: &Option<std::path::PathBuf>,
    _format: &Option<argparser::Format>,
    _table_options: &argparser::CliTableOptions,
) -> Result<(), String> {
    use clearhead_core::workspace::ics::parse_ics_file;
    use clearhead_core::workspace::plans::collect_plan_files;

    debug!(query = %query, "Executing Show Plan");

    let query_lower = query.to_lowercase();

    let candidates: Vec<(String, clearhead_core::Plan)> = if let Some(path) = file {
        let charter_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        parse_ics_file(path)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|p| (charter_name.clone(), p))
            .collect()
    } else {
        let entries = collect_plan_files(&ctx.data_dir).map_err(|e| e.to_string())?;
        let mut result = Vec::new();
        for entry in entries {
            match parse_ics_file(&entry.path) {
                Ok(ps) => result.extend(ps.into_iter().map(|p| (entry.charter_name.clone(), p))),
                Err(e) => eprintln!("Warning: skipping {}: {}", entry.path.display(), e),
            }
        }
        result
    };

    let (charter_name, plan) = candidates
        .into_iter()
        .find(|(_, p)| {
            p.name.to_lowercase().contains(&query_lower)
                || p.external_id
                    .as_deref()
                    .map(|uid| uid.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
                || p.id.to_string().starts_with(&query_lower)
        })
        .ok_or_else(|| format!("No plan found matching '{}'", query))?;

    println!("{}", plan.name);
    println!("{}", "=".repeat(plan.name.len()));
    println!("charter:    {}", charter_name);
    if let Some(dt) = plan.dtstart {
        println!("dtstart:    {}", dt.format("%Y-%m-%d %H:%M"));
    }
    if let Some(r) = &plan.recurrence {
        println!("recurrence: {}", r.frequency.to_lowercase());
    }
    if let Some(tmpl) = &plan.template_name {
        println!("template:   {}", tmpl);
    }
    if let Some(desc) = &plan.description {
        println!("description:\n  {}", desc.replace('\n', "\n  "));
    }
    if let Some(uid) = &plan.external_id {
        println!("uid:        {}", uid);
    }

    Ok(())
}

fn resolve_plan_file(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    charter: &Option<String>,
) -> Result<PathBuf, String> {
    if let Some(path) = file {
        return Ok(ctx.resolve_action_file(Some(path)));
    }

    if let Some(query) = charter {
        let charters = clearhead_core::load_markdown_charters(&ctx.data_dir)
            .map_err(|e| e.to_string())?;
        let charter = resolve_markdown_charter(&charters, query)
            .ok_or_else(|| format!("No charter found matching '{}'", query))?;

        if let Some(path) = &charter.ics_file {
            return Ok(ctx.data_dir.join(path));
        }
        if let Some(path) = &charter.md_file {
            return Ok(ctx.data_dir.join(path).with_extension("ics"));
        }

        let key = charter.alias.as_deref().unwrap_or(&charter.title);
        return Ok(ctx.data_dir.join(format!("{}.ics", slug(key))));
    }

    Ok(ctx.resolve_action_file(None).with_extension("ics"))
}

fn load_plan_file(path: &Path) -> Result<Vec<clearhead_core::Plan>, String> {
    if path.exists() {
        clearhead_core::workspace::ics::parse_ics_file(path).map_err(|e| e.to_string())
    } else {
        Ok(Vec::new())
    }
}

fn save_plan_file(path: &Path, plans: &[clearhead_core::Plan]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    fs::write(path, format_plans_as_ics(plans))
        .map_err(|e| format!("Failed to write plan file '{}': {}", path.display(), e))
}

fn format_plans_as_ics(plans: &[clearhead_core::Plan]) -> String {
    let mut calendar = Calendar::new()
        .name("ClearHead Plans")
        .description("Schedules managed by ClearHead")
        .done();

    for plan in plans {
        calendar.push(plan_to_event(plan));
    }

    calendar.to_string()
}

fn plan_to_event(plan: &clearhead_core::Plan) -> Event {
    let mut event = Event::new();
    let uid = plan.external_id.clone().unwrap_or_else(|| plan.id.to_string());
    event.uid(&uid);
    event.summary(&plan.name);

    if let Some(dtstart) = plan.dtstart {
        event.starts(dtstart.with_timezone(&Utc));
    }
    if let Some(priority) = plan.priority {
        event.priority(priority);
    }
    if let Some(contexts) = &plan.contexts {
        if !contexts.is_empty() {
            event.add_property("CATEGORIES", &contexts.join(","));
        }
    }
    if let Some(alias) = &plan.alias {
        event.add_property("X-CLEARHEAD-ALIAS", alias);
    }
    if let Some(recurrence) = &plan.recurrence {
        let rrule = recurrence.to_string();
        event.add_property("RRULE", rrule.strip_prefix("R:").unwrap_or(&rrule));
    }

    let mut description = Vec::new();
    if let Some(template) = &plan.template_name {
        description.push(format!("template: {}", template));
    }
    if let Some(text) = &plan.description {
        description.push(text.clone());
    }
    if !description.is_empty() {
        event.description(&description.join("\n"));
    }

    event
}

fn parse_local_datetime(value: Option<&str>) -> Result<Option<DateTime<Local>>, String> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|dt| dt.with_timezone(&Local))
                .map_err(|e| format!("Invalid --scheduled-at '{}': {}", value, e))
        })
        .transpose()
}

fn parse_rrule(value: Option<&str>) -> Result<Option<clearhead_core::Recurrence>, String> {
    value
        .map(|value| {
            clearhead_core::Recurrence::from_rrule_str(value)
                .ok_or_else(|| format!("Invalid --rrule '{}': expected RFC5545 RRULE fields", value))
        })
        .transpose()
}

fn reject_act_only_plan_fields(fields: &argparser::ActionFields) -> Result<(), String> {
    if fields.state.is_some() {
        return Err("Plan state is stored on planned acts; use `update act --state` once act state editing exists".to_string());
    }
    Ok(())
}

fn find_plan_for_mutation(
    ctx: &CommandContext,
    file: &Option<PathBuf>,
    query: &str,
) -> Result<(PathBuf, Vec<clearhead_core::Plan>, usize), String> {
    let files = if let Some(path) = file {
        vec![ctx.resolve_action_file(Some(path))]
    } else {
        clearhead_core::workspace::plans::collect_plan_files(&ctx.data_dir)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|entry| entry.path)
            .collect()
    };

    for path in files {
        let plans = load_plan_file(&path)?;
        if let Some(idx) = plans.iter().position(|plan| plan_matches(plan, query)) {
            return Ok((path, plans, idx));
        }
    }

    Err(format!("No plan found matching '{}'", query))
}

fn plan_matches(plan: &clearhead_core::Plan, query: &str) -> bool {
    let query_lower = query.to_lowercase();
    let id = plan.id.to_string();
    id == query || id.starts_with(query)
        || plan
            .external_id
            .as_deref()
            .map(|uid| uid.eq_ignore_ascii_case(query) || uid.to_lowercase().contains(&query_lower))
            .unwrap_or(false)
        || plan
            .alias
            .as_deref()
            .map(|alias| alias.eq_ignore_ascii_case(query))
            .unwrap_or(false)
        || plan.name.to_lowercase().contains(&query_lower)
}

fn resolve_markdown_charter<'a>(
    charters: &'a [clearhead_core::MarkdownCharter],
    query: &str,
) -> Option<&'a clearhead_core::MarkdownCharter> {
    let query_lower = query.to_lowercase();
    if let Ok(uuid) = uuid::Uuid::parse_str(query) {
        if let Some(c) = charters.iter().find(|c| c.id == uuid) {
            return Some(c);
        }
    }
    if query.len() >= 4 && query.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Some(c) = charters.iter().find(|c| c.id.to_string().starts_with(query)) {
            return Some(c);
        }
    }
    if let Some(c) = charters.iter().find(|c| {
        c.alias
            .as_deref()
            .map(|alias| alias.eq_ignore_ascii_case(query))
            .unwrap_or(false)
    }) {
        return Some(c);
    }
    charters.iter().find(|c| c.title.to_lowercase().contains(&query_lower))
}

fn slug(value: &str) -> String {
    value.to_lowercase().replace(' ', "-").replace('&', "and")
}

pub fn add_plan(
    ctx: &CommandContext,
    name: &str,
    file: &Option<PathBuf>,
    charter: &Option<String>,
    parent: &Option<String>,
    fields: &argparser::ActionFields,
    schedule: &argparser::PlanScheduleFields,
    dry_run: bool,
) -> Result<(), String> {
    reject_act_only_plan_fields(fields)?;
    if parent.is_some() {
        return Err("Plan hierarchy in ICS files is not implemented yet".to_string());
    }

    let input_file = resolve_plan_file(ctx, file, charter)?;
    debug!(name = %name, input_file = %input_file.display(), dry_run = dry_run, "Executing Add Plan");

    let uid = uuid::Uuid::now_v7().to_string();
    let new_id = clearhead_core::workspace::ics::plan_id_from_ics_uid(&uid);
    let new_plan = clearhead_core::Plan {
        id: new_id,
        name: name.to_string(),
        description: fields.description.clone(),
        priority: fields.priority,
        contexts: (!fields.context.is_empty()).then(|| fields.context.clone()),
        alias: fields.alias.clone(),
        recurrence: parse_rrule(schedule.rrule.as_deref())?,
        dtstart: parse_local_datetime(schedule.scheduled_at.as_deref())?,
        external_id: Some(uid),
        template_name: schedule.template.clone(),
        ..Default::default()
    };

    let mut plans = load_plan_file(&input_file)?;
    plans.push(new_plan.clone());

    if dry_run {
        println!("{}", format_plans_as_ics(&[new_plan]));
    } else {
        save_plan_file(&input_file, &plans)?;

        try_emit(
            &new_id,
            TelemetryEvent::ActionCreated {
                name: name.to_string(),
                file_path: input_file.display().to_string(),
            },
        );

        info!(name = %name, id = %new_id, "Plan added successfully");
        println!("{}", new_id);
    }
    Ok(())
}

pub fn update_plan(
    ctx: &CommandContext,
    query: &str,
    file: &Option<PathBuf>,
    name: &Option<String>,
    fields: &argparser::ActionFields,
    schedule: &argparser::PlanScheduleFields,
    dry_run: bool,
) -> Result<(), String> {
    reject_act_only_plan_fields(fields)?;

    let (input_file, mut plans, idx) = find_plan_for_mutation(ctx, file, query)?;
    debug!(query = %query, input_file = %input_file.display(), dry_run = dry_run, "Executing Update Plan");

    if let Some(name) = name {
        plans[idx].name = name.clone();
    }
    if let Some(priority) = fields.priority {
        plans[idx].priority = Some(priority);
    }
    if !fields.context.is_empty() {
        plans[idx].contexts = Some(fields.context.clone());
    }
    if let Some(description) = &fields.description {
        plans[idx].description = Some(description.clone());
    }
    if let Some(alias) = &fields.alias {
        plans[idx].alias = Some(alias.clone());
    }
    if schedule.scheduled_at.is_some() {
        plans[idx].dtstart = parse_local_datetime(schedule.scheduled_at.as_deref())?;
    }
    if schedule.rrule.is_some() {
        plans[idx].recurrence = parse_rrule(schedule.rrule.as_deref())?;
    }
    if let Some(template) = &schedule.template {
        plans[idx].template_name = Some(template.clone());
    }

    let updated = plans[idx].clone();

    if dry_run {
        println!("{}", format_plans_as_ics(&[updated]));
    } else {
        save_plan_file(&input_file, &plans)?;
        info!(name = %updated.name, id = %updated.id, "Plan updated successfully");
    }
    Ok(())
}

pub fn complete_plan(
    ctx: &CommandContext,
    query: &str,
    file: &Option<std::path::PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let _ = (ctx, query, file, dry_run);
    Err("Plans are schedules and do not have completion state; use `complete act` for planned acts".to_string())
}

pub fn delete_plan(
    ctx: &CommandContext,
    query: &str,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let (input_file, mut plans, idx) = find_plan_for_mutation(ctx, file, query)?;
    debug!(query = %query, input_file = %input_file.display(), dry_run = dry_run, "Executing Delete Plan");

    let plan = plans.remove(idx);

    if dry_run {
        println!("{}", format_plans_as_ics(&[plan]));
    } else {
        save_plan_file(&input_file, &plans)?;

        try_emit(
            &plan.id,
            TelemetryEvent::ActionDeleted {
                name: plan.name.clone(),
            },
        );

        info!(name = %plan.name, id = %plan.id, "Plan deleted successfully");
    }
    Ok(())
}

pub fn archive_plans(
    ctx: &CommandContext,
    scope: &Option<String>,
    file: &Option<PathBuf>,
    dry_run: bool,
) -> Result<(), String> {
    let _ = (ctx, scope, file, dry_run);
    Err(
        "Plan archival is not implemented yet: plans are schedules in .ics files, and recurring schedule completion needs separate lifecycle semantics. Use `archive acts` to move completed/cancelled planned acts.".to_string(),
    )
}

pub fn export_plans(
    ctx: &CommandContext,
    reference: &Option<String>,
    output: &Option<std::path::PathBuf>,
    open_only: bool,
    recursive: bool,
) -> Result<(), String> {
    use crate::environment_reader::resolve_file_path;
    use clearhead_core::reference::{
        filter_model_for_act, filter_model_for_charter, filter_model_for_plan, resolve_reference,
        ReferenceOptions, ReferenceTarget,
    };

    debug!(reference = ?reference, output = ?output, open_only = open_only, recursive = recursive, "Executing Export Plans");

    let model = if let Some(reference) = reference {
        if reference == "-" {
            let content = read_input(None)?;
            let actions = parse_content_for_read(&content, "stdin", "export plans")?;
            let charter = clearhead_core::workspace::actions::convert::from_actions_with_charter(
                &actions,
                "stdin".to_string(),
            );
            clearhead_core::DomainModel {
                objectives: vec![],
                charters: vec![charter],
            }
        } else if reference.ends_with(".actions") {
            let resolved = resolve_file_path(reference, &ctx.data_dir);
            let actions = load_file_for_read(&resolved, "export plans")?;
            let relative = resolved.strip_prefix(&ctx.data_dir).unwrap_or(&resolved);
            let charter_name = clearhead_core::infer_charter_name(relative)
                .unwrap_or_else(|| "unknown".to_string());
            let charter = clearhead_core::workspace::actions::convert::from_actions_with_charter(
                &actions,
                charter_name,
            );
            let model = clearhead_core::DomainModel {
                objectives: vec![],
                charters: vec![charter],
            };

            model
        } else {
            let model = clearhead_cli::load_workspace_domain_model(&ctx.data_dir)?;
            let target = resolve_reference(&model, reference, &ReferenceOptions::default())
                .map_err(|e| e.to_string())?;
            match target {
                ReferenceTarget::Charter(id) => filter_model_for_charter(&model, id, recursive),
                ReferenceTarget::Plan(id) => filter_model_for_plan(&model, id),
                ReferenceTarget::Act(id) => filter_model_for_act(&model, id),
            }
        }
    } else {
        clearhead_cli::load_workspace_domain_model(&ctx.data_dir)?
    };

    let icalendar = clearhead_cli::format_as_icalendar(&model, open_only)?;

    if let Some(output_path) = output {
        info!(output_path = %output_path.display(), "Writing iCalendar export to file");
        fs::write(output_path, icalendar).map_err(|e| format!("Failed to write to file: {}", e))?;
    } else {
        println!("{}", icalendar);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearhead_cli::Action;
    use uuid::Uuid;

    fn action(id: Uuid, parent_id: Option<Uuid>) -> Action {
        Action { id, parent_id, name: id.to_string(), ..Default::default() }
    }

    #[test]
    fn insert_after_last_descendant_with_no_children() {
        let parent = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        // [parent, sibling] — sibling is not a descendant of parent
        let actions = vec![action(parent, None), action(sibling, None)];
        // child should go at index 1 (immediately after parent, before sibling)
        assert_eq!(insert_index_after_descendants(&actions, parent), 1);
    }

    #[test]
    fn insert_after_last_descendant_skips_existing_children() {
        let parent = Uuid::new_v4();
        let child = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        // [parent, child, sibling]
        let actions = vec![
            action(parent, None),
            action(child, Some(parent)),
            action(sibling, None),
        ];
        // new child should go at index 2 (after existing child, before sibling)
        assert_eq!(insert_index_after_descendants(&actions, parent), 2);
    }

    #[test]
    fn insert_after_last_descendant_handles_grandchildren() {
        let parent = Uuid::new_v4();
        let child = Uuid::new_v4();
        let grandchild = Uuid::new_v4();
        let sibling = Uuid::new_v4();
        // [parent, child, grandchild, sibling]
        let actions = vec![
            action(parent, None),
            action(child, Some(parent)),
            action(grandchild, Some(child)),
            action(sibling, None),
        ];
        // new child should go at index 3 (after grandchild, before sibling)
        assert_eq!(insert_index_after_descendants(&actions, parent), 3);
    }

    #[test]
    fn insert_after_last_descendant_unknown_parent_appends() {
        let unknown = Uuid::new_v4();
        let actions = vec![action(Uuid::new_v4(), None)];
        assert_eq!(insert_index_after_descendants(&actions, unknown), actions.len());
    }
}
