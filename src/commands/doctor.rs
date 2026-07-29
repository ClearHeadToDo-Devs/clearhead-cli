//! `clearhead doctor` — workspace fsck and narrowly-scoped sidecar repair.
//!
//! Diagnosis remains read-only by default. `--fix` removes only the two states
//! doctor can prove have no owner: stale per-action entries and sidecar files
//! whose companion `.actions` file no longer exists.

use crate::commands::CommandContext;
use anyhow::Context;
use clearhead_core::workspace::{Diagnosis, FindingSeverity, diagnose};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn run(ctx: &CommandContext, json: bool, fix: bool, dry_run: bool) -> anyhow::Result<()> {
    let mut diagnosis = diagnose(&ctx.data_dir, ctx.plan_override().as_deref()).context("doctor")?;

    if fix {
        prune_sidecars(ctx, &diagnosis, dry_run)?;
        if dry_run {
            return Ok(());
        }
        diagnosis = diagnose(&ctx.data_dir, ctx.plan_override().as_deref())
            .context("doctor after repair")?;
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&diagnosis)?);
    } else {
        print_report(&diagnosis);
    }

    match (diagnosis.violations(), diagnosis.warnings()) {
        (0, 0) => Ok(()),
        (0, _) => std::process::exit(1),
        (_, _) => std::process::exit(2),
    }
}

fn prune_sidecars(
    ctx: &CommandContext,
    diagnosis: &Diagnosis,
    dry_run: bool,
) -> anyhow::Result<()> {
    let mut entries: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    let mut files = Vec::new();

    for finding in &diagnosis.findings {
        match finding.code.as_str() {
            "sidecar-orphan" => {
                let id = finding
                    .message
                    .strip_prefix("entry '")
                    .and_then(|rest| rest.split_once('\''))
                    .map(|(id, _)| id.to_string())
                    .with_context(|| {
                        format!(
                            "doctor emitted an unreadable sidecar-orphan finding for {}",
                            finding.path.display()
                        )
                    })?;
                entries.entry(finding.path.clone()).or_default().push(id);
            }
            "orphaned-sidecar" => files.push(finding.path.clone()),
            _ => {}
        }
    }

    if entries.is_empty() && files.is_empty() {
        println!("No fixable sidecar state found.");
        return Ok(());
    }

    let charter_root = clearhead_core::charter_root(&ctx.data_dir);
    for (relative, ids) in &entries {
        for id in ids {
            if dry_run {
                println!(
                    "Would prune sidecar entry {} from {}",
                    id,
                    relative.display()
                );
            } else {
                println!("Pruned sidecar entry {} from {}", id, relative.display());
            }
        }
    }
    for relative in &files {
        if dry_run {
            println!("Would remove orphaned sidecar {}", relative.display());
        } else {
            println!("Removed orphaned sidecar {}", relative.display());
        }
    }
    if dry_run {
        println!(
            "Dry run: {} entr{} and {} file(s) would be removed.",
            entries.values().map(Vec::len).sum::<usize>(),
            if entries.values().map(Vec::len).sum::<usize>() == 1 {
                "y"
            } else {
                "ies"
            },
            files.len()
        );
        return Ok(());
    }

    let data_root = clearhead_core::workspace_data_root(&ctx.data_dir);
    let _lock = clearhead_core::workspace::durability::WorkspaceLock::try_acquire(&data_root)?
        .context("workspace is locked by another writer")?;
    clearhead_core::workspace::durability::recover_pending(&charter_root)?;

    for (relative, ids) in entries {
        let path = charter_root.join(&relative);
        let mut metadata = clearhead_core::workspace::sidecar::read_sidecar(&path)?;
        for id in ids {
            metadata.actions.remove(&id);
        }
        clearhead_core::workspace::sidecar::write_sidecar(&path, &metadata)?;
    }
    for relative in files {
        let path = charter_root.join(relative);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }

    Ok(())
}

fn print_report(diagnosis: &Diagnosis) {
    println!(
        "checked {} charters, {} actions",
        diagnosis.checked_charters, diagnosis.checked_actions
    );

    if diagnosis.findings.is_empty() {
        println!("workspace clean");
        return;
    }

    for severity in [FindingSeverity::Violation, FindingSeverity::Warning] {
        let group: Vec<_> = diagnosis
            .findings
            .iter()
            .filter(|f| f.severity == severity)
            .collect();
        if group.is_empty() {
            continue;
        }
        let label = match severity {
            FindingSeverity::Violation => "violations",
            FindingSeverity::Warning => "warnings",
        };
        println!("\n{} ({})", label, group.len());
        for finding in group {
            println!("  [{}] {}", finding.path.display(), finding.code);
            for line in finding.message.lines() {
                println!("    {}", line.trim_start());
            }
        }
    }

    println!(
        "\n{} violation(s), {} warning(s)",
        diagnosis.violations(),
        diagnosis.warnings()
    );
}
