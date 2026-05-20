use std::collections::HashMap;

use tracing::info;

use crate::argparser;
use crate::commands::CommandContext;
use clearhead_core::{ActionState, Charter};

pub fn read_charters(
    ctx: &CommandContext,
    format: &Option<argparser::Format>,
    explicit_only: bool,
) -> Result<(), String> {
    let model = clearhead_core::load_domain_model(&ctx.data_dir).map_err(|e| e.to_string())?;

    let mut charters: Vec<&Charter> = model.charters.iter().collect();

    if explicit_only {
        // An explicit charter has metadata beyond just a title — id that is non-random,
        // description, alias, or objectives. The simplest proxy: alias is set.
        charters.retain(|c| c.alias.is_some() || c.description.is_some());
    }

    if charters.is_empty() {
        println!("No charters found.");
        return Ok(());
    }

    match format {
        Some(argparser::Format::Json) => {
            let json = serde_json::to_string_pretty(&charters)
                .map_err(|e| format!("Failed to serialize charters: {}", e))?;
            println!("{}", json);
        }
        Some(argparser::Format::Table) => {
            print_charter_table(&charters);
        }
        _ => {
            print_charter_tree(&charters);
        }
    }
    Ok(())
}

fn print_charter_table(charters: &[&Charter]) {
    use comfy_table::{Cell, Color, Table, presets::UTF8_FULL};

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_header(vec![
            Cell::new("Title").fg(Color::Cyan),
            Cell::new("Alias").fg(Color::Cyan),
            Cell::new("Parent").fg(Color::Cyan),
            Cell::new("Open Actions").fg(Color::Cyan),
        ]);

    let sorted = sort_charters_hierarchically(charters);

    for charter in sorted {
        let open = open_act_count(charter);
        table.add_row(vec![
            Cell::new(&charter.title),
            Cell::new(charter.alias.as_deref().unwrap_or("-")),
            Cell::new(charter.parent.as_deref().unwrap_or("-")),
            Cell::new(if open > 0 { open.to_string() } else { "-".to_string() }),
        ]);
    }

    println!("{table}");
}

fn sort_charters_hierarchically<'a>(charters: &[&'a Charter]) -> Vec<&'a Charter> {
    let mut by_parent: HashMap<String, Vec<&Charter>> = HashMap::new();
    let mut roots: Vec<&Charter> = Vec::new();

    let all_keys: std::collections::HashSet<String> = charters
        .iter()
        .flat_map(|c| {
            let mut v = vec![c.title.to_lowercase()];
            if let Some(a) = &c.alias {
                v.push(a.to_lowercase());
            }
            v
        })
        .collect();

    for &c in charters {
        match c.parent.as_deref() {
            Some(p) if all_keys.contains(&p.to_lowercase()) => {
                by_parent.entry(p.to_lowercase()).or_default().push(c);
            }
            _ => roots.push(c),
        }
    }

    // Sort roots alphabetically
    roots.sort_by(|a, b| a.title.cmp(&b.title));

    let mut result = Vec::new();
    for root in roots {
        flatten_charter_hierarchy(root, &by_parent, &mut result);
    }
    result
}

fn flatten_charter_hierarchy<'a>(
    charter: &'a Charter,
    by_parent: &HashMap<String, Vec<&'a Charter>>,
    result: &mut Vec<&'a Charter>,
) {
    result.push(charter);
    let mut kids = charter_children(charter, by_parent);
    kids.sort_by(|a, b| a.title.cmp(&b.title));
    for kid in kids {
        flatten_charter_hierarchy(kid, by_parent, result);
    }
}

fn print_charter_tree(charters: &[&Charter]) {
    let mut by_parent: HashMap<String, Vec<&Charter>> = HashMap::new();
    let mut roots: Vec<&Charter> = Vec::new();

    let all_keys: std::collections::HashSet<String> = charters
        .iter()
        .flat_map(|c| {
            let mut v = vec![c.title.to_lowercase()];
            if let Some(a) = &c.alias {
                v.push(a.to_lowercase());
            }
            v
        })
        .collect();

    for &c in charters {
        match c.parent.as_deref() {
            Some(p) if all_keys.contains(&p.to_lowercase()) => {
                by_parent.entry(p.to_lowercase()).or_default().push(c);
            }
            _ => roots.push(c),
        }
    }

    // Sort roots alphabetically
    roots.sort_by(|a, b| a.title.cmp(&b.title));

    for root in &roots {
        let open = open_act_count(root);
        let open_str = if open > 0 { format!("  ({open} open)") } else { String::new() };
        println!("{}{}", root.title, open_str);
        let kids = charter_children(root, &by_parent);
        for (i, kid) in kids.iter().enumerate() {
            print_charter_node(kid, &by_parent, "", i == kids.len() - 1);
        }
    }
}

