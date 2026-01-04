use chrono::{DateTime, Local};
use clearhead_cli::get_parsed_document;
use clearhead_cli::entities::{ParsedDocument, SourceRange};
use dashmap::DashMap;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};
use tree_sitter::{Parser, Tree};
use uuid::Uuid;

use clearhead_cli::format::{FormatConfig, OutputFormat, format};
use clearhead_cli::treesitter::get_node_text;

#[derive(Debug)]
struct DocumentState {
    text: String,
    tree: Tree,
    parsed: Option<ParsedDocument>,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: DashMap<Uri, DocumentState>,
}

impl Backend {
    fn get_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_actions::LANGUAGE.into())
            .expect("Error loading actions grammar");
        parser
    }

    async fn update_document(&self, uri: Uri, text: String) {
        let mut parser = Self::get_parser();
        if let Some(tree) = parser.parse(&text, None) {
            let parsed = get_parsed_document(&text).ok();
            
            let diagnostics = if let Some(ref p) = parsed {
                compute_diagnostics(p)
            } else {
                Vec::new()
            };

            self.documents.insert(
                uri.clone(),
                DocumentState {
                    text: text.clone(),
                    tree: tree.clone(),
                    parsed,
                },
            );

            self.client
                .publish_diagnostics(uri, diagnostics, None)
                .await;
        }
    }
}

fn source_range_to_lsp_range(src: SourceRange) -> Range {
    Range {
        start: Position::new(src.start_row as u32, src.start_col as u32),
        end: Position::new(src.end_row as u32, src.end_col as u32),
    }
}

