//! `clearhead doctor` — read-only workspace fsck.
//!
//! Prints every coherence finding and exits 0 (clean), 1 (warnings only),
//! or 2 (violations) so CI and hooks can gate on it. Never fixes anything:
//! cleanup belongs to the commands that own each file surface (Decision 34).

use anyhow::Context;
use crate::commands::CommandContext;
use clearhead_core::workspace::{Diagnosis, FindingSeverity, diagnose};

pub fn run(ctx: &CommandContext, json: bool) -> anyhow::Result<()> {
    let diagnosis = diagnose(&ctx.data_dir, ctx.plan_override().as_deref())
        .context("doctor")?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&diagnosis)?
        );
    } else {
        print_report(&diagnosis);
    }

    match (diagnosis.violations(), diagnosis.warnings()) {
        (0, 0) => Ok(()),
        (0, _) => std::process::exit(1),
        (_, _) => std::process::exit(2),
    }
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
