use chrono::{DateTime, Local};
use clearhead_cli::{LintDiagnostic, LintSeverity, ParsedDocument, lint_document};
use clearhead_core::SourceRange;
use tower_lsp_server::ls_types::*;
use tree_sitter::Tree;
use uuid::Uuid;

pub fn source_range_to_lsp_range(src: SourceRange) -> Range {
    Range {
        start: Position::new(src.start_row as u32, src.start_col as u32),
        end: Position::new(src.end_row as u32, src.end_col as u32),
    }
}

fn lint_severity_to_lsp(severity: LintSeverity) -> DiagnosticSeverity {
    match severity {
        LintSeverity::Error => DiagnosticSeverity::ERROR,
        LintSeverity::Warning => DiagnosticSeverity::WARNING,
        LintSeverity::Info => DiagnosticSeverity::INFORMATION,
    }
}

fn lint_diagnostic_to_lsp(diag: LintDiagnostic) -> Diagnostic {
    Diagnostic {
        range: source_range_to_lsp_range(diag.range),
        severity: Some(lint_severity_to_lsp(diag.severity)),
        code: Some(NumberOrString::String(diag.code)),
        source: Some("clearhead-lsp".to_string()),
        message: diag.message,
        ..Default::default()
    }
}

pub fn compute_diagnostics(doc: &ParsedDocument) -> Vec<Diagnostic> {
    lint_document(doc)
        .into_iter()
        .map(lint_diagnostic_to_lsp)
        .collect()
}

fn create_quick_fix(
    uri: Uri,
    pos: Position,
    new_text: String,
    title: String,
) -> CodeActionOrCommand {
    let mut changes = std::collections::HashMap::new();
    changes.insert(
        uri,
        vec![TextEdit {
            range: Range::new(pos, pos),
            new_text,
        }],
    );

    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        }),
        ..Default::default()
    })
}

pub fn compute_code_actions(
    doc: &ParsedDocument,
    uri: &Uri,
    range: Range,
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    for action in &doc.actions {
        if let Some(metadata) = doc.source_map.get(&action.id) {
            let action_range = source_range_to_lsp_range(metadata.root);

            if range.start.line <= action_range.end.line
                && range.end.line >= action_range.start.line
            {
                let insert_pos = source_range_to_lsp_range(metadata.line_range).end;

                // 1. Hydrate UUID
                if metadata.is_id_generated {
                    let uuid = Uuid::now_v7();
                    actions.push(create_quick_fix(
                        uri.clone(),
                        insert_pos,
                        format!(" #{}", uuid),
                        "Hydrate Action (Add UUID)".to_string(),
                    ));
                }

                // 2. Add Completion Date
                if action.state == clearhead_cli::ActionState::Completed
                    && action.completed_at.is_none()
                {
                    let now = Local::now();
                    actions.push(create_quick_fix(
                        uri.clone(),
                        insert_pos,
                        format!(" %{}", now.format("%Y-%m-%dT%H:%M")),
                        "Set Completion Date (Today)".to_string(),
                    ));
                }

                // 3. Add Creation Date
                if action.created_at.is_none() {
                    let now = Local::now();
                    actions.push(create_quick_fix(
                        uri.clone(),
                        insert_pos,
                        format!(" ^{}", now.format("%Y-%m-%dT%H:%M")),
                        "Set Creation Date (Today)".to_string(),
                    ));

                    // Derive from UUID (if not generated)
                    if !metadata.is_id_generated {
                        let timestamp_ms = (action.id.as_u128() >> 80) as i64;
                        if let Some(dt) = DateTime::from_timestamp(
                            timestamp_ms / 1000,
                            ((timestamp_ms % 1000) * 1_000_000) as u32,
                        ) {
                            let local_dt: DateTime<Local> = dt.into();
                            actions.push(create_quick_fix(
                                uri.clone(),
                                insert_pos,
                                format!(" ^{}", local_dt.format("%Y-%m-%dT%H:%M")),
                                "Derive Creation Date from UUID".to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }
    actions
}

pub fn compute_inlay_hints(
    doc: &ParsedDocument,
    base_time: Option<DateTime<Local>>,
) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let now = base_time.unwrap_or_else(Local::now);

    for action in &doc.actions {
        if let Some(metadata) = doc.source_map.get(&action.id) {
            // Do Date Hint
            if let (Some(dt), Some(range)) = (action.scheduled_at, metadata.do_date) {
                let diff = dt.signed_duration_since(now);
                let label = if diff.num_days() > 0 {
                    format!(" (due in {}d)", diff.num_days())
                } else if diff.num_days() < 0 {
                    format!(" ({}d ago)", -diff.num_days())
                } else {
                    " (due today)".to_string()
                };

                let lsp_range = source_range_to_lsp_range(range);
                hints.push(InlayHint {
                    position: lsp_range.end,
                    label: InlayHintLabel::String(label),
                    kind: Some(InlayHintKind::PARAMETER),
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(true),
                    padding_right: Some(false),
                    data: None,
                });
            }

            // Completed Date Hint
            if let (Some(dt), Some(range)) = (action.completed_at, metadata.completed_date) {
                let diff = now.signed_duration_since(dt);
                let label = format!(" (done {}d ago)", diff.num_days());

                let lsp_range = source_range_to_lsp_range(range);
                hints.push(InlayHint {
                    position: lsp_range.end,
                    label: InlayHintLabel::String(label),
                    kind: Some(InlayHintKind::PARAMETER),
                    text_edits: None,
                    tooltip: None,
                    padding_left: Some(true),
                    padding_right: Some(false),
                    data: None,
                });
            }
        }
    }
    hints
}

pub fn compute_semantic_tokens(tree: &Tree) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let mut cursor = tree.walk();
    let mut nodes_to_check = vec![tree.root_node()];

    while let Some(node) = nodes_to_check.pop() {
        let token_type = match node.kind() {
            "id" => Some(0),
            "priority" | "state_value" => Some(1),
            "name" | "description" => Some(2),
            "story" => Some(3),
            "context" => Some(4),
            "do_date" | "completed_date" => Some(5),
            _ => None,
        };

        if let Some(type_idx) = token_type {
            tokens.push(SemanticToken {
                delta_line: node.start_position().row as u32,
                delta_start: node.start_position().column as u32,
                length: (node.end_byte() - node.start_byte()) as u32,
                token_type: type_idx,
                token_modifiers_bitset: 0,
            });
        }

        for child in node.children(&mut cursor) {
            nodes_to_check.push(child);
        }
    }

    // Sort tokens by line and then by column
    tokens.sort_by(|a, b| {
        if a.delta_line != b.delta_line {
            a.delta_line.cmp(&b.delta_line)
        } else {
            a.delta_start.cmp(&b.delta_start)
        }
    });

    tokens
}

pub fn get_node_at_position(tree: &Tree, position: Position) -> Option<tree_sitter::Node<'_>> {
    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    tree.root_node()
        .named_descendant_for_point_range(point, point)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearhead_cli::get_parsed_document;
    use tree_sitter::Parser;

    #[test]
    fn test_lsp_adapter_converts_diagnostics() {
        let text = "[ ] This action has no ID";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = compute_diagnostics(&parsed);
        assert_eq!(diagnostics.len(), 1);

        assert!(
            diagnostics
                .iter()
                .all(|d| d.source == Some("clearhead-lsp".to_string()))
        );
        assert!(diagnostics.iter().all(|d| d.code.is_some()));
    }

    // Unit tests for compute_code_actions

    #[test]
    fn test_code_actions_hydrate_uuid() {
        let text = "[ ] Task without ID";
        let parsed = get_parsed_document(text).unwrap();
        let uri = Uri::from_file_path("/test.actions").unwrap();
        let range = Range::new(Position::new(0, 0), Position::new(0, 0));

        let actions = compute_code_actions(&parsed, &uri, range);

        let titles: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => Some(ca.title.as_str()),
                _ => None,
            })
            .collect();

        assert!(
            titles.contains(&"Hydrate Action (Add UUID)"),
            "Expected hydrate action, got: {:?}",
            titles
        );
    }

    #[test]
    fn test_code_actions_completion_date() {
        let text = "[x] Completed task #019baaec-00b6-7991-be34-94b68212619a";
        let parsed = get_parsed_document(text).unwrap();
        let uri = Uri::from_file_path("/test.actions").unwrap();
        let range = Range::new(Position::new(0, 0), Position::new(0, 0));

        let actions = compute_code_actions(&parsed, &uri, range);

        let titles: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => Some(ca.title.as_str()),
                _ => None,
            })
            .collect();

        assert!(
            titles.contains(&"Set Completion Date (Today)"),
            "Expected completion date action, got: {:?}",
            titles
        );
    }

    #[test]
    fn test_code_actions_creation_date() {
        let text = "[ ] Task with ID #019baaec-00b6-7991-be34-94b68212619a";
        let parsed = get_parsed_document(text).unwrap();
        let uri = Uri::from_file_path("/test.actions").unwrap();
        let range = Range::new(Position::new(0, 0), Position::new(0, 0));

        let actions = compute_code_actions(&parsed, &uri, range);

        let titles: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => Some(ca.title.as_str()),
                _ => None,
            })
            .collect();

        assert!(
            titles.contains(&"Set Creation Date (Today)"),
            "Expected creation date action, got: {:?}",
            titles
        );
        assert!(
            titles.contains(&"Derive Creation Date from UUID"),
            "Expected derive from UUID action, got: {:?}",
            titles
        );
    }

    #[test]
    fn test_code_actions_cursor_outside_action() {
        let text = "[ ] Task on line 0";
        let parsed = get_parsed_document(text).unwrap();
        let uri = Uri::from_file_path("/test.actions").unwrap();
        let range = Range::new(Position::new(5, 0), Position::new(5, 0));

        let actions = compute_code_actions(&parsed, &uri, range);

        assert!(
            actions.is_empty(),
            "Expected no actions when cursor is outside, got: {:?}",
            actions.len()
        );
    }

    #[test]
    fn test_code_actions_completed_with_date_no_suggestion() {
        let text = "[x] Done task %2026-01-15T10:00 #019baaec-00b6-7991-be34-94b68212619a";
        let parsed = get_parsed_document(text).unwrap();
        let uri = Uri::from_file_path("/test.actions").unwrap();
        let range = Range::new(Position::new(0, 0), Position::new(0, 0));

        let actions = compute_code_actions(&parsed, &uri, range);

        let titles: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(ca) => Some(ca.title.as_str()),
                _ => None,
            })
            .collect();

        assert!(
            !titles.contains(&"Set Completion Date (Today)"),
            "Should not suggest completion date when already present"
        );
    }

    // Unit tests for compute_inlay_hints

    #[test]
    fn test_inlay_hints_due_in_future() {
        let text = "[ ] Future task @2026-01-20T10:00 #019baaec-00b6-7991-be34-94b68212619a";
        let parsed = get_parsed_document(text).unwrap();
        let base_time = DateTime::parse_from_rfc3339("2026-01-15T10:00:00+00:00")
            .unwrap()
            .with_timezone(&Local);

        let hints = compute_inlay_hints(&parsed, Some(base_time));

        assert_eq!(hints.len(), 1);
        match &hints[0].label {
            InlayHintLabel::String(s) => {
                assert!(s.contains("due in"), "Expected 'due in', got: {}", s)
            }
            _ => panic!("Expected string label"),
        }
    }

    #[test]
    fn test_inlay_hints_due_in_past() {
        let text = "[ ] Overdue task @2026-01-10T10:00 #019baaec-00b6-7991-be34-94b68212619a";
        let parsed = get_parsed_document(text).unwrap();
        let base_time = DateTime::parse_from_rfc3339("2026-01-15T10:00:00+00:00")
            .unwrap()
            .with_timezone(&Local);

        let hints = compute_inlay_hints(&parsed, Some(base_time));

        assert_eq!(hints.len(), 1);
        match &hints[0].label {
            InlayHintLabel::String(s) => assert!(s.contains("ago"), "Expected 'ago', got: {}", s),
            _ => panic!("Expected string label"),
        }
    }

    #[test]
    fn test_inlay_hints_due_today() {
        let text = "[ ] Today task @2026-01-15T10:00 #019baaec-00b6-7991-be34-94b68212619a";
        let parsed = get_parsed_document(text).unwrap();
        let base_time = DateTime::parse_from_rfc3339("2026-01-15T12:00:00+00:00")
            .unwrap()
            .with_timezone(&Local);

        let hints = compute_inlay_hints(&parsed, Some(base_time));

        assert_eq!(hints.len(), 1);
        match &hints[0].label {
            InlayHintLabel::String(s) => {
                assert!(s.contains("due today"), "Expected 'due today', got: {}", s)
            }
            _ => panic!("Expected string label"),
        }
    }

    #[test]
    fn test_inlay_hints_completed_date() {
        let text = "[x] Done task %2026-01-10T10:00 #019baaec-00b6-7991-be34-94b68212619a";
        let parsed = get_parsed_document(text).unwrap();
        let base_time = DateTime::parse_from_rfc3339("2026-01-15T10:00:00+00:00")
            .unwrap()
            .with_timezone(&Local);

        let hints = compute_inlay_hints(&parsed, Some(base_time));

        assert_eq!(hints.len(), 1);
        match &hints[0].label {
            InlayHintLabel::String(s) => assert!(
                s.contains("done") && s.contains("ago"),
                "Expected 'done X ago', got: {}",
                s
            ),
            _ => panic!("Expected string label"),
        }
    }

    #[test]
    fn test_inlay_hints_no_dates_no_hints() {
        let text = "[ ] Plain task #019baaec-00b6-7991-be34-94b68212619a";
        let parsed = get_parsed_document(text).unwrap();

        let hints = compute_inlay_hints(&parsed, None);

        assert!(hints.is_empty(), "Expected no hints for task without dates");
    }

    // Unit tests for compute_semantic_tokens

    fn get_tree(text: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_actions::LANGUAGE.into())
            .unwrap();
        parser.parse(text, None).unwrap()
    }

    #[test]
    fn test_semantic_tokens_id() {
        let text = "[ ] Task #019baaec-00b6-7991-be34-94b68212619a";
        let tree = get_tree(text);

        let tokens = compute_semantic_tokens(&tree);

        assert!(
            tokens.iter().any(|t| t.token_type == 0),
            "Expected id token (type 0)"
        );
    }

    #[test]
    fn test_semantic_tokens_priority() {
        let text = "[ ] Task !2 #019baaec-00b6-7991-be34-94b68212619a";
        let tree = get_tree(text);

        let tokens = compute_semantic_tokens(&tree);

        assert!(
            tokens.iter().any(|t| t.token_type == 1),
            "Expected priority token (type 1)"
        );
    }

    #[test]
    fn test_semantic_tokens_context() {
        let text = "[ ] Task +home #019baaec-00b6-7991-be34-94b68212619a";
        let tree = get_tree(text);

        let tokens = compute_semantic_tokens(&tree);

        assert!(
            tokens.iter().any(|t| t.token_type == 4),
            "Expected context token (type 4)"
        );
    }

    #[test]
    fn test_semantic_tokens_dates() {
        let text = "[ ] Task @2026-01-20T10:00 #019baaec-00b6-7991-be34-94b68212619a";
        let tree = get_tree(text);

        let tokens = compute_semantic_tokens(&tree);

        assert!(
            tokens.iter().any(|t| t.token_type == 5),
            "Expected date token (type 5)"
        );
    }

    #[test]
    fn test_semantic_tokens_sorted_by_position() {
        let text = "[ ] Task 1 #019baaec-00b6-7991-be34-94b68212619a\n[ ] Task 2 #019baaec-00b6-7991-be34-94b68212619b";
        let tree = get_tree(text);

        let tokens = compute_semantic_tokens(&tree);

        for window in tokens.windows(2) {
            let (a, b) = (&window[0], &window[1]);
            assert!(
                b.delta_line > a.delta_line
                    || (b.delta_line == a.delta_line && b.delta_start >= a.delta_start),
                "Tokens not sorted: {:?} should come before {:?}",
                a,
                b
            );
        }
    }
}