/// Compute diagnostics using the parsed document model
fn compute_diagnostics(doc: &ParsedDocument) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for action in &doc.actions {
        if let Some(metadata) = doc.source_map.get(&action.id) {
            // Rule 1: Missing UUID
            if metadata.is_id_generated {
                diagnostics.push(Diagnostic {
                    range: source_range_to_lsp_range(metadata.root),
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("missing-id".to_string())),
                    source: Some("clearhead-lsp".to_string()),
                    message: "Action is missing a UUID. Use 'Hydrate Action' to add one."
                        .to_string(),
                    ..Default::default()
                });
            }

            // Tree Consistency Rules (E012, E013)
            if action.parent_id.is_none() {
                let children: Vec<_> = doc.actions.iter()
                    .filter(|a| a.parent_id == Some(action.id))
                    .collect();

                if !children.is_empty() {
                    let all_children_completed = children.iter()
                        .all(|c| c.state == clearhead_cli::entities::ActionState::Completed);
                    let any_children_uncompleted = children.iter()
                        .any(|c| c.state != clearhead_cli::entities::ActionState::Completed);
                    let is_completed = action.state == clearhead_cli::entities::ActionState::Completed;

                    // Rule 2: Completed parent with uncompleted children (E012)
                    if is_completed && any_children_uncompleted {
                        diagnostics.push(Diagnostic {
                            range: source_range_to_lsp_range(metadata.root),
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: Some(NumberOrString::String("E012".to_string())),
                            source: Some("clearhead-lsp".to_string()),
                            message: "Parent is completed but some children are still active (E012)."
                                .to_string(),
                            ..Default::default()
                        });
                    }

                    // Rule 3: Uncompleted parent with all children completed (E013)
                    if !is_completed && all_children_completed {
                        diagnostics.push(Diagnostic {
                            range: source_range_to_lsp_range(metadata.root),
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: Some(NumberOrString::String("E013".to_string())),
                            source: Some("clearhead-lsp".to_string()),
                            message: "All children are completed. Should this parent be completed too? (E013)"
                                .to_string(),
                            ..Default::default()
                        });
                    }
                }
            }

            // Rule 4: Completed action missing date (E001)
            if action.state == clearhead_cli::entities::ActionState::Completed 
               && action.completed_date_time.is_none() 
            {
                diagnostics.push(Diagnostic {
                    range: source_range_to_lsp_range(metadata.root),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("E001".to_string())),
                    source: Some("clearhead-lsp".to_string()),
                    message: "Completed action is missing a completion date (E001).".to_string(),
                    ..Default::default()
                });
            }

            // Rule 5: Missing Creation Date (E014)
            if action.created_date_time.is_none() && metadata.is_id_generated {
                diagnostics.push(Diagnostic {
                    range: source_range_to_lsp_range(metadata.root),
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("E014".to_string())),
                    source: Some("clearhead-lsp".to_string()),
                    message: "Action is missing a creation date (E014).".to_string(),
                    ..Default::default()
                });
            }

            // Rule 6: Future Creation Date (E015)
            if let Some(created) = action.created_date_time {
                if created > Local::now() {
                    diagnostics.push(Diagnostic {
                        range: source_range_to_lsp_range(metadata.created_date.unwrap_or(metadata.root)),
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String("E015".to_string())),
                        source: Some("clearhead-lsp".to_string()),
                        message: "Creation date cannot be in the future (E015).".to_string(),
                        ..Default::default()
                    });
                }

                // Rule 7: Completion Before Creation (E016)
                if let Some(completed) = action.completed_date_time {
                    if completed < created {
                        diagnostics.push(Diagnostic {
                            range: source_range_to_lsp_range(metadata.completed_date.unwrap_or(metadata.root)),
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: Some(NumberOrString::String("E016".to_string())),
                            source: Some("clearhead-lsp".to_string()),
                            message: "Completion date cannot be before creation date (E016).".to_string(),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
    diagnostics
}

/// Compute code actions using the parsed document model
fn compute_code_actions(doc: &ParsedDocument, uri: &Uri, range: Range) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    // Check if cursor intersects with any action that needs hydration or completion date
    for action in &doc.actions {
        if let Some(metadata) = doc.source_map.get(&action.id) {
            let action_range = source_range_to_lsp_range(metadata.root);
            
            if range.start.line <= action_range.end.line 
               && range.end.line >= action_range.start.line 
            {
                // 1. Hydrate UUID
                if metadata.is_id_generated {
                    let uuid = Uuid::now_v7();
                    let new_text = format!(" #{}", uuid);
                    let insert_pos = source_range_to_lsp_range(metadata.line_range).end;

                    let mut changes = std::collections::HashMap::new();
                    changes.insert(
                        uri.clone(),
                        vec![TextEdit {
                            range: Range::new(insert_pos, insert_pos),
                            new_text,
                        }],
                    );

                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: "Hydrate Action (Add UUID)".to_string(),
                        kind: Some(CodeActionKind::QUICKFIX),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));
                }

                // 2. Add Completion Date
                if action.state == clearhead_cli::entities::ActionState::Completed 
                   && action.completed_date_time.is_none() 
                {
                    let now = Local::now();
                    let new_text = format!(" %{}", now.format("%Y-%m-%dT%H:%M"));
                    let insert_pos = source_range_to_lsp_range(metadata.line_range).end;

                    let mut changes = std::collections::HashMap::new();
                    changes.insert(
                        uri.clone(),
                        vec![TextEdit {
                            range: Range::new(insert_pos, insert_pos),
                            new_text,
                        }],
                    );

                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: "Set Completion Date (Today)".to_string(),
                        kind: Some(CodeActionKind::QUICKFIX),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));
                }

                // 3. Add Creation Date
                if action.created_date_time.is_none() {
                    // Option A: Set to today (for new actions)
                    let now = Local::now();
                    let new_text = format!(" ^{}", now.format("%Y-%m-%dT%H:%M"));
                    let insert_pos = source_range_to_lsp_range(metadata.line_range).end;

                    let mut changes = std::collections::HashMap::new();
                    changes.insert(
                        uri.clone(),
                        vec![TextEdit {
                            range: Range::new(insert_pos, insert_pos),
                            new_text,
                        }],
                    );

                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: "Set Creation Date (Today)".to_string(),
                        kind: Some(CodeActionKind::QUICKFIX),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));

                    // Option B: Derive from UUID (if not generated)
                    if !metadata.is_id_generated {
                        // Extract timestamp from UUID v7
                        let timestamp_ms = (action.id.as_u128() >> 80) as i64;
                        if let Some(dt) = DateTime::from_timestamp(timestamp_ms / 1000, ((timestamp_ms % 1000) * 1_000_000) as u32) {
                            let local_dt: DateTime<Local> = dt.into();
                            let new_text = format!(" ^{}", local_dt.format("%Y-%m-%dT%H:%M"));
                            
                            let mut changes = std::collections::HashMap::new();
                            changes.insert(
                                uri.clone(),
                                vec![TextEdit {
                                    range: Range::new(insert_pos, insert_pos),
                                    new_text,
                                }],
                            );

                            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                                title: "Derive Creation Date from UUID".to_string(),
                                kind: Some(CodeActionKind::QUICKFIX),
                                edit: Some(WorkspaceEdit {
                                    changes: Some(changes),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            }));
                        }
                    }
                }
            }
        }
    }
    actions
}

