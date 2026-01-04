use crate::entities::{ActionState, ParsedDocument, SourceRange};
use chrono::Local;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq)]
pub struct LintDiagnostic {
    pub code: String,
    pub severity: LintSeverity,
    pub message: String,
    pub range: SourceRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Error,
    Warning,
    Info,
}

impl LintDiagnostic {
    fn new(code: &str, severity: LintSeverity, message: String, range: SourceRange) -> Self {
        Self {
            code: code.to_string(),
            severity,
            message,
            range,
        }
    }

    fn error(code: &str, message: String, range: SourceRange) -> Self {
        Self::new(code, LintSeverity::Error, message, range)
    }

    fn warning(code: &str, message: String, range: SourceRange) -> Self {
        Self::new(code, LintSeverity::Warning, message, range)
    }

    fn info(code: &str, message: String, range: SourceRange) -> Self {
        Self::new(code, LintSeverity::Info, message, range)
    }
}

/// Lint a parsed document and return all diagnostics
pub fn lint_document(doc: &ParsedDocument) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen_ids = HashSet::<Uuid>::new();

    for action in &doc.actions {
        if let Some(metadata) = doc.source_map.get(&action.id) {
            // Rule: Missing UUID (Info level style rule now)
            if metadata.is_id_generated && metadata.raw_id.is_none() {
                diagnostics.push(LintDiagnostic::info(
                    "missing-id",
                    "Action is missing a UUID. Use 'Hydrate Action' to add one.".to_string(),
                    metadata.root,
                ));
            }

            // E004: Invalid UUID Format
            if let Some(raw_id) = &metadata.raw_id {
                diagnostics.push(LintDiagnostic::error(
                    "E004",
                    format!("Invalid UUID format: '{}' (E004).", raw_id),
                    metadata.root,
                ));
            }

            // E005: Duplicate ID
            if !metadata.is_id_generated {
                if !seen_ids.insert(action.id) {
                    diagnostics.push(LintDiagnostic::error(
                        "E005",
                        format!("Duplicate action ID found: {} (E005)", action.id),
                        metadata.root,
                    ));
                }
            }

            // Tree Consistency Rules (E012, E013)
            let children: Vec<_> = doc
                .actions
                .iter()
                .filter(|a| a.parent_id == Some(action.id))
                .collect();

            if !children.is_empty() {
                let all_children_completed = children
                    .iter()
                    .all(|c| c.state == ActionState::Completed);
                let any_children_uncompleted = children
                    .iter()
                    .any(|c| c.state != ActionState::Completed);
                let is_completed = action.state == ActionState::Completed;

                // E012: Completed parent with uncompleted children
                if is_completed && any_children_uncompleted {
                    diagnostics.push(LintDiagnostic::error(
                        "E012",
                        "Parent is completed but some children are still active (E012)."
                            .to_string(),
                        metadata.root,
                    ));
                }

                // E013: Uncompleted parent with all children completed
                if !is_completed && all_children_completed {
                    diagnostics.push(LintDiagnostic::warning(
                        "E013",
                        "All children are completed. Should this parent be completed too? (E013)"
                            .to_string(),
                        metadata.root,
                    ));
                }
            }

            // E001: Completed action missing date
            if action.state == ActionState::Completed && action.completed_date_time.is_none() {
                diagnostics.push(LintDiagnostic::error(
                    "E001",
                    "Completed action is missing a completion date (E001).".to_string(),
                    metadata.root,
                ));
            }

            // E002: Completed date requires completed state
            if action.state != ActionState::Completed && action.completed_date_time.is_some() {
                diagnostics.push(LintDiagnostic::error(
                    "E002",
                    "Action has a completion date but is not marked as completed (E002).".to_string(),
                    metadata.completed_date.unwrap_or(metadata.root),
                ));
            }

            // E003: Invalid Priority Level (1-5)
            if let Some(priority) = action.priority {
                if priority == 0 || priority > 5 {
                    diagnostics.push(LintDiagnostic::error(
                        "E003",
                        format!("Priority must be 1-5 (got {}) (E003).", priority),
                        metadata.root,
                    ));
                }
            }

            // E008: Empty Context Tag
            if let Some(contexts) = &action.context_list {
                if contexts.iter().any(|c| c.is_empty()) {
                    diagnostics.push(LintDiagnostic::error(
                        "E008",
                        "Context tags cannot be empty (E008).".to_string(),
                        metadata.root,
                    ));
                }
            }

            // E014: Missing Creation Date
            let is_v7 = !metadata.is_id_generated && action.id.get_variant() == uuid::Variant::RFC4122 && action.id.get_version_num() == 7;
            if action.created_date_time.is_none() && !is_v7 {
                diagnostics.push(LintDiagnostic::error(
                    "E014",
                    "Action is missing a creation date (E014).".to_string(),
                    metadata.root,
                ));
            }

            // E015: Future Creation Date
            if let Some(created) = action.created_date_time {
                if created > Local::now() {
                    diagnostics.push(LintDiagnostic::error(
                        "E015",
                        "Creation date cannot be in the future (E015).".to_string(),
                        metadata.created_date.unwrap_or(metadata.root),
                    ));
                }

                // E016: Completion Before Creation
                if let Some(completed) = action.completed_date_time {
                    if completed < created {
                        diagnostics.push(LintDiagnostic::error(
                            "E016",
                            "Completion date cannot be before creation date (E016).".to_string(),
                            metadata.completed_date.unwrap_or(metadata.root),
                        ));
                    }
                }
            }
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::get_parsed_document;

    #[test]
    fn test_lint_missing_id() {
        let text = "[ ] This action has no ID";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        // missing-id (Info) + E014 (Error)
        assert_eq!(diagnostics.len(), 2);

        // Find the missing-id diagnostic
        let uuid_diag = diagnostics
            .iter()
            .find(|d| d.code == "missing-id")
            .unwrap();
        assert_eq!(uuid_diag.severity, LintSeverity::Info);
        assert!(uuid_diag.message.contains("missing a UUID"));

        // Find the E014 diagnostic
        let created_diag = diagnostics.iter().find(|d| d.code == "E014").unwrap();
        assert_eq!(created_diag.severity, LintSeverity::Error);
        assert!(created_diag.message.contains("missing a creation date"));
    }

    #[test]
    fn test_lint_has_id() {
        let text = "[ ] This action has an ID #01942d99-4c27-77f6-9316-107024843939";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert_eq!(diagnostics.len(), 0);
    }
}