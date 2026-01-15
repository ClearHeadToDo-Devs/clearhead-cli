use crate::entities::{Action, ActionState, ParsedDocument, SourceMetadata, SourceRange};
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

// ============================================================================
// Individual Linting Rules (Pure Functions - Independently Testable)
// ============================================================================

/// Check if duration is present without a do-date (E001)
fn check_duration_without_do_date(action: &Action, metadata: &SourceMetadata) -> Option<LintDiagnostic> {
    if action.do_duration.is_some() && action.do_date_time.is_none() {
        Some(LintDiagnostic::error(
            "E001",
            "Duration requires a do-date to be meaningful (E001).".to_string(),
            metadata.root,
        ))
    } else {
        None
    }
}

/// Check if recurrence is present without a do-date (E002)
fn check_recurrence_without_do_date(action: &Action, metadata: &SourceMetadata) -> Option<LintDiagnostic> {
    if action.recurrence.is_some() && action.do_date_time.is_none() {
        Some(LintDiagnostic::error(
            "E002",
            "Recurrence requires a do-date as the starting point (E002).".to_string(),
            metadata.root,
        ))
    } else {
        None
    }
}

/// Check if action is missing a UUID (Info - style preference)
fn check_missing_id(metadata: &SourceMetadata) -> Option<LintDiagnostic> {
    if metadata.is_id_generated && metadata.raw_id.is_none() {
        Some(LintDiagnostic::info(
            "missing-id",
            "Action is missing a UUID. Use 'Hydrate Action' to add one.".to_string(),
            metadata.root,
        ))
    } else {
        None
    }
}

/// Check if UUID format is invalid (E006)
fn check_invalid_uuid(metadata: &SourceMetadata) -> Option<LintDiagnostic> {
    metadata.raw_id.as_ref().map(|raw_id| {
        LintDiagnostic::error(
            "E006",
            format!("Invalid UUID format: '{}'. UUIDs must follow standard format (E006).", raw_id),
            metadata.root,
        )
    })
}

/// Check for duplicate UUIDs (I004)
fn check_duplicate_id(action: &Action, metadata: &SourceMetadata, seen_ids: &mut HashSet<Uuid>) -> Option<LintDiagnostic> {
    if !metadata.is_id_generated && !seen_ids.insert(action.id) {
        Some(LintDiagnostic::info(
            "I004",
            format!("Duplicate action ID found: {} (I004)", action.id),
            metadata.root,
        ))
    } else {
        None
    }
}

/// Check tree consistency rules (W002, W003)
fn check_tree_consistency(action: &Action, children: &[&Action], metadata: &SourceMetadata) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();

    if children.is_empty() {
        return diagnostics;
    }

    let all_children_completed = children.iter().all(|c| c.state == ActionState::Completed);
    let any_children_uncompleted = children.iter().any(|c| c.state != ActionState::Completed);
    let is_completed = action.state == ActionState::Completed;

    // W002: Completed parent with uncompleted children
    if is_completed && any_children_uncompleted {
        diagnostics.push(LintDiagnostic::warning(
            "W002",
            "Parent is completed but some children are still active (W002).".to_string(),
            metadata.root,
        ));
    }

    // W003: Uncompleted parent with all children completed
    if !is_completed && all_children_completed {
        diagnostics.push(LintDiagnostic::warning(
            "W003",
            "All children are completed. Should this parent be completed too? (W003)".to_string(),
            metadata.root,
        ));
    }

    diagnostics
}

/// Check if completed action is missing completion date (I001)
fn check_missing_completion_date(action: &Action, metadata: &SourceMetadata) -> Option<LintDiagnostic> {
    if action.state == ActionState::Completed && action.completed_date_time.is_none() {
        Some(LintDiagnostic::info(
            "I001",
            "Completed action is missing a completion date (I001).".to_string(),
            metadata.root,
        ))
    } else {
        None
    }
}

