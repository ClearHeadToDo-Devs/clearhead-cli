use std::collections::HashMap;

use chrono::Local;
use clearhead_cli::archive::archive_actions;
use clearhead_cli::telemetry::{
    TelemetryEvent, Tool, emit_event, event_from_field_change, event_from_state_change,
};
use clearhead_cli::{FormatConfig, OutputFormat, ParsedDocument, format};
use clearhead_core::workspace::actions::{Diff, FieldChange, diff_actions, get_node_text};
use clearhead_core::{ArchiveCharterOptions, archive_charter, workspace::check_for_workspace};
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
                    commands: vec![
                        "clearhead/archive".to_string(),
                        "clearhead/archiveCharter".to_string(),
                    ],
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

        // Snapshot diff for telemetry and check whether any actions completed
        let any_completed = if let Some(mut doc) = self.documents.get_mut(&uri) {
            let any = if let (Some(current), Some(last)) = (&doc.parsed, &doc.last_saved_parsed) {
                let diff = diff_actions(&last.actions, &current.actions);
                let file_path = uri
                    .to_file_path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                emit_diff_telemetry(&diff, current, &file_path);
                // Check if any state transition to Completed occurred
                diff.modified.iter().any(|m| {
                    m.changes.iter().any(|c| matches!(
                        c,
                        clearhead_core::workspace::actions::FieldChange::State {
                            new: clearhead_core::workspace::actions::ActionState::Completed,
                            ..
                        }
                    ))
                })
            } else {
                false
            };
            doc.last_saved_parsed = doc.parsed.clone();
            any
        } else {
            false
        };

        // Auto-archive: if any actions just completed, sweep finished trees out of the buffer
        if any_completed {
            if let Some(doc) = self.documents.get(&uri) {
                let content = doc.text.clone();
                drop(doc); // release lock before spawn_blocking
                let source_path = uri
                    .to_file_path()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_default();
                let result =
                    tokio::task::spawn_blocking(move || archive_actions(&content, &source_path))
                        .await;
                match result {
                    Ok(Ok((new_content, res))) if res.archived_count > 0 => {
                        if let Err(e) = self
                            .client
                            .apply_edit(full_replace_workspace_edit(uri.clone(), new_content))
                            .await
                        {
                            warn!(error = %e, "Auto-archive: failed to apply workspace edit");
                        } else {
                            info!(count = res.archived_count, "Auto-archived completed actions on save");
                            self.client
                                .show_message(
                                    MessageType::INFO,
                                    format!(
                                        "Auto-archived {} completed action(s) to {}",
                                        res.archived_count,
                                        res.completed_path.display()
                                    ),
                                )
                                .await;
                        }
                    }
                    Ok(Ok(_)) => {} // nothing to archive
                    Ok(Err(e)) => debug!("Auto-archive skipped: {}", e),
                    Err(e) => warn!("Auto-archive task panicked: {}", e),
                }
            }
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
        let position = params.text_document_position_params.position;

        let (node_kind, ref_text, same_file_range) = {
            let Some(doc) = self.documents.get(&uri) else {
                return Ok(None);
            };
            let Some(node) = get_node_at_position(&doc.tree, position) else {
                return Ok(None);
            };
            let kind = node.kind().to_string();
            let text = get_node_text(&node, &doc.text);

            // Same-file fast path: tags and UUID id nodes resolved from in-memory state
            let same_file = match kind.as_str() {
                // Context/story tags — existing behaviour
                "story" | "context" => doc
                    .parsed
                    .as_ref()
                    .and_then(|p| p.tag_index.get(&text))
                    .and_then(|ranges| ranges.first().copied())
                    .map(source_range_to_lsp_range),
                // The `#uuid` id field — jump to the action itself (same file)
                "uuid_value" => doc
                    .parsed
                    .as_ref()
                    .and_then(|p| {
                        uuid::Uuid::parse_str(&text)
                            .ok()
                            .and_then(|id| p.source_map.get(&id))
                    })
                    .map(|m| source_range_to_lsp_range(m.root)),
                _ => None,
            };

            (kind, text, same_file)
        };

        if let Some(range) = same_file_range {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: uri.clone(),
                range,
            })));
        }

        // Cross-file path: predecessor references, alias lookups, short UUIDs
        let needs_workspace_lookup = matches!(
            node_kind.as_str(),
            "uuid_value" | "short_uuid_value" | "predecessor_reference"
                | "predecessor_name" | "alias_name"
        );

        if needs_workspace_lookup {
            let source_path = uri
                .to_file_path()
                .ok_or_else(|| internal_error("URI is not a file path"))?
                .to_path_buf();

            let result = tokio::task::spawn_blocking(move || {
                find_definition_in_workspace(&source_path, &ref_text)
            })
            .await
            .map_err(|e| internal_error(format!("Definition lookup panicked: {e}")))?
            ;

            if let Some((file_path, range)) = result {
                let target_uri = Uri::from_file_path(&file_path)
                    .ok_or_else(|| internal_error("Could not build URI from path"))?;
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: target_uri,
                    range,
                })));
            }
        }

        Ok(None)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let (node_kind, ref_text, same_file_locations) = {
            let Some(doc) = self.documents.get(&uri) else {
                return Ok(None);
            };
            let Some(node) = get_node_at_position(&doc.tree, position) else {
                return Ok(None);
            };
            let kind = node.kind().to_string();
            let text = get_node_text(&node, &doc.text);

            // Same-file: tags/contexts — existing behaviour
            let same_file = match kind.as_str() {
                "story" | "context" => doc
                    .parsed
                    .as_ref()
                    .and_then(|p| p.tag_index.get(&text))
                    .map(|ranges| {
                        ranges
                            .iter()
                            .map(|r| Location {
                                uri: uri.clone(),
                                range: source_range_to_lsp_range(*r),
                            })
                            .collect::<Vec<_>>()
                    }),
                _ => None,
            };

            (kind, text, same_file)
        };

        if let Some(locs) = same_file_locations {
            return Ok(Some(locs));
        }

        // Cross-file: find all predecessor references to this UUID across the workspace
        let needs_workspace_lookup =
            matches!(node_kind.as_str(), "uuid_value" | "short_uuid_value");

        if needs_workspace_lookup {
            let source_path = uri
                .to_file_path()
                .ok_or_else(|| internal_error("URI is not a file path"))?
                .to_path_buf();

            let result = tokio::task::spawn_blocking(move || {
                find_references_in_workspace(&source_path, &ref_text)
            })
            .await
            .map_err(|e| internal_error(format!("References lookup panicked: {e}")))?
            ;

            if let Some(locations) = result {
                return Ok(Some(locations));
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
            "clearhead/archiveCharter" => self.cmd_archive_charter(params.arguments).await,
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

    /// `clearhead/archiveCharter` — sweep a closed charter's artifacts into `archive.ttl`.
    ///
    /// Arguments: `[uri, charter_name, force?, dry_run?]`
    /// - `uri`          — any file URI inside the workspace (used to locate `.clearhead/`)
    /// - `charter_name` — alias, title fragment, or UUID prefix of the charter to archive
    /// - `force`        — (optional bool) archive even if open actions remain
    /// - `dry_run`      — (optional bool) report counts without writing
    async fn cmd_archive_charter(&self, args: Vec<Value>) -> Result<Option<Value>> {
        let uri_val = args
            .first()
            .ok_or_else(|| Error::invalid_params("Missing URI argument"))?;
        let uri = serde_json::from_value::<Uri>(uri_val.clone())
            .map_err(|e| Error::invalid_params(format!("Invalid URI: {e}")))?;

        let charter_name = args
            .get(1)
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::invalid_params("Missing charter_name argument"))?
            .to_string();

        let force = args.get(2).and_then(|v| v.as_bool()).unwrap_or(false);
        let dry_run = args.get(3).and_then(|v| v.as_bool()).unwrap_or(false);

        let source_path = uri
            .to_file_path()
            .ok_or_else(|| Error::invalid_params("URI is not a file path"))?
            .to_path_buf();

        let result = tokio::task::spawn_blocking(move || {
            // Walk up from the file to find the workspace .clearhead/ root
            let workspace_root = check_for_workspace(&source_path)
                .ok_or_else(|| "No .clearhead workspace found for this file".to_string())?;

            let opts = ArchiveCharterOptions { force, dry_run };
            archive_charter(&workspace_root, &charter_name, &opts).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| internal_error(format!("Archive charter task panicked: {e}")))?;

        match result {
            Ok(res) => {
                let msg = if res.was_dry_run {
                    format!(
                        "Dry run: would archive '{}' ({} primary + {} completed actions, {} plans)",
                        res.charter_name,
                        res.primary_actions_swept,
                        res.completed_actions_swept,
                        res.plans_swept,
                    )
                } else {
                    format!(
                        "Archived charter '{}' → {}",
                        res.charter_name,
                        res.archive_ttl_path.display()
                    )
                };
                self.client.show_message(MessageType::INFO, msg).await;
            }
            Err(e) => {
                self.client
                    .show_message(MessageType::WARNING, format!("Archive charter failed: {e}"))
                    .await;
            }
        }

        Ok(None)
    }
}

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

// =============================================================================
// Workspace reference resolution helpers
// =============================================================================

/// Resolve a reference string to a `(file_path, Range)` using the core reference resolver.
///
/// Handles full UUIDs, short UUID prefixes, alias names, and path notation.
/// Returns `None` when the reference cannot be resolved or the workspace is unavailable.
fn find_definition_in_workspace(
    source_path: &std::path::Path,
    ref_text: &str,
) -> Option<(std::path::PathBuf, tower_lsp_server::ls_types::Range)> {
    use clearhead_core::{
        ReferenceOptions, ReferenceTarget, list_action_files, load_domain_model,
        parse_document, resolve_reference,
    };

    let workspace_root = check_for_workspace(source_path)?;
    let model = load_domain_model(&workspace_root).ok()?;
    let opts = ReferenceOptions::default();
    let target = resolve_reference(&model, ref_text, &opts).ok()?;

    let target_uuid = match target {
        ReferenceTarget::Action(id) | ReferenceTarget::Charter(id) | ReferenceTarget::Plan(id) => id,
    };

    // Walk all action files to find which one defines this UUID
    let action_files = list_action_files(&workspace_root).ok()?;
    for file_path in action_files {
        let content = std::fs::read_to_string(&file_path).ok()?;
        let parsed = parse_document(&content).ok()?;
        if let Some(meta) = parsed.source_map.get(&target_uuid) {
            return Some((file_path, source_range_to_lsp_range(meta.root)));
        }
    }

    None
}

/// Find all locations in the workspace that reference a given UUID as a predecessor.
///
/// Returns `None` when the workspace is unavailable or no references exist.
fn find_references_in_workspace(
    source_path: &std::path::Path,
    ref_text: &str,
) -> Option<Vec<tower_lsp_server::ls_types::Location>> {
    use clearhead_core::{list_action_files, parse_document};

    let target_uuid = uuid::Uuid::parse_str(ref_text).ok();

    let workspace_root = check_for_workspace(source_path)?;
    let action_files = list_action_files(&workspace_root).ok()?;
    let mut locations = Vec::new();

    for file_path in &action_files {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let parsed = match parse_document(&content) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let file_uri = match Uri::from_file_path(file_path) {
            Some(u) => u,
            None => continue,
        };

        for action in &parsed.actions {
            let refs_target = action.predecessors.as_ref().map_or(false, |preds| {
                preds.iter().any(|pred| match target_uuid {
                    Some(uuid) => pred.resolved_uuid == Some(uuid),
                    None => pred.raw_ref.starts_with(ref_text),
                })
            });

            if refs_target {
                if let Some(meta) = parsed.source_map.get(&action.id) {
                    locations.push(tower_lsp_server::ls_types::Location {
                        uri: file_uri.clone(),
                        range: source_range_to_lsp_range(meta.root),
                    });
                }
            }
        }
    }

    if locations.is_empty() { None } else { Some(locations) }
}
