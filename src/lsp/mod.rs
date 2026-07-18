use std::collections::HashMap;
use std::path::PathBuf;

use clearhead_cli::{ParsedDocument, get_parsed_document};
use dashmap::DashMap;
use tokio::sync::OnceCell;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LspService, Server};
use tracing::{debug, error, warn};
use tree_sitter::{Parser, Tree};

mod handlers;
mod providers;

use providers::compute_diagnostics;

#[derive(Debug)]
struct DocumentState {
    text: String,
    tree: Tree,
    parsed: Option<ParsedDocument>,
    last_saved_parsed: Option<ParsedDocument>,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: DashMap<Uri, DocumentState>,
    workspace_roots: OnceCell<HashMap<Uri, PathBuf>>,
}

impl Backend {
    fn get_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_actions::LANGUAGE.into())
            .expect("Error loading actions grammar");
        parser
    }

    /// Route a document URI to its workspace root.
    ///
    /// Matches against the charter subtree of each known root (via prefix).
    /// Falls back to filesystem walk when workspace_roots is empty or unset —
    /// covers generic LSP clients that don't send workspaceFolders.
    fn workspace_for_uri(&self, uri: &Uri) -> Option<PathBuf> {
        let file_path = uri.to_file_path()?.to_path_buf();

        if let Some(roots) = self.workspace_roots.get() {
            if !roots.is_empty() {
                let abs = std::fs::canonicalize(&file_path).unwrap_or_else(|_| file_path.clone());
                for root_path in roots.values() {
                    let charter_root = clearhead_core::charter_root(root_path);
                    let abs_root = std::fs::canonicalize(&charter_root).unwrap_or(charter_root);
                    if abs.starts_with(&abs_root) {
                        return Some(root_path.clone());
                    }
                }
                // File isn't under any registered workspace — fall through to project detection
            }
        }

        clearhead_core::workspace::check_for_workspace(&file_path)
    }

    async fn update_document(&self, uri: Uri, text: String, is_fresh_load: bool) {
        let mut parser = Self::get_parser();
        if let Some(tree) = parser.parse(&text, None) {
            let parsed = get_parsed_document(&text).ok();

            let diagnostics = if let Some(ref p) = parsed {
                debug!(uri = ?uri, action_count = p.actions.len(), "Document updated");
                compute_diagnostics(p)
            } else {
                warn!(uri = ?uri, "Document update failed to parse");
                Vec::new()
            };

            let last_saved_parsed = if is_fresh_load {
                parsed.clone()
            } else {
                self.documents
                    .get(&uri)
                    .and_then(|d| d.last_saved_parsed.clone())
            };

            self.documents.insert(
                uri.clone(),
                DocumentState {
                    text: text.clone(),
                    tree: tree.clone(),
                    parsed,
                    last_saved_parsed,
                },
            );

            self.client
                .publish_diagnostics(uri, diagnostics, None)
                .await;
        } else {
            error!(uri = ?uri, "Failed to parse document tree");
            self.client
                .log_message(
                    MessageType::ERROR,
                    format!("Failed to parse document: {:?}", uri),
                )
                .await;
        }
    }
}

pub async fn start_lsp() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: DashMap::new(),
        workspace_roots: OnceCell::new(),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearhead_cli::{FormatConfig, OutputFormat, format};

    #[test]
    fn test_lsp_format_normalizes() {
        let text = "[ ] Task without ID";
        let parsed = get_parsed_document(text).unwrap();

        let config = FormatConfig {
            include_id: true,
            ..Default::default()
        };

        let formatted = format(&parsed.actions, OutputFormat::Actions, Some(config), None).unwrap();

        assert!(formatted.contains("#"));
        assert!(formatted.contains("[ ] Task without ID"));
    }

    #[tokio::test]
    async fn test_update_document_manages_last_saved() {
        let (service, _) = LspService::new(|client| Backend {
            client,
            documents: DashMap::new(),
            workspace_roots: OnceCell::new(),
        });
        let backend = service.inner();
        let uri = Uri::from_file_path("/test.actions").unwrap();

        // 1. Initial load (did_open)
        backend
            .update_document(uri.clone(), "[ ] Task 1".to_string(), true)
            .await;
        {
            let doc = backend.documents.get(&uri).unwrap();
            assert!(doc.last_saved_parsed.is_some());
            assert_eq!(doc.last_saved_parsed.as_ref().unwrap().actions.len(), 1);
        }

        // 2. Change (did_change)
        backend
            .update_document(uri.clone(), "[ ] Task 1\n[ ] Task 2".to_string(), false)
            .await;
        {
            let doc = backend.documents.get(&uri).unwrap();
            assert_eq!(doc.parsed.as_ref().unwrap().actions.len(), 2);
            // last_saved should still be the old state (1 action)
            assert_eq!(doc.last_saved_parsed.as_ref().unwrap().actions.len(), 1);
        }
    }

    #[tokio::test]
    async fn test_did_save_emits_events() {
        use tower_lsp_server::LanguageServer;

        let (service, _) = LspService::new(|client| Backend {
            client,
            documents: DashMap::new(),
            workspace_roots: OnceCell::new(),
        });
        let backend = service.inner();
        let uri = Uri::from_file_path("/test.actions").unwrap();

        // 1. Load initial state
        backend
            .update_document(
                uri.clone(),
                "[ ] Task 1 #019baaec-00b6-7991-be34-94b68212619a".to_string(),
                true,
            )
            .await;

        // 2. Change state to completed
        backend
            .update_document(
                uri.clone(),
                "[x] Task 1 #019baaec-00b6-7991-be34-94b68212619a".to_string(),
                false,
            )
            .await;

        // 3. Trigger save
        let params = DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            text: None,
        };
        backend.did_save(params).await;

        // 4. Verify last_saved was updated
        {
            let doc = backend.documents.get(&uri).unwrap();
            assert_eq!(
                doc.last_saved_parsed.as_ref().unwrap().actions[0].state,
                clearhead_cli::ActionState::Completed
            );
        }
    }
}