/// Check if action has completion date but isn't completed (I002)
fn check_completion_date_without_state(action: &Action, metadata: &SourceMetadata) -> Option<LintDiagnostic> {
    if action.state != ActionState::Completed && action.completed_date_time.is_some() {
        Some(LintDiagnostic::info(
            "I002",
            "Action has a completion date but is not marked as completed (I002).".to_string(),
            metadata.completed_date.unwrap_or(metadata.root),
        ))
    } else {
        None
    }
}

/// Check if priority is in valid range 1-5 (I003)
fn check_priority_range(action: &Action, metadata: &SourceMetadata) -> Option<LintDiagnostic> {
    action.priority.and_then(|priority| {
        if priority == 0 || priority > 5 {
            Some(LintDiagnostic::info(
                "I003",
                format!("Priority must be 1-5 (got {}) (I003).", priority),
                metadata.root,
            ))
        } else {
            None
        }
    })
}

/// Check for empty context tags (E003)
fn check_empty_context(action: &Action, metadata: &SourceMetadata) -> Option<LintDiagnostic> {
    action.context_list.as_ref().and_then(|contexts| {
        if contexts.iter().any(|c| c.is_empty()) {
            Some(LintDiagnostic::error(
                "E003",
                "Context tags cannot be empty (E003).".to_string(),
                metadata.root,
            ))
        } else {
            None
        }
    })
}

/// Check if action is missing creation date (W004)
fn check_missing_creation_date(action: &Action, metadata: &SourceMetadata) -> Option<LintDiagnostic> {
    let is_v7 = !metadata.is_id_generated
        && action.id.get_variant() == uuid::Variant::RFC4122
        && action.id.get_version_num() == 7;

    if action.created_date_time.is_none() && !is_v7 {
        Some(LintDiagnostic::warning(
            "W004",
            "Action is missing a creation date. Consider adding ^ or using UUIDv7 (W004).".to_string(),
            metadata.root,
        ))
    } else {
        None
    }
}

/// Check creation date validity - future dates and completion before creation (W005, W006)
fn check_creation_date_validity(action: &Action, metadata: &SourceMetadata) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();

    if let Some(created) = action.created_date_time {
        // W005: Future Creation Date
        if created > Local::now() {
            diagnostics.push(LintDiagnostic::warning(
                "W005",
                "Creation date cannot be in the future (W005).".to_string(),
                metadata.created_date.unwrap_or(metadata.root),
            ));
        }

        // W006: Completion Before Creation
        if let Some(completed) = action.completed_date_time {
            if completed < created {
                diagnostics.push(LintDiagnostic::warning(
                    "W006",
                    "Completion date cannot be before creation date (W006).".to_string(),
                    metadata.completed_date.unwrap_or(metadata.root),
                ));
            }
        }
    }

    diagnostics
}

