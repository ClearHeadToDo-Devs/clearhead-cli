use tracing::info;

use crate::argparser;
use crate::commands::CommandContext;
use clearhead_core::Charter;

pub fn read_charters(
    ctx: &CommandContext,
    format: &Option<argparser::Format>,
    explicit_only: bool,
) -> Result<(), String> {
    use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};

    let model = clearhead_core::load_domain_model(&ctx.data_dir)
        .map_err(|e| e.to_string())?;

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

    let use_json = matches!(format, Some(argparser::Format::Json));

    if use_json {
        let json = serde_json::to_string_pretty(&charters)
            .map_err(|e| format!("Failed to serialize charters: {}", e))?;
        println!("{}", json);
    } else {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.set_header(vec![
            Cell::new("Name").fg(Color::Cyan),
            Cell::new("Parent").fg(Color::Cyan),
            Cell::new("Alias").fg(Color::Cyan),
            Cell::new("ID").fg(Color::Cyan),
        ]);

        for charter in &charters {
            let alias = charter.alias.as_deref().unwrap_or("-");
            let parent = charter.parent.as_deref().unwrap_or("-");
            let short_id = &charter.id.to_string()[..8];

            table.add_row(vec![
                Cell::new(&charter.title),
                Cell::new(parent),
                Cell::new(alias),
                Cell::new(short_id),
            ]);
        }

        println!("{}", table);
    }
    Ok(())
}

pub fn show_charter(ctx: &CommandContext, query: &str) -> Result<(), String> {
    let model = clearhead_core::load_domain_model(&ctx.data_dir)
        .map_err(|e| e.to_string())?;

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
        if let Some(c) = charters.iter().find(|c| c.id.to_string().starts_with(query)) {
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
    dry_run: bool,
) -> Result<(), String> {
    use clearhead_core::domain::Charter;

    let id = uuid::Uuid::now_v7();
    let charter = Charter {
        id,
        title: title.to_string(),
        description: None,
        alias: alias.clone(),
        parent: parent.clone(),
        objectives: None,
        plans: vec![],
    };

    if dry_run {
        let formatted = clearhead_core::format_charter(&charter);
        println!("{}", formatted);
    } else {
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
        std::fs::write(&file_path, content)
            .map_err(|e| format!("Failed to write charter: {}", e))?;

        info!(title = %title, id = %id, path = %file_path.display(), "Charter created");
        println!("{}", id);
    }
    Ok(())
}
