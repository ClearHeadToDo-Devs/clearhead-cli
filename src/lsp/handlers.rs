use chrono::Local;
use clearhead_cli::archive::archive_actions;
use clearhead_cli::telemetry::{
    TelemetryEvent, Tool, emit_event, event_from_field_change, event_from_state_change,
};
use clearhead_cli::{FormatConfig, OutputFormat, format};
use clearhead_core::workspace::actions::{FieldChange, diff_actions, get_node_text};
use serde_json::Value;
use tower_lsp_server::LanguageServer;
use tower_lsp_server::jsonrpc::{Error, Result};
use tower_lsp_server::ls_types::*;
use tracing::{debug, info, warn};

use super::Backend;
use super::providers::*;

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
                    commands: vec![
                        "clearhead/archive".to_string(),
                        "clearhead/forceSync".to_string(),
                    ],
                    ..Default::default()
                }),
                ..Default::default()
            },
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
        let file_path = uri.to_file_path().map(|p| p.to_string_lossy().to_string());

        debug!(uri = ?uri, "Processing didSave notification");

        if let Some(mut doc) = self.documents.get_mut(&uri) {
            // Diff existing parsed vs last_saved
            if let (Some(current), Some(last)) = (&doc.parsed, &doc.last_saved_parsed) {
                let diff = diff_actions(&last.actions, &current.actions);

                if !diff.is_empty() {
                    info!(
                        uri = ?uri,
                        added = diff.added.len(),
                        removed = diff.removed.len(),
                        modified = diff.modified.len(),
                        "Changes detected on save"
                    );
                }

                let event_file_path = file_path.clone().unwrap_or_default();

                // Emit events for added actions
                for action in &diff.added {
                    debug!(id = %action.id, name = %action.name, "Emitting action_created event");
                    if let Err(e) = emit_event(
                        Tool::Lsp,
                        Some(action.id.to_string()),
                        TelemetryEvent::ActionCreated {
                            name: action.name.clone(),
                            file_path: event_file_path.clone(),
                        },
                    ) {
                        warn!(error = %e, "Failed to emit action_created event");
                    }
                }

                // Emit events for removed actions
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

                // Emit events for modified actions
                for mod_action in &diff.modified {
                    let action_uuid = Some(mod_action.id.to_string());

                    let action_name = current
                        .actions
                        .iter()
                        .find(|a| a.id == mod_action.id)
                        .map(|a| a.name.as_str())
                        .unwrap_or("");

                    for change in &mod_action.changes {
                        let event = match change {
                            FieldChange::State { old, new } => {
                                debug!(id = %mod_action.id, old = ?old, new = ?new, "Emitting state change event");
                                event_from_state_change(*old, *new, action_name)
                            }
                            _ => event_from_field_change(change),
                        };
                        if let Some(evt) = event {
                            if let Err(e) = emit_event(Tool::Lsp, action_uuid.clone(), evt) {
                                warn!(error = %e, "Failed to emit property change event");
                            }
                        }
                    }
                }
            }

            // Sync to CRDT (source of truth) - only for managed workspace files
            if let Some(path_str) = &file_path {
                self.sync_to_crdt_and_apply_edit(&uri, path_str, &doc.text)
                    .await;
            }

            // Update last_saved_parsed to current
            doc.last_saved_parsed = doc.parsed.clone();
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
                    let char_before = line.chars().nth(char_idx - 1);

                    if let Some(c) = char_before {
                        match c {
                            '@' | '%' | '^' => {
                                let now = Local::now();
                                let mut items = Vec::new();

                                let make_item = |label: String, detail: &str| -> CompletionItem {
                                    CompletionItem {
                                        label: label.clone(),
                                        kind: Some(CompletionItemKind::VALUE),
                                        detail: Some(detail.to_string()),
                                        insert_text: Some(label),
                                        ..Default::default()
                                    }
                                };

                                let now_str = now.format("%Y-%m-%dT%H:%M").to_string();
                                items.push(make_item(now_str, "Now"));

                                let today_str = now.format("%Y-%m-%d").to_string();
                                items.push(make_item(today_str, "Today"));

                                let tomorrow = now + chrono::Duration::days(1);
                                let tomorrow_str = tomorrow.format("%Y-%m-%d").to_string();
                                items.push(make_item(tomorrow_str, "Tomorrow"));

                                return Ok(Some(CompletionResponse::Array(items)));
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
                    Ok(formatted_text) => {
                        let start = Position::new(0, 0);
                        let end = Position::new(u32::MAX, 0);

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

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        match params.command.as_str() {
            "clearhead/archive" => {
                if let Some(uri_val) = params.arguments.first() {
                    if let Ok(uri) = serde_json::from_value::<Uri>(uri_val.clone()) {
                        if let Some(doc) = self.documents.get(&uri) {
                            let source_path = uri
                                .to_file_path()
                                .ok_or_else(|| Error::invalid_params("Invalid URI"))?;
                            let log_dir = source_path.parent().unwrap().join("logs");

                            let _ = std::fs::create_dir_all(&log_dir);

                            match archive_actions(&doc.text, &source_path.to_path_buf(), &log_dir) {
                                Ok((new_content, result)) => {
                                    let edit = WorkspaceEdit {
                                        changes: Some({
                                            let mut map = std::collections::HashMap::new();
                                            map.insert(
                                                uri.clone(),
                                                vec![TextEdit {
                                                    range: Range::new(
                                                        Position::new(0, 0),
                                                        Position::new(u32::MAX, 0),
                                                    ),
                                                    new_text: new_content,
                                                }],
                                            );
                                            map
                                        }),
                                        ..Default::default()
                                    };

                                    self.client.apply_edit(edit).await.map_err(|e| {
                                        let err = format!("Failed to apply edit: {}", e);
                                        Error {
                                            code:
                                                tower_lsp_server::jsonrpc::ErrorCode::InternalError,
                                            message: err.into(),
                                            data: None,
                                        }
                                    })?;

                                    self.client
                                        .show_message(
                                            MessageType::INFO,
                                            format!(
                                                "Archived {} actions to {}",
                                                result.archived_count,
                                                result.log_path.display()
                                            ),
                                        )
                                        .await;
                                }
                                Err(e) => {
                                    self.client
                                        .show_message(
                                            MessageType::WARNING,
                                            format!("Archive failed: {}", e),
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                }
                Ok(None)
            }
            "clearhead/forceSync" => {
                if let Some(uri_val) = params.arguments.first() {
                    if let Ok(uri) = serde_json::from_value::<Uri>(uri_val.clone()) {
                        let source_path = uri
                            .to_file_path()
                            .ok_or_else(|| Error::invalid_params("Invalid URI"))?;

                        use clearhead_cli::crdt::load_action_repo;
                        use clearhead_core::workspace::actions::convert;
                        match load_action_repo(&source_path) {
                            Ok(repo) => match repo.get_model().map(|m| convert::to_action_list(&m)) {
                                Ok(actions) => {
                                    use clearhead_cli::{OutputFormat, format};
                                    match format(&actions, OutputFormat::Actions, None, None) {
                                        Ok(formatted_content) => {
                                            let edit = WorkspaceEdit {
                                                changes: Some({
                                                    let mut map = std::collections::HashMap::new();
                                                    map.insert(
                                                        uri.clone(),
                                                        vec![TextEdit {
                                                            range: Range::new(
                                                                Position::new(0, 0),
                                                                Position::new(u32::MAX, 0),
                                                            ),
                                                            new_text: formatted_content,
                                                        }],
                                                    );
                                                    map
                                                }),
                                                ..Default::default()
                                            };

                                            self.client.apply_edit(edit).await.map_err(|e| {
                                                    let err = format!("Failed to apply edit: {}", e);
                                                    Error {
                                                        code: tower_lsp_server::jsonrpc::ErrorCode::InternalError,
                                                        message: err.into(),
                                                        data: None,
                                                    }
                                                })?;

                                            self.client
                                                .show_message(
                                                    MessageType::INFO,
                                                    "Buffer synced with CRDT state".to_string(),
                                                )
                                                .await;
                                        }
                                        Err(e) => {
                                            self.client
                                                .show_message(
                                                    MessageType::ERROR,
                                                    format!("Format failed: {}", e),
                                                )
                                                .await;
                                        }
                                    }
                                }
                                Err(e) => {
                                    self.client
                                        .show_message(
                                            MessageType::ERROR,
                                            format!("Failed to get actions from CRDT: {}", e),
                                        )
                                        .await;
                                }
                            },
                            Err(e) if e.contains("outside managed workspace") => {
                                use clearhead_cli::environment_reader::get_data_dir;
                                self.client
                                    .show_message(
                                        MessageType::WARNING,
                                        format!(
                                            "File is outside managed workspace.\n\
                                         CRDT sync only available for files in: {}",
                                            get_data_dir().display()
                                        ),
                                    )
                                    .await;
                            }
                            Err(e) => {
                                self.client
                                    .show_message(
                                        MessageType::ERROR,
                                        format!("Failed to load CRDT: {}", e),
                                    )
                                    .await;
                            }
                        }
                    }
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}