fn print_charter_node(
    charter: &Charter,
    by_parent: &HashMap<String, Vec<&Charter>>,
    prefix: &str,
    is_last: bool,
) {
    let connector = if is_last { "└── " } else { "├── " };
    let open = open_act_count(charter);
    let open_str = if open > 0 { format!("  ({open} open)") } else { String::new() };
    println!("{}{}{}{}", prefix, connector, charter.title, open_str);

    let child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
    let mut kids = charter_children(charter, by_parent);
    kids.sort_by(|a, b| a.title.cmp(&b.title));
    for (i, kid) in kids.iter().enumerate() {
        print_charter_node(kid, by_parent, &child_prefix, i == kids.len() - 1);
    }
}

fn charter_children<'a>(
    charter: &Charter,
    by_parent: &HashMap<String, Vec<&'a Charter>>,
) -> Vec<&'a Charter> {
    let mut kids: Vec<&Charter> = Vec::new();
    if let Some(alias) = charter.alias.as_deref() {
        kids.extend(
            by_parent
                .get(&alias.to_lowercase())
                .into_iter()
                .flatten()
                .copied(),
        );
    }
    for &kid in by_parent
        .get(&charter.title.to_lowercase())
        .into_iter()
        .flatten()
    {
        if !kids.iter().any(|k| k.id == kid.id) {
            kids.push(kid);
        }
    }
    kids
}

fn open_act_count(charter: &Charter) -> usize {
    charter
        .actions
        .iter()
        .filter(|a| !matches!(a.state, ActionState::Completed | ActionState::Cancelled))
        .count()
}

pub fn show_charter(ctx: &CommandContext, query: &str) -> Result<(), String> {
    let model = clearhead_core::load_domain_model(&ctx.data_dir).map_err(|e| e.to_string())?;

    let found = resolve_charter(&model.charters, query)
        .ok_or_else(|| format!("No charter found matching '{}'", query))?;

    let formatted = clearhead_core::format_charter(found);
    println!("{}", formatted);

    let plan_count = found.plans.len();
    if plan_count > 0 {
        println!("Plans: {}", plan_count);
    }

    Ok(())
}

/// Resolve a charter by UUID prefix, alias, or name from a slice of charters.
pub fn resolve_charter<'a>(charters: &'a [Charter], query: &str) -> Option<&'a Charter> {
    let query_lower = query.to_lowercase();

    // 1. Full UUID match
    if let Ok(uuid) = uuid::Uuid::parse_str(query) {
        if let Some(c) = charters.iter().find(|c| c.id == uuid) {
            return Some(c);
        }
    }

    // 2. Short UUID prefix (8 hex chars)
    if query.len() >= 4 && query.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        if let Some(c) = charters
            .iter()
            .find(|c| c.id.to_string().starts_with(query))
        {
            return Some(c);
        }
    }

    // 3. Alias match (case-insensitive, exact)
    if let Some(c) = charters.iter().find(|c| {
        c.alias
            .as_ref()
            .is_some_and(|a| a.to_lowercase() == query_lower)
    }) {
        return Some(c);
    }

    // 4. Title match (case-insensitive, partial)
    charters
        .iter()
        .find(|c| c.title.to_lowercase().contains(&query_lower))
}

pub fn add_charter(
    ctx: &CommandContext,
    title: &str,
    alias: &Option<String>,
    parent: &Option<String>,
    template: &Option<String>,
    dry_run: bool,
) -> Result<(), String> {
    use clearhead_core::domain::Charter;
    use clearhead_core::workspace::templates;

    let id = uuid::Uuid::now_v7();
    let charter = Charter {
        id,
        title: title.to_string(),
        description: None,
        alias: alias.clone(),
        parent: parent.clone(),
        objectives: None,
        state: None,
        plans: vec![],
        actions: vec![],
        workspace: None,
    };

    if dry_run {
        let formatted = clearhead_core::format_charter(&charter);
        println!("{}", formatted);
        if let Some(tpl_name) = template {
            println!("Would apply template '{}'", tpl_name);
        }
        return Ok(());
    }

    let filename = alias
        .as_deref()
        .unwrap_or(title)
        .to_lowercase()
        .replace(' ', "-")
        .replace('&', "and");
    let file_path = ctx.data_dir.join(format!("{}.md", filename));

    if file_path.exists() {
        return Err(format!("File already exists: {}", file_path.display()));
    }

    let content = clearhead_core::format_charter(&charter);
    std::fs::write(&file_path, content).map_err(|e| format!("Failed to write charter: {}", e))?;

    info!(title = %title, id = %id, path = %file_path.display(), "Charter created");
    println!("{}", id);

    if let Some(tpl_name) = template {
        let charter_dir = file_path.parent().unwrap_or(std::path::Path::new(""));
        let data_root = clearhead_core::workspace_data_root(&ctx.data_dir);

        let tpl_path = templates::resolve_template(charter_dir, &data_root, tpl_name)
            .map_err(|e| format!("Failed to resolve template: {}", e))?
            .ok_or_else(|| format!("Template '{}' not found", tpl_name))?;

        let tpl_acts = clearhead_core::workspace::read_acts(&tpl_path)
            .map_err(|e| format!("Failed to read template: {}", e))?;

        let instantiated =
            templates::instantiate_template(&tpl_acts, |_| uuid::Uuid::now_v7(), None);

        let actions_path = ctx.data_dir.join(format!("{}.actions", filename));
        super::save_file(&actions_path, &instantiated)?;

        println!(
            "Applied template '{}': {} act(s) to {}",
            tpl_name,
            instantiated.len(),
            actions_path.display()
        );
    }

    Ok(())
}