/// Lint a parsed document and return all diagnostics
///
/// This function orchestrates all linting rules, calling each in turn.
/// Individual rules are extracted into separate functions for testability.
pub fn lint_document(doc: &ParsedDocument) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    let mut seen_ids = HashSet::<Uuid>::new();

    for action in &doc.actions {
        if let Some(metadata) = doc.source_map.get(&action.id) {
            // E001: Duration without do-date
            if let Some(diag) = check_duration_without_do_date(action, metadata) {
                diagnostics.push(diag);
            }

            // E002: Recurrence without do-date
            if let Some(diag) = check_recurrence_without_do_date(action, metadata) {
                diagnostics.push(diag);
            }

            // Check missing ID
            if let Some(diag) = check_missing_id(metadata) {
                diagnostics.push(diag);
            }

            // Check invalid UUID format (E006)
            if let Some(diag) = check_invalid_uuid(metadata) {
                diagnostics.push(diag);
            }

            // Check duplicate ID (I004)
            if let Some(diag) = check_duplicate_id(action, metadata, &mut seen_ids) {
                diagnostics.push(diag);
            }

            // Check tree consistency (requires children lookup)
            let children: Vec<_> = doc
                .actions
                .iter()
                .filter(|a| a.parent_id == Some(action.id))
                .collect();
            diagnostics.extend(check_tree_consistency(action, &children, metadata));

            // Check completion date rules
            if let Some(diag) = check_missing_completion_date(action, metadata) {
                diagnostics.push(diag);
            }

            if let Some(diag) = check_completion_date_without_state(action, metadata) {
                diagnostics.push(diag);
            }

            // Check priority range
            if let Some(diag) = check_priority_range(action, metadata) {
                diagnostics.push(diag);
            }

            // Check empty context tags
            if let Some(diag) = check_empty_context(action, metadata) {
                diagnostics.push(diag);
            }

            // Check creation date rules
            if let Some(diag) = check_missing_creation_date(action, metadata) {
                diagnostics.push(diag);
            }

            diagnostics.extend(check_creation_date_validity(action, metadata));
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
        // Only missing-id (Info) - created date is auto-injected when ID is generated
        assert_eq!(diagnostics.len(), 1);

        // Find the missing-id diagnostic
        let uuid_diag = diagnostics
            .iter()
            .find(|d| d.code == "missing-id")
            .unwrap();
        assert_eq!(uuid_diag.severity, LintSeverity::Info);
        assert!(uuid_diag.message.contains("missing a UUID"));
    }

    #[test]
    fn test_lint_has_id() {
        let text = "[ ] This action has an ID #01942d99-4c27-77f6-9316-107024843939";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert_eq!(diagnostics.len(), 0);
    }

    // ========================================================================
    // Individual Rule Tests
    // ========================================================================

    #[test]
    fn test_check_missing_completion_date_info() {
        let text = "[x] Completed task with no completion date #01942d99-4c27-77f6-9316-107024843939";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert!(diagnostics.iter().any(|d| d.code == "I001"));
        // Verify it's Info severity per spec
        let diag = diagnostics.iter().find(|d| d.code == "I001").unwrap();
        assert_eq!(diag.severity, LintSeverity::Info);
    }

    #[test]
    fn test_check_missing_completion_date_ok() {
        let text = "[x] Completed task %2025-01-09T14:00 #01942d99-4c27-77f6-9316-107024843939";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert!(!diagnostics.iter().any(|d| d.code == "I001"));
    }

    #[test]
    fn test_check_invalid_uuid_format() {
        // Test the check_invalid_uuid function directly since grammar won't parse invalid formats
        use crate::entities::{SourceMetadata, SourceRange};
        let metadata = SourceMetadata {
            root: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 10,
            },
            line_range: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 10,
            },
            do_date: None,
            completed_date: None,
            created_date: None,
            is_id_generated: false,
            raw_id: Some("invalid-uuid-format".to_string()),
        };

        let result = check_invalid_uuid(&metadata);
        assert!(result.is_some());
        assert_eq!(result.unwrap().code, "E006");
    }

    #[test]
    fn test_check_duplicate_id_detection() {
        let text = "[ ] Task 1 #01942d99-4c27-77f6-9316-107024843939\n[ ] Task 2 #01942d99-4c27-77f6-9316-107024843939";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert!(diagnostics.iter().any(|d| d.code == "I004"));
        // Verify it's Info severity per spec
        let diag = diagnostics.iter().find(|d| d.code == "I004").unwrap();
        assert_eq!(diag.severity, LintSeverity::Info);
    }

    #[test]
    fn test_check_tree_consistency_parent_completed() {
        let text = "[x] Parent task #01942d99-4c27-77f6-9316-107024843939 %2025-01-09T14:00\n>[ ] Child task still open #01942d99-4c27-77f6-9316-107024843999";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert!(diagnostics.iter().any(|d| d.code == "W002"));
        // Verify it's Warning severity per spec
        let diag = diagnostics.iter().find(|d| d.code == "W002").unwrap();
        assert_eq!(diag.severity, LintSeverity::Warning);
    }

    #[test]
    fn test_check_tree_consistency_children_completed() {
        let text = "[ ] Parent task #01942d99-4c27-77f6-9316-107024843939\n>[x] Child task completed #01942d99-4c27-77f6-9316-107024843998 %2025-01-09T14:00\n>[x] Another child completed #01942d99-4c27-77f6-9316-107024843999 %2025-01-09T14:00";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert!(diagnostics.iter().any(|d| d.code == "W003"));
        // Verify it's Warning severity per spec
        let diag = diagnostics.iter().find(|d| d.code == "W003").unwrap();
        assert_eq!(diag.severity, LintSeverity::Warning);
    }

    #[test]
    fn test_check_priority_range_invalid_zero() {
        let text = "[ ] Task with priority 0 !0 #01942d99-4c27-77f6-9316-107024843939";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert!(diagnostics.iter().any(|d| d.code == "I003"));
        // Verify it's Info severity per spec
        let diag = diagnostics.iter().find(|d| d.code == "I003").unwrap();
        assert_eq!(diag.severity, LintSeverity::Info);
    }

    #[test]
    fn test_check_priority_range_invalid_high() {
        let text = "[ ] Task with priority 6 !6 #01942d99-4c27-77f6-9316-107024843939";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert!(diagnostics.iter().any(|d| d.code == "I003"));
    }

    #[test]
    fn test_check_priority_range_valid() {
        let text = "[ ] Task with priority 3 !3 #01942d99-4c27-77f6-9316-107024843939";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert!(!diagnostics.iter().any(|d| d.code == "I003"));
    }

    #[test]
    fn test_check_empty_context_tag() {
        // Test the check_empty_context function directly since grammar may prevent empty tags
        use crate::entities::{Action, SourceMetadata, SourceRange};
        let action = Action {
            id: uuid::Uuid::parse_str("01942d99-4c27-77f6-9316-107024843939").unwrap(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Test".to_string(),
            description: None,
            priority: None,
            context_list: Some(vec!["work".to_string(), "".to_string()]),
            do_date_time: None,
            do_duration: None,
            recurrence: None,
            completed_date_time: None,
            created_date_time: None,
            predecessors: None,
            story: None,
        };
        let metadata = SourceMetadata {
            root: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 10,
            },
            line_range: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 10,
            },
            do_date: None,
            completed_date: None,
            created_date: None,
            is_id_generated: false,
            raw_id: None,
        };

        let result = check_empty_context(&action, &metadata);
        assert!(result.is_some());
        let diag = result.unwrap();
        assert_eq!(diag.code, "E003");
        assert_eq!(diag.severity, LintSeverity::Error);
    }

    #[test]
    fn test_check_creation_date_future() {
        use chrono::Duration;
        let future = Local::now() + Duration::days(1);
        let text = format!(
            "[ ] Task from the future ^{} #01942d99-4c27-77f6-9316-107024843939",
            future.format("%Y-%m-%dT%H:%M")
        );
        let parsed = get_parsed_document(&text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert!(diagnostics.iter().any(|d| d.code == "W005"));
        // Verify it's Warning severity per spec
        let diag = diagnostics.iter().find(|d| d.code == "W005").unwrap();
        assert_eq!(diag.severity, LintSeverity::Warning);
    }

    #[test]
    fn test_check_completion_before_creation() {
        let text = "[ ] Task with wrong dates ^2025-01-10T14:00 %2025-01-09T14:00 #01942d99-4c27-77f6-9316-107024843939";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = lint_document(&parsed);
        assert!(diagnostics.iter().any(|d| d.code == "W006"));
        // Verify it's Warning severity per spec
        let diag = diagnostics.iter().find(|d| d.code == "W006").unwrap();
        assert_eq!(diag.severity, LintSeverity::Warning);
    }

    // ========================================================================
    // New E001/E002 Tests (Duration and Recurrence without Do-Date)
    // ========================================================================

    #[test]
    fn test_check_duration_without_do_date() {
        // Test the function directly since parsing D without @ may not be valid in grammar
        use crate::entities::{Action, SourceMetadata, SourceRange};
        let action = Action {
            id: uuid::Uuid::parse_str("01942d99-4c27-77f6-9316-107024843939").unwrap(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Meeting".to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: None,  // No do-date
            do_duration: Some(60),  // Has duration
            recurrence: None,
            completed_date_time: None,
            created_date_time: None,
            predecessors: None,
            story: None,
        };
        let metadata = SourceMetadata {
            root: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 20,
            },
            line_range: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 20,
            },
            do_date: None,
            completed_date: None,
            created_date: None,
            is_id_generated: false,
            raw_id: None,
        };

        let result = check_duration_without_do_date(&action, &metadata);
        assert!(result.is_some());
        let diag = result.unwrap();
        assert_eq!(diag.code, "E001");
        assert_eq!(diag.severity, LintSeverity::Error);
    }

    #[test]
    fn test_check_duration_with_do_date_ok() {
        use crate::entities::{Action, SourceMetadata, SourceRange};
        let action = Action {
            id: uuid::Uuid::parse_str("01942d99-4c27-77f6-9316-107024843939").unwrap(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Meeting".to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: Some(Local::now()),  // Has do-date
            do_duration: Some(60),  // Has duration
            recurrence: None,
            completed_date_time: None,
            created_date_time: None,
            predecessors: None,
            story: None,
        };
        let metadata = SourceMetadata {
            root: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 20,
            },
            line_range: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 20,
            },
            do_date: None,
            completed_date: None,
            created_date: None,
            is_id_generated: false,
            raw_id: None,
        };

        let result = check_duration_without_do_date(&action, &metadata);
        assert!(result.is_none());
    }

    #[test]
    fn test_check_recurrence_without_do_date() {
        use crate::entities::{Action, Recurrence, SourceMetadata, SourceRange};
        let action = Action {
            id: uuid::Uuid::parse_str("01942d99-4c27-77f6-9316-107024843939").unwrap(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Daily standup".to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: None,  // No do-date
            do_duration: None,
            recurrence: Some(Recurrence {  // Has recurrence
                frequency: "daily".to_string(),
                interval: None,
                count: None,
                until: None,
                by_second: None,
                by_minute: None,
                by_hour: None,
                by_day: None,
                by_month_day: None,
                by_year_day: None,
                by_week_no: None,
                by_month: None,
                by_set_pos: None,
                week_start: None,
            }),
            completed_date_time: None,
            created_date_time: None,
            predecessors: None,
            story: None,
        };
        let metadata = SourceMetadata {
            root: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 20,
            },
            line_range: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 20,
            },
            do_date: None,
            completed_date: None,
            created_date: None,
            is_id_generated: false,
            raw_id: None,
        };

        let result = check_recurrence_without_do_date(&action, &metadata);
        assert!(result.is_some());
        let diag = result.unwrap();
        assert_eq!(diag.code, "E002");
        assert_eq!(diag.severity, LintSeverity::Error);
    }

    #[test]
    fn test_check_recurrence_with_do_date_ok() {
        use crate::entities::{Action, Recurrence, SourceMetadata, SourceRange};
        let action = Action {
            id: uuid::Uuid::parse_str("01942d99-4c27-77f6-9316-107024843939").unwrap(),
            parent_id: None,
            state: ActionState::NotStarted,
            name: "Daily standup".to_string(),
            description: None,
            priority: None,
            context_list: None,
            do_date_time: Some(Local::now()),  // Has do-date
            do_duration: None,
            recurrence: Some(Recurrence {  // Has recurrence
                frequency: "daily".to_string(),
                interval: None,
                count: None,
                until: None,
                by_second: None,
                by_minute: None,
                by_hour: None,
                by_day: None,
                by_month_day: None,
                by_year_day: None,
                by_week_no: None,
                by_month: None,
                by_set_pos: None,
                week_start: None,
            }),
            completed_date_time: None,
            created_date_time: None,
            predecessors: None,
            story: None,
        };
        let metadata = SourceMetadata {
            root: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 20,
            },
            line_range: SourceRange {
                start_row: 0,
                start_col: 0,
                end_row: 0,
                end_col: 20,
            },
            do_date: None,
            completed_date: None,
            created_date: None,
            is_id_generated: false,
            raw_id: None,
        };

        let result = check_recurrence_without_do_date(&action, &metadata);
        assert!(result.is_none());
    }
}