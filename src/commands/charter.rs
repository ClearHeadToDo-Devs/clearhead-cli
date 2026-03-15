use tracing::info;

use crate::argparser;
use crate::commands::CommandContext;
use clearhead_core::{FsWorkspaceStore, WorkspaceStore, ObjectiveRef};

pub fn read_charters(
    ctx: &CommandContext,
    format: &Option<argparser::Format>,
    explicit_only: bool,
) -> Result<(), String> {
    use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};

    let store = FsWorkspaceStore::new(&ctx.data_dir);
    let mut charters = store.discover_charters().map_err(|e| e.to_string())?;

    if explicit_only {
        charters.retain(|dc| dc.is_explicit);
    }

    if charters.is_empty() {
        println!("No charters found.");
        return Ok(());
    }

    let use_json = matches!(format, Some(argparser::Format::Json));

    if use_json {
        let json_charters: Vec<_> = charters.iter().map(|dc| &dc.charter).collect();
        let json = serde_json::to_string_pretty(&json_charters)
            .map_err(|e| format!("Failed to serialize charters: {}", e))?;
        println!("{}", json);
    } else {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);

        table.set_header(vec![
            Cell::new("Name").fg(Color::Cyan),
            Cell::new("Type").fg(Color::Cyan),
            Cell::new("Alias").fg(Color::Cyan),
            Cell::new("Source").fg(Color::Cyan),
        ]);

        for dc in &charters {
            let type_str = if dc.is_explicit {
                "explicit"
            } else {
                "implicit"
            };
            let alias = dc.charter.alias.as_deref().unwrap_or("-");
            let source_str = if dc.is_explicit {
                dc.source_key.clone()
            } else {
                format!("{} (inferred)", dc.source_key)
            };

            table.add_row(vec![
                Cell::new(&dc.charter.title),
                Cell::new(type_str),
                Cell::new(alias),
                Cell::new(source_str),
            ]);
        }

        println!("{}", table);
    }
    Ok(())
}

pub fn show_charter(ctx: &CommandContext, query: &str) -> Result<(), String> {
    use clearhead_core::workspace::store::infer_project_name;
    use std::path::Path;

    let store = FsWorkspaceStore::new(&ctx.data_dir);
    let charters = store.discover_charters().map_err(|e| e.to_string())?;

    let found = resolve_discovered_charter(&charters, query)
        .ok_or_else(|| format!("No charter found matching '{}'", query))?;

    let formatted = clearhead_core::format_charter(&found.charter);
    println!("{}", formatted);

    let charter_base = found.charter.title.to_lowercase();
    let objectives = store.list_objectives().map_err(|e| e.to_string())?;
    let plan_count: usize = objectives
        .iter()
        .filter(|obj| {
            infer_project_name(Path::new(&obj.key))
                .map(|n| n.to_lowercase() == charter_base)
                .unwrap_or(false)
        })
        .filter_map(|obj| store.load_domain_model(obj).ok())
        .map(|model| model.all_plans().len())
        .sum();

    if plan_count > 0 {
        println!("Plans: {}", plan_count);
    }

    Ok(())
}

/// Resolve a charter by UUID prefix, alias, or name from DiscoveredCharter list.
pub fn resolve_discovered_charter<'a>(
    charters: &'a [clearhead_core::DiscoveredCharter],
    query: &str,
) -> Option<&'a clearhead_core::DiscoveredCharter> {
    let query_lower = query.to_lowercase();

    // 1. Full UUID match
    if let Ok(uuid) = uuid::Uuid::parse_str(query) {
        if let Some(dc) = charters.iter().find(|dc| dc.charter.id == uuid) {
            return Some(dc);
        }
    }

    // 2. Short UUID prefix
    if query.len() >= 4 && query.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        if let Some(dc) = charters
            .iter()
            .find(|dc| dc.charter.id.to_string().starts_with(query))
        {
            return Some(dc);
        }
    }

    // 3. Alias match (case-insensitive, exact)
    if let Some(dc) = charters.iter().find(|dc| {
        dc.charter
            .alias
            .as_ref()
            .is_some_and(|a: &String| a.to_lowercase() == query_lower)
    }) {
        return Some(dc);
    }

    // 4. Title match (case-insensitive, partial)
    charters
        .iter()
        .find(|dc| dc.charter.title.to_lowercase().contains(&query_lower))
}

pub fn add_charter(
    ctx: &CommandContext,
    title: &str,
    alias: &Option<String>,
    parent: &Option<String>,
    write: bool,
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

    let formatted = clearhead_core::format_charter(&charter);

    if write {
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

        let mut store = FsWorkspaceStore::new(&ctx.data_dir);
        let objective = ObjectiveRef::new(&filename);
        store.save_charter(&objective, &charter).map_err(|e| e.to_string())?;

        info!(title = %title, id = %id, path = %file_path.display(), "Charter created");
        println!(
            "Created charter: {} #{}\nWritten to: {}",
            title,
            id,
            file_path.display()
        );
    } else {
        println!("{}", formatted);
    }
    Ok(())
}