/// Compute inlay hints using the parsed document model
fn compute_inlay_hints(
    doc: &ParsedDocument,
    base_time: Option<DateTime<Local>>,
) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let now = base_time.unwrap_or_else(Local::now);

    for action in &doc.actions {
        if let Some(metadata) = doc.source_map.get(&action.id) {
            // Do Date Hint
            if let (Some(dt), Some(range)) = (action.do_date_time, metadata.do_date) {
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
            if let (Some(dt), Some(range)) = (action.completed_date_time, metadata.completed_date) {
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

/// Pure logic: Compute raw semantic tokens (not yet delta encoded)
fn compute_semantic_tokens(tree: &Tree) -> Vec<SemanticToken> {
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

impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "clearhead-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            legend: SemanticTokensLegend {
                                token_types: vec![
                                    SemanticTokenType::VARIABLE, // id
                                    SemanticTokenType::KEYWORD,  // priority, state
                                    SemanticTokenType::STRING,   // name, description
                                    SemanticTokenType::COMMENT,  // story
                                    SemanticTokenType::FUNCTION, // context/tags
                                    SemanticTokenType::MACRO,    // date
                                ],
                                token_modifiers: vec![],
                            },
                            ..Default::default()
                        },
                    ),
                ),
                inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
                    InlayHintOptions {
                        resolve_provider: Some(false),
                        ..Default::default()
                    },
                ))),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.update_document(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            self.update_document(params.text_document.uri, change.text)
                .await;
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            if let Some(ref parsed) = doc.parsed {
                let actions = compute_code_actions(parsed, &uri, params.range);
                return Ok(Some(actions));
            }
        }
        Ok(None)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            let tokens = compute_semantic_tokens(&doc.tree);

            // Convert to relative (delta) encoding
            let mut last_line = 0;
            let mut last_start = 0;
            let mut data = Vec::new();

            for token in tokens {
                let delta_line = token.delta_line - last_line;
                let delta_start = if delta_line == 0 {
                    token.delta_start - last_start
                } else {
                    token.delta_start
                };

                data.push(SemanticToken {
                    delta_line,
                    delta_start,
                    length: token.length,
                    token_type: token.token_type,
                    token_modifiers_bitset: 0,
                });

                last_line = token.delta_line;
                last_start = token.delta_start;
            }

            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data,
            })));
        }
        Ok(None)
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            if let Some(ref parsed) = doc.parsed {
                let hints = compute_inlay_hints(parsed, None);
                return Ok(Some(hints));
            }
        }
        Ok(None)
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            let position = params.text_document_position_params.position;
            if let Some(node) = get_node_at_position(&doc.tree, position) {
                // We only handle jumping for stories and contexts
                if node.kind() == "story" || node.kind() == "context" {
                    let tag_text = get_node_text(&node, &doc.text);
                    if let Some(ref parsed) = doc.parsed {
                        if let Some(ranges) = parsed.tag_index.get(&tag_text) {
                            // For "Definition", we jump to the *first* occurrence
                            if let Some(first_range) = ranges.first() {
                                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                                    uri: uri.clone(),
                                    range: source_range_to_lsp_range(*first_range),
                                })));
                            }
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            let position = params.text_document_position.position;
            if let Some(node) = get_node_at_position(&doc.tree, position) {
                if node.kind() == "story" || node.kind() == "context" {
                    let tag_text = get_node_text(&node, &doc.text);
                    if let Some(ref parsed) = doc.parsed {
                        if let Some(ranges) = parsed.tag_index.get(&tag_text) {
                            let locations = ranges.iter().map(|r| Location {
                                uri: uri.clone(),
                                range: source_range_to_lsp_range(*r),
                            }).collect();
                            return Ok(Some(locations));
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            if let Some(ref parsed) = doc.parsed {
                // Use default config for now, or derive from params.options (tab_size etc)
                let config = FormatConfig {
                    indent_width: params.options.tab_size as usize,
                    ..Default::default()
                };

                match format(&parsed.actions, OutputFormat::Actions, Some(config)) {
                    Ok(formatted_text) => {
                        // Replace the entire document
                        let start = Position::new(0, 0);
                        // To be safe, finding the end of the document is tricky without iterating lines
                        // But we can use the tree root range
                        let root = doc.tree.root_node();
                        let end = Position::new(
                            root.end_position().row as u32,
                            root.end_position().column as u32,
                        );

                        return Ok(Some(vec![TextEdit {
                            range: Range::new(start, end),
                            new_text: formatted_text,
                        }]));
                    }
                    Err(e) => {
                        self.client
                            .log_message(MessageType::ERROR, format!("Formatting failed: {}", e))
                            .await;
                        return Ok(None);
                    }
                }
            }
        }
        Ok(None)
    }
}

pub async fn start_lsp() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: DashMap::new(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

fn get_node_at_position(tree: &Tree, position: Position) -> Option<tree_sitter::Node<'_>> {
    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    tree.root_node()
        .named_descendant_for_point_range(point, point)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests need updating because compute_diagnostics now takes ParsedDocument
    // We can rely on get_parsed_document in tests.

    #[test]
    fn test_diagnostics_missing_id() {
        let text = "[ ] This action has no ID";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = compute_diagnostics(&parsed);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(diagnostics[0].message.contains("missing a UUID"));
    }

    #[test]
    fn test_diagnostics_has_id() {
        let text = "[ ] This action has an ID #01942d99-4c27-77f6-9316-107024843939";
        let parsed = get_parsed_document(text).unwrap();

        let diagnostics = compute_diagnostics(&parsed);
        assert_eq!(diagnostics.len(), 0);
    }
}
