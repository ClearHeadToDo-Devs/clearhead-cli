use std::path::{Path, PathBuf};

pub enum ResolvedScope {
    Charter {
        file_path: PathBuf,
    },
    Plan {
        file_path: PathBuf,
        _plan_query: String,
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

    // 2 segments: validate the plan exists (eager error on typo)
    let actions = crate::commands::load_file(&file_path)?;
    if clearhead_cli::resolve_reference(&actions, segments[1]).is_none() {
        return Err(format!(
            "No plan found matching '{}' in charter '{}'",
            segments[1], segments[0]
        ));
    }

    Ok(ResolvedScope::Plan {
        file_path,
        _plan_query: segments[1].to_string(),
    })
}
