use std::collections::HashMap;

use crate::commands::CommandContext;

pub fn run(ctx: &CommandContext) -> Result<(), String> {
    print_config_section(ctx);
    println!();
    print_workspace_section(ctx)
}

fn print_config_section(ctx: &CommandContext) {
    println!("config");
    println!(
        "  config_file: {}{}",
        ctx.config_path.display(),
        if ctx.config_path.exists() {
            ""
        } else {
            " (missing)"
        }
    );
    println!(
        "  data_dir: {}  [override: CLEARHEAD_DATA_DIR | {}]",
        display_config_value(&ctx.config.data_dir, "<project-root-or-xdg-default>"),
        ctx.config_path.display()
    );
    println!(
        "  config_dir: {}  [override: CLEARHEAD_CONFIG_DIR | {}]",
        display_config_value(&ctx.config.config_dir, "<xdg-config-default>"),
        ctx.config_path.display()
    );
    println!(
        "  default_file: {}  [override: CLEARHEAD_DEFAULT_FILE | {}]",
        ctx.config.default_file,
        ctx.config_path.display()
    );
}

fn print_workspace_section(ctx: &CommandContext) -> Result<(), String> {
    println!("workspace");

    let workspace_source = resolve_workspace_source(ctx);
    let data_root = clearhead_core::workspace_data_root(&ctx.data_dir);
    println!(
        "  resolved_data_root: {} ({})",
        data_root.display(),
        workspace_source
    );

    let manifest = clearhead_core::collect_workspace_manifest(&ctx.data_dir)
        .map_err(|e| format!("Failed to collect workspace manifest: {}", e))?;
    let model = clearhead_core::load_domain_model(&ctx.data_dir)
        .map_err(|e| format!("Failed to load workspace: {}", e))?;

    let root_alias = find_root_charter_alias(&model.charters).unwrap_or("-".to_string());
    println!("  root_charter: {}", root_alias);

    if manifest.is_empty() {
        println!("  charters: none discovered");
    } else {
        println!("  charters:");
        for entry in manifest {
            println!(
                "    - alias={} file={} parent={} source={}",
                entry.charter_name,
                entry.path,
                entry.inferred_parent.unwrap_or_else(|| "-".to_string()),
                format_source_type(&entry.source_type)
            );
        }
    }

    let charter_count = model.charters.len();
    let plan_count: usize = model.charters.iter().map(|c| c.plans.len()).sum();
    let act_count: usize = model.charters.iter().map(|c| c.actions.len()).sum();
    let warnings = count_workspace_warnings(&model.charters);

    println!(
        "  graph_summary: {} charters | {} plans | {} acts | {} warnings",
        charter_count, plan_count, act_count, warnings
    );

    Ok(())
}

fn display_config_value(value: &str, fallback_label: &str) -> String {
    if value.is_empty() {
        fallback_label.to_string()
    } else {
        value.to_string()
    }
}

fn resolve_workspace_source(ctx: &CommandContext) -> &'static str {
    if std::env::var("CLEARHEAD_DATA_DIR").is_ok() {
        "env"
    } else if !ctx.config.data_dir.is_empty() {
        "config"
    } else if ctx.project_root.is_some() {
        "cwd-walk"
    } else {
        "xdg-default"
    }
}

fn find_root_charter_alias(charters: &[clearhead_core::Charter]) -> Option<String> {
    charters
        .iter()
        .find(|charter| charter.parent.is_none())
        .map(|charter| {
            charter
                .alias
                .clone()
                .unwrap_or_else(|| charter.title.clone())
        })
}

fn format_source_type(source_type: &clearhead_core::ManifestSourceType) -> &'static str {
    match source_type {
        clearhead_core::ManifestSourceType::Actions => "actions",
        clearhead_core::ManifestSourceType::Markdown => "markdown",
        clearhead_core::ManifestSourceType::Ics => "ics",
        clearhead_core::ManifestSourceType::ActionsPlusMarkdown => "actions+markdown",
        clearhead_core::ManifestSourceType::ActionsPlusIcs => "actions+ics",
        clearhead_core::ManifestSourceType::MarkdownPlusIcs => "markdown+ics",
        clearhead_core::ManifestSourceType::ActionsPlusMarkdownPlusIcs => "actions+markdown+ics",
    }
}

fn count_workspace_warnings(charters: &[clearhead_core::Charter]) -> usize {
    let known_aliases: std::collections::HashSet<&str> = charters
        .iter()
        .filter_map(|charter| charter.alias.as_deref())
        .collect();

    let mut warnings = 0;
    let mut alias_counts: HashMap<&str, usize> = HashMap::new();

    for charter in charters {
        if let Some(alias) = charter.alias.as_deref() {
            *alias_counts.entry(alias).or_default() += 1;
        }

        if let Some(parent) = charter.parent.as_deref()
            && !known_aliases.contains(parent)
        {
            warnings += 1;
        }
    }

    warnings + alias_counts.values().filter(|count| **count > 1).count()
}
