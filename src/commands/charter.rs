use std::collections::HashMap;

use tracing::info;

use crate::argparser;
use crate::commands::CommandContext;
use clearhead_core::{ActPhase, Charter};

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

    if matches!(format, Some(argparser::Format::Json)) {
        let json = serde_json::to_string_pretty(&charters)
            .map_err(|e| format!("Failed to serialize charters: {}", e))?;
        println!("{}", json);
    } else {
        print_charter_tree(&charters);
    }
    Ok(())
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
    let kids = charter_children(charter, by_parent);
    for (i, kid) in kids.iter().enumerate() {
        print_charter_node(kid, by_parent, &child_prefix, i == kids.len() - 1);
    }
}

fn charter_children<'a>(charter: &Charter, by_parent: &'a HashMap<String, Vec<&'a Charter>>) -> Vec<&'a Charter> {
    let mut kids: Vec<&Charter> = Vec::new();
    if let Some(alias) = charter.alias.as_deref() {
        kids.extend(by_parent.get(&alias.to_lowercase()).into_iter().flatten().copied());
    }
    for &kid in by_parent.get(&charter.title.to_lowercase()).into_iter().flatten() {
        if !kids.iter().any(|k| k.id == kid.id) {
            kids.push(kid);
        }
    }
    kids
}

fn open_act_count(charter: &Charter) -> usize {
    charter
        .acts
        .iter()
        .filter(|a| !matches!(a.phase, ActPhase::Completed | ActPhase::Cancelled))
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
        plans: vec![],
        acts: vec![],
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
