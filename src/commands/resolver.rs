use std::path::{Path, PathBuf};
use uuid::Uuid;

use clearhead_core::ReferenceTarget;

use crate::commands::CommandContext;

pub enum ResolvedScope {
    Charter { file_path: PathBuf },
    Plan { file_path: PathBuf, #[allow(dead_code)] plan_id: Uuid },
    Action { file_path: PathBuf, #[allow(dead_code)] action_id: Uuid },
}

/// Resolve a domain reference string (e.g. `"health"`, `"health/exercise"`,
/// `"c:clearhead-cli"`, `"a:0e8dda27"`) to a concrete file path.
///
/// Searches all configured workspaces in primary-first order; returns the
/// first match. The [`ReferenceOptions`] default applies: `c:`/`p:`/`a:`
/// prefixes allowed, alias and UUID matching only (no title fuzzing).
pub fn resolve_domain_ref(ctx: &CommandContext, ref_str: &str) -> Result<ResolvedScope, String> {
    let models = ctx.all_domain_models()?;

    let workspace_refs: Vec<(&str, &clearhead_core::DomainModel)> =
        models.iter().map(|(name, model)| (name.as_str(), model)).collect();

    let (ws_name, target) = clearhead_core::resolve_reference_in_workspaces(
        &workspace_refs,
        ref_str,
        &clearhead_core::ReferenceOptions::default(),
    )
    .map_err(|e| e.to_string())?;

    let ws_dir = ctx
        .workspace_dirs()
        .into_iter()
        .find(|(name, _)| *name == ws_name)
        .map(|(_, dir)| dir)
        .ok_or_else(|| format!("Internal: workspace '{}' missing from dirs", ws_name))?;

    let model = models
        .iter()
        .find(|(name, _)| *name == ws_name)
        .map(|(_, m)| m)
        .ok_or_else(|| format!("Internal: model for workspace '{}' missing", ws_name))?;

    match target {
        ReferenceTarget::Charter(id) => {
            let file_path = charter_actions_path(model, id, &ws_dir)?;
            Ok(ResolvedScope::Charter { file_path })
        }
        ReferenceTarget::Plan(plan_id) => {
            let charter_id = model
                .charters
                .iter()
                .find(|c| c.plans.iter().any(|p| p.id == plan_id))
                .map(|c| c.id)
                .ok_or_else(|| format!("Plan {} not found in model", plan_id))?;
            let file_path = charter_actions_path(model, charter_id, &ws_dir)?;
            Ok(ResolvedScope::Plan { file_path, plan_id })
        }
        ReferenceTarget::Action(action_id) => {
            let charter_id = model
                .charters
                .iter()
                .find(|c| c.actions.iter().any(|a| a.id == action_id))
                .map(|c| c.id)
                .ok_or_else(|| format!("Action {} not found in model", action_id))?;
            let file_path = charter_actions_path(model, charter_id, &ws_dir)?;
            Ok(ResolvedScope::Action { file_path, action_id })
        }
    }
}

fn charter_actions_path(
    model: &clearhead_core::DomainModel,
    charter_id: Uuid,
    ws_dir: &Path,
) -> Result<PathBuf, String> {
    let charter = model
        .charters
        .iter()
        .find(|c| c.id == charter_id)
        .ok_or_else(|| format!("Charter {} not found in model", charter_id))?;
    let key = charter.alias.as_deref().unwrap_or(&charter.title);
    crate::commands::charter_to_file_path(ws_dir, key)
}
