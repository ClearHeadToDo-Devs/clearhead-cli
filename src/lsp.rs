use chrono::{DateTime, Local};
use dashmap::DashMap;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};
use tree_sitter::{Parser, Tree};
use uuid::Uuid;

use clearhead_cli::entities::parse_iso8601_datetime;
use clearhead_cli::treesitter::get_node_text;
use clearhead_cli::format::{format_with_topiary, FormatConfig};

#[derive(Debug)]
struct DocumentState {
    text: String,
    tree: Tree,
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
            self.documents.insert(uri.clone(), DocumentState { text: text.clone(), tree: tree.clone() });
            let diagnostics = compute_diagnostics(&tree);
            self.client.publish_diagnostics(uri, diagnostics, None).await;
        }
    }
}

/// Pure logic: Compute diagnostics (e.g. missing IDs) from the tree
fn compute_diagnostics(tree: &Tree) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut cursor = tree.walk();
    let mut nodes_to_check = vec![tree.root_node()];

    while let Some(node) = nodes_to_check.pop() {
        if node.kind().contains("action")
            && !node.kind().contains("marker")
            && !node.kind().contains("list")
        {
            let has_id = node
                .children_by_field_name("metadata", &mut node.walk())
                .any(|child| child.kind() == "id");

            if !has_id {
                let range = Range {
                    start: Position::new(
                        node.start_position().row as u32,
                        node.start_position().column as u32,
                    ),
                    end: Position::new(
                        node.end_position().row as u32,
                        node.end_position().column as u32,
                    ),
                };

                diagnostics.push(Diagnostic {
                    range,
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: Some(NumberOrString::String("missing-id".to_string())),
                    source: Some("clearhead-lsp".to_string()),
                    message: "Action is missing a UUID. Use 'Hydrate Action' to add one.".to_string(),
                    ..Default::default()
                });
            }
        }

        for child in node.children(&mut cursor) {
            nodes_to_check.push(child);
        }
    }
    diagnostics
}

/// Pure logic: Compute code actions for a specific range
fn compute_code_actions(tree: &Tree, uri: &Uri, range: Range) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();
    let mut cursor = tree.walk();
    let mut nodes_to_check = vec![tree.root_node()];

    while let Some(node) = nodes_to_check.pop() {
        // Check if this node is an action (root_action, depth1_action, etc.)
        if node.kind().contains("action")
            && !node.kind().contains("marker")
            && !node.kind().contains("list")
        {
            // Check if it has an 'id' metadata child
            let has_id = node
                .children_by_field_name("metadata", &mut node.walk())
                .any(|child: tree_sitter::Node| child.kind() == "id");

            if !has_id {
                let node_range = Range {
                    start: Position::new(
                        node.start_position().row as u32,
                        node.start_position().column as u32,
                    ),
                    end: Position::new(
                        node.end_position().row as u32,
                        node.end_position().column as u32,
                    ),
                };

                // Only suggest if the cursor is within/near the action
                if node_range.start.line <= range.start.line
                    && node_range.end.line >= range.end.line
                {
                    let uuid = Uuid::now_v7();
                    let new_text = format!(" #{}", uuid);

                    // Insert at the end of the action body (before children)
                    let insert_pos = Position::new(
                        node.start_position().row as u32,
                        node.end_position().column as u32,
                    );

                    let mut changes = std::collections::HashMap::new();
                    changes.insert(
                        uri.clone(),
                        vec![TextEdit {
                            range: Range::new(insert_pos, insert_pos),
                            new_text,
                        }],
                    );

                    let action = CodeAction {
                        title: "Hydrate Action (Add UUID)".to_string(),
                        kind: Some(CodeActionKind::QUICKFIX),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        ..Default::default()
                    };
                    actions.push(CodeActionOrCommand::CodeAction(action));
                }
            }
        }

        // Add children to queue
        for child in node.children(&mut cursor) {
            nodes_to_check.push(child);
        }
    }
    actions
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

