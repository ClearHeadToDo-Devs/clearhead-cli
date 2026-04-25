use std::collections::HashMap;

use chrono::Local;
use clearhead_cli::archive::archive_actions;
use clearhead_cli::telemetry::{
    TelemetryEvent, Tool, emit_event, event_from_field_change, event_from_state_change,
};
use clearhead_cli::{FormatConfig, OutputFormat, ParsedDocument, format};
use clearhead_core::workspace::actions::{Diff, FieldChange, diff_actions, get_node_text};
use serde_json::Value;
use tower_lsp_server::LanguageServer;
use tower_lsp_server::jsonrpc::{Error, Result};
use tower_lsp_server::ls_types::*;
use tracing::{debug, info, warn};

use super::Backend;
use super::providers::*;

// =============================================================================
// LanguageServer trait — entry points (what the LSP does)
// =============================================================================

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
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "@".to_string(),
                        "%".to_string(),
                        "^".to_string(),
                    ]),
                    ..Default::default()
                }),
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
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["clearhead/archive".to_string()],
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.update_document(params.text_document.uri, params.text_document.text, true)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            self.update_document(params.text_document.uri, change.text, false)
                .await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        debug!(uri = ?uri, "Processing didSave notification");

        if let Some(mut doc) = self.documents.get_mut(&uri) {
            if let (Some(current), Some(last)) = (&doc.parsed, &doc.last_saved_parsed) {
                let diff = diff_actions(&last.actions, &current.actions);
                let file_path = uri
                    .to_file_path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                emit_diff_telemetry(&diff, current, &file_path);
            }
            doc.last_saved_parsed = doc.parsed.clone();
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        if let Some(doc) = self.documents.get(&uri) {
            if let Some(ref parsed) = doc.parsed {
                return Ok(Some(compute_code_actions(parsed, &uri, params.range)));
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
            let mut last_line = 0;
            let mut last_start = 0;
            let data = tokens
                .into_iter()
                .map(|token| {
                    let delta_line = token.delta_line - last_line;
                    let delta_start = if delta_line == 0 {
                        token.delta_start - last_start
                    } else {
                        token.delta_start
                    };
                    last_line = token.delta_line;
                    last_start = token.delta_start;
                    SemanticToken {
                        delta_line,
                        delta_start,
                        length: token.length,
                        token_type: token.token_type,
                        token_modifiers_bitset: 0,
                    }
                })
                .collect();

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
                return Ok(Some(compute_inlay_hints(parsed, None)));
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
                if node.kind() == "story" || node.kind() == "context" {
                    let tag_text = get_node_text(&node, &doc.text);
                    if let Some(ref parsed) = doc.parsed {
                        if let Some(ranges) = parsed.tag_index.get(&tag_text) {
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
                            let locations = ranges
                                .iter()
                                .map(|r| Location {
                                    uri: uri.clone(),
                                    range: source_range_to_lsp_range(*r),
                                })
                                .collect();
                            return Ok(Some(locations));
                        }
                    }
                }
            }
        }
        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        if let Some(doc) = self.documents.get(&uri) {
            let line_idx = position.line as usize;
            let lines: Vec<&str> = doc.text.lines().collect();

            if line_idx < lines.len() {
                let line = lines[line_idx];
                let char_idx = position.character as usize;

                if char_idx > 0 {
                    if let Some(c) = line.chars().nth(char_idx - 1) {
                        match c {
                            '@' | '%' | '^' => {
                                return Ok(Some(CompletionResponse::Array(date_completion_items(
                                    Local::now(),
                                ))));
                            }
                            _ => {}
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
                let config = FormatConfig {
                    indent_width: params.options.tab_size as usize,
                    ..Default::default()
                };
                match format(&parsed.actions, OutputFormat::Actions, Some(config), None) {
                    Ok(new_text) => {
                        return Ok(Some(vec![full_replace_text_edit(new_text)]));
                    }
                    Err(e) => {
                        self.client
                            .log_message(MessageType::ERROR, format!("Formatting failed: {e}"))
                            .await;
                        return Ok(None);
                    }
                }
            }
        }
        Ok(None)
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        match params.command.as_str() {
            "clearhead/archive" => self.cmd_archive(params.arguments).await,
            _ => Ok(None),
        }
    }
}

// =============================================================================
// Backend command implementations — one function per command
// =============================================================================

impl Backend {
    async fn cmd_archive(&self, args: Vec<Value>) -> Result<Option<Value>> {
        let uri_val = args
            .first()
            .ok_or_else(|| Error::invalid_params("Missing URI argument"))?;
        let uri = serde_json::from_value::<Uri>(uri_val.clone())
            .map_err(|e| Error::invalid_params(format!("Invalid URI: {e}")))?;

        let source_path = uri
            .to_file_path()
            .ok_or_else(|| Error::invalid_params("URI is not a file path"))?
            .to_path_buf();

        // Drop the DashMap guard before any .await points
        let content = {
            self.documents
                .get(&uri)
                .ok_or_else(|| Error::invalid_params("Document not found in LSP state"))?
                .text
                .clone()
        };

        // Run blocking file I/O on a dedicated thread so the async executor stays
        // free to process the workspace/applyEdit round-trip with the client
        let result = tokio::task::spawn_blocking(move || archive_actions(&content, &source_path))
            .await
            .map_err(|e| internal_error(format!("Archive task panicked: {e}")))?;

        match result {
            Ok((new_content, archive_result)) => {
                self.client
                    .apply_edit(full_replace_workspace_edit(uri, new_content))
                    .await
                    .map_err(|e| internal_error(format!("Failed to apply edit: {e}")))?;
                self.client
                    .show_message(
                        MessageType::INFO,
                        format!(
                            "Archived {} actions to {}",
                            archive_result.archived_count,
                            archive_result.completed_path.display()
                        ),
                    )
                    .await;
            }
            Err(e) => {
                self.client
                    .show_message(MessageType::WARNING, format!("Archive failed: {e}"))
                    .await;
            }
        }

        Ok(None)
    }
}

// =============================================================================
// Helpers — small, stateless utilities used above
// =============================================================================

/// A WorkspaceEdit that replaces the entire content of a document.
fn full_replace_workspace_edit(uri: Uri, text: String) -> WorkspaceEdit {
    WorkspaceEdit {
        changes: Some(HashMap::from([(uri, vec![full_replace_text_edit(text)])])),
        ..Default::default()
    }
}

/// A TextEdit that replaces the entire content of a document.
fn full_replace_text_edit(text: String) -> TextEdit {
    TextEdit {
        range: Range::new(Position::new(0, 0), Position::new(u32::MAX, 0)),
        new_text: text,
    }
}

fn internal_error(msg: impl std::fmt::Display) -> Error {
    Error {
        code: tower_lsp_server::jsonrpc::ErrorCode::InternalError,
        message: msg.to_string().into(),
        data: None,
    }
}

fn date_completion_items(now: chrono::DateTime<Local>) -> Vec<CompletionItem> {
    let make_item = |label: String, detail: &str| CompletionItem {
        label: label.clone(),
        kind: Some(CompletionItemKind::VALUE),
        detail: Some(detail.to_string()),
        insert_text: Some(label),
        ..Default::default()
    };

    vec![
        make_item(now.format("%Y-%m-%dT%H:%M").to_string(), "Now"),
        make_item(now.format("%Y-%m-%d").to_string(), "Today"),
        make_item(
            (now + chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string(),
            "Tomorrow",
        ),
    ]
}

fn emit_diff_telemetry(diff: &Diff, current: &ParsedDocument, file_path: &str) {
    if !diff.is_empty() {
        info!(
            added = diff.added.len(),
            removed = diff.removed.len(),
            modified = diff.modified.len(),
            "Changes detected on save"
        );
    }

    for action in &diff.added {
        debug!(id = %action.id, name = %action.name, "Emitting action_created event");
        if let Err(e) = emit_event(
            Tool::Lsp,
            Some(action.id.to_string()),
            TelemetryEvent::ActionCreated {
                name: action.name.clone(),
                file_path: file_path.to_string(),
            },
        ) {
            warn!(error = %e, "Failed to emit action_created event");
        }
    }

    for action in &diff.removed {
        debug!(id = %action.id, name = %action.name, "Emitting action_deleted event");
        if let Err(e) = emit_event(
            Tool::Lsp,
            Some(action.id.to_string()),
            TelemetryEvent::ActionDeleted {
                name: action.name.clone(),
            },
        ) {
            warn!(error = %e, "Failed to emit action_deleted event");
        }
    }

    for mod_action in &diff.modified {
        let id = Some(mod_action.id.to_string());
        let name = current
            .actions
            .iter()
            .find(|a| a.id == mod_action.id)
            .map(|a| a.name.as_str())
            .unwrap_or("");

        for change in &mod_action.changes {
            let event = match change {
                FieldChange::State { old, new } => {
                    debug!(id = %mod_action.id, old = ?old, new = ?new, "Emitting state change event");
                    event_from_state_change(*old, *new, name)
                }
                _ => event_from_field_change(change),
            };
            if let Some(evt) = event {
                if let Err(e) = emit_event(Tool::Lsp, id.clone(), evt) {
                    warn!(error = %e, "Failed to emit property change event");
                }
            }
        }
    }
}