// ============================================================================
// archive charter
// ============================================================================

/// Archive a charter (or all closed charters) into `archive.ttl`.
///
/// Requires `state: Closed` in the charter frontmatter. Open actions in the
/// primary `.actions` file are a hard stop unless `force` is true.
pub fn archive_charter(
    ctx: &CommandContext,
    query: &Option<String>,
    closed: bool,
    force: bool,
    dry_run: bool,
) -> Result<(), String> {
    use clearhead_core::{ArchiveCharterOptions, archive_charter as do_archive, archive_closed_charters};

    let opts = ArchiveCharterOptions { force, dry_run };

    if closed {
        // Sweep every charter whose frontmatter carries `state: Closed`.
        let results = archive_closed_charters(&ctx.data_dir, &opts)
            .map_err(|e| e.to_string())?;

        if results.is_empty() {
            println!("No closed charters found to archive.");
            return Ok(());
        }

        for r in &results {
            print_archive_result(r);
        }
        return Ok(());
    }

    let q = query
        .as_deref()
        .ok_or_else(|| "Provide a charter name/alias/UUID or pass --closed".to_string())?;

    let result = do_archive(&ctx.data_dir, q, &opts).map_err(|e| e.to_string())?;
    print_archive_result(&result);
    Ok(())
}

fn print_archive_result(r: &clearhead_core::ArchiveCharterResult) {
    let prefix = if r.was_dry_run { "[dry-run] Would archive" } else { "Archived" };
    println!(
        "{} charter '{}': {} primary action(s), {} completed action(s), {} plan(s) → {}",
        prefix,
        r.charter_name,
        r.primary_actions_swept,
        r.completed_actions_swept,
        r.plans_swept,
        r.archive_ttl_path.display(),
    );
}

// ============================================================================
// update charter
// ============================================================================

/// Update a charter's metadata fields (currently: state, title, alias).
///
/// Writes the changes back to the charter's `.md` file. Errors if the charter
/// has no `.md` file (implicit charters have no writable file).
pub fn update_charter(
    ctx: &CommandContext,
    query: &str,
    state: &Option<crate::argparser::CharterStateArg>,
    title: &Option<String>,
    alias: &Option<String>,
    dry_run: bool,
) -> Result<(), String> {
    let mcs = clearhead_core::load_workspace(&ctx.data_dir).map_err(|e| e.to_string())?;
    let charters: Vec<Charter> = mcs.iter().cloned().map(Charter::from).collect();

    let mc = resolve_charter(&charters, query)
        .ok_or_else(|| format!("No charter found matching '{}'", query))?;

    // Find the MarkdownCharter so we have the file path
    let mc_full = mcs
        .iter()
        .find(|c| c.id == mc.id)
        .ok_or_else(|| format!("Internal: MarkdownCharter for '{}' missing", query))?;

    let md_path_rel = mc_full
        .md_file
        .as_ref()
        .ok_or_else(|| format!("Charter '{}' has no .md file; add one to make it writable", mc.title))?;

    let charter_root = clearhead_core::charter_root(&ctx.data_dir);
    let md_path = charter_root.join(md_path_rel);

    // Build updated charter
    let mut updated = mc.clone();
    if let Some(s) = state {
        updated.state = Some((*s).into());
    }
    if let Some(t) = title {
        updated.title = t.clone();
    }
    if let Some(a) = alias {
        updated.alias = Some(a.clone());
    }

    let formatted = clearhead_core::format_charter(&updated);

    if dry_run {
        println!("Would write to {}:\n{}", md_path.display(), formatted);
        return Ok(());
    }

    std::fs::write(&md_path, &formatted)
        .map_err(|e| format!("Failed to write '{}': {}", md_path.display(), e))?;

    info!(
        charter = %mc.title,
        path = %md_path.display(),
        state = ?updated.state,
        "Charter updated"
    );

    if let Some(new_state) = &updated.state {
        println!(
            "Charter '{}' updated: state → {}",
            updated.title, new_state
        );
    } else {
        println!("Charter '{}' updated.", updated.title);
    }

    Ok(())
}
