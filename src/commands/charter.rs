use std::fs;
use tracing::info;

use crate::argparser;
use crate::commands::CommandContext;

pub fn read_charters(
    ctx: &CommandContext,
    format: &Option<argparser::Format>,
    explicit_only: bool,
) -> Result<(), String> {
    use clearhead_cli::workspace::{CharterSource, discover_charters};
    use comfy_table::{Cell, Color, ContentArrangement, Table, presets::UTF8_FULL};

    let mut charters = discover_charters(&ctx.data_dir)?;

    if explicit_only {
        charters.retain(|sc| sc.is_explicit());
    }

    if charters.is_empty() {
        println!("No charters found.");
        return Ok(());
    }

    let use_json = matches!(format, Some(argparser::Format::Json));

    if use_json {
        let json_charters: Vec<_> = charters.iter().map(|sc| &sc.charter).collect();
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

        for sc in &charters {
            let type_str = if sc.is_explicit() {
                "explicit"
            } else {
                "implicit"
            };
            let alias = sc.charter.alias.as_deref().unwrap_or("-");
            let source_str = match &sc.source {
                CharterSource::ExplicitFile(p) => p.display().to_string(),
                CharterSource::ImplicitFromFile(p) => {
                    format!("{} (inferred)", p.display())
                }
                CharterSource::ImplicitFromDirectory(p) => {
                    format!("{} (inferred)", p.display())
                }
            };

            table.add_row(vec![
                Cell::new(&sc.charter.title),
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
    use clearhead_cli::workspace::{discover_charters, load_workspace, resolve_charter};

    let charters = discover_charters(&ctx.data_dir)?;
    let found = resolve_charter(&charters, query)
        .ok_or_else(|| format!("No charter found matching '{}'", query))?;

    let formatted = clearhead_core::format_charter(&found.charter);
    println!("{}", formatted);

    let workspace = load_workspace(&ctx.data_dir)?;
    let charter_title = found.charter.title.to_lowercase();
    let plan_count = workspace
        .actions
        .sourced_actions
        .iter()
        .filter(|sa| {
            sa.source
                .project
                .as_ref()
                .is_some_and(|p| p.to_lowercase() == charter_title)
        })
        .count();

    if plan_count > 0 {
        println!("Plans: {}", plan_count);
    }

    Ok(())
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

        fs::write(&file_path, &formatted)
            .map_err(|e| format!("Failed to write charter file: {}", e))?;
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