/// Pure logic: Compute inlay hints
fn compute_inlay_hints(tree: &Tree, source_text: &str, base_time: Option<DateTime<Local>>) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    let mut cursor = tree.walk();
    let mut nodes_to_check = vec![tree.root_node()];

    let now = base_time.unwrap_or_else(Local::now);

    while let Some(node) = nodes_to_check.pop() {
        if node.kind() == "do_date" || node.kind() == "completed_date" {
            if let Some(datetime_node) = node.child_by_field_name("datetime") {
                let date_str = get_node_text(&datetime_node, source_text);
                if let Some(dt) = parse_iso8601_datetime(&date_str) {
                    let label = if node.kind() == "do_date" {
                        let diff = dt.signed_duration_since(now);
                        if diff.num_days() > 0 {
                            format!(" (due in {}d)", diff.num_days())
                        } else if diff.num_days() < 0 {
                            format!(" ({}d ago)", -diff.num_days())
                        } else {
                            " (due today)".to_string()
                        }
                    } else {
                        let diff = now.signed_duration_since(dt);
                        format!(" (done {}d ago)", diff.num_days())
                    };

                    hints.push(InlayHint {
                        position: Position::new(
                            node.end_position().row as u32,
                            node.end_position().column as u32,
                        ),
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

        for child in node.children(&mut cursor) {
            nodes_to_check.push(child);
        }
    }
    hints
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
            let actions = compute_code_actions(&doc.tree, &uri, params.range);
            return Ok(Some(actions));
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
            let hints = compute_inlay_hints(&doc.tree, &doc.text, None);
            return Ok(Some(hints));
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
                    let locations = compute_tag_references(&doc.tree, &doc.text, &uri, node.kind(), &tag_text);
                    
                    // For "Definition", we jump to the *first* occurrence
                    if let Some(first_loc) = locations.first() {
                         return Ok(Some(GotoDefinitionResponse::Scalar(first_loc.clone())));
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
                    let locations = compute_tag_references(&doc.tree, &doc.text, &uri, node.kind(), &tag_text);
                    return Ok(Some(locations));
                }
            }
        }
        Ok(None)
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            // Use default config for now, or derive from params.options (tab_size etc)
            // But FormatConfig struct is specific to our app logic (include_id etc)
            let config = FormatConfig {
                indent_width: params.options.tab_size as usize,
                ..Default::default()
            };

            match format_with_topiary(&doc.text, &config) {
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
                },
                Err(e) => {
                     self.client.log_message(MessageType::ERROR, format!("Formatting failed: {}", e)).await;
                     return Ok(None);
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
        documents: DashMap::new() 
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

fn get_node_at_position(tree: &Tree, position: Position) -> Option<tree_sitter::Node<'_>> {
    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    tree.root_node()
        .named_descendant_for_point_range(point, point)
}

/// Pure logic: Find all locations of a specific tag (story/context)
fn compute_tag_references(tree: &Tree, source_text: &str, uri: &Uri, target_kind: &str, target_text: &str) -> Vec<Location> {
    let mut locations = Vec::new();
    let mut cursor = tree.walk();
    let mut nodes_to_check = vec![tree.root_node()];

    while let Some(node) = nodes_to_check.pop() {
        if node.kind() == target_kind {
             let current_text = get_node_text(&node, source_text);
             if current_text == target_text {
                 let range = Range {
                    start: Position::new(
                        node.start_position().row as u32,
                        node.start_position().column as u32,
                    ),
                    end: Position::new(
                        node.end_position().row as u32,
                        node.end_position().column as u32,
                    ),
                };
                locations.push(Location {
                    uri: uri.clone(),
                    range,
                });
             }
        }

        for child in node.children(&mut cursor) {
            nodes_to_check.push(child);
        }
    }
    
    // Sort by position
    locations.sort_by(|a, b| {
         if a.range.start.line != b.range.start.line {
             a.range.start.line.cmp(&b.range.start.line)
         } else {
             a.range.start.character.cmp(&b.range.start.character)
         }
    });
    
    locations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_actions::LANGUAGE.into())
            .expect("Error loading actions grammar");
        parser
    }

    #[test]
    fn test_diagnostics_missing_id() {
        let text = "[ ] This action has no ID";
        let mut parser = get_parser();
        let tree = parser.parse(text, None).unwrap();
        
        let diagnostics = compute_diagnostics(&tree);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(diagnostics[0].message.contains("missing a UUID"));
    }

    #[test]
    fn test_diagnostics_has_id() {
        let text = "[ ] This action has an ID #01942d99-4c27-77f6-9316-107024843939";
        let mut parser = get_parser();
        let tree = parser.parse(text, None).unwrap();
        
        let diagnostics = compute_diagnostics(&tree);
        assert_eq!(diagnostics.len(), 0);
    }

    #[test]
    fn test_compute_tag_references() {
        use std::str::FromStr;
        let text = "[ ] Action 1 *MyStory\n[ ] Action 2 *MyStory\n[ ] Action 3 *OtherStory";
        let mut parser = get_parser();
        let tree = parser.parse(text, None).unwrap();
        let uri = Uri::from_str("file:///test.actions").unwrap();
        
        // Search for *MyStory
        // *MyStory is at line 0, col 13 and line 1, col 13
        // Note: the tree-sitter node includes the '*' prefix usually, let's verify if get_node_text returns "*MyStory"
        // Yes, get_node_text returns the full text range of the node.
        
        let refs = compute_tag_references(&tree, text, &uri, "story", "*MyStory");
        
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].range.start.line, 0);
        assert_eq!(refs[1].range.start.line, 1);
        
        // Search for *OtherStory
        let refs_other = compute_tag_references(&tree, text, &uri, "story", "*OtherStory");
        assert_eq!(refs_other.len(), 1);
        assert_eq!(refs_other[0].range.start.line, 2);
    }
}
