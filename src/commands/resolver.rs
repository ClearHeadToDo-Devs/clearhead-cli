use std::path::{Path, PathBuf};
use uuid::Uuid;

pub enum ResolvedScope {
    Charter {
        file_path: PathBuf,
    },
    Plan {
        file_path: PathBuf,
        plan_id: Uuid,
    },
}

/// Resolve a domain reference string like `"health"` or `"health/exercise"` to a
/// concrete file path (and optionally a plan query within that file).
///
/// Delegates to existing `charter_to_file_path` and `resolve_reference` — does not
/// reimplement resolution logic.
pub fn resolve_domain_ref(data_dir: &Path, ref_str: &str) -> Result<ResolvedScope, String> {
    let segments: Vec<&str> = ref_str.splitn(3, '/').collect();

    if segments.len() > 2 {
        return Err("Reference paths deeper than charter/plan are not yet supported.".to_string());
    }

    let file_path = crate::commands::charter_to_file_path(data_dir, segments[0])?;

    if segments.len() == 1 {
        return Ok(ResolvedScope::Charter { file_path });
    }

    // 2 segments: resolve the plan and extract its UUID
    let actions = crate::commands::load_file(&file_path)?;
    let resolved = clearhead_cli::resolve_reference(&actions, segments[1])
        .ok_or_else(|| format!("No plan found matching '{}' in charter '{}'", segments[1], segments[0]))?;
    let plan_id = actions[resolved.index].id;

    Ok(ResolvedScope::Plan {
        file_path,
        plan_id,
    })
}
