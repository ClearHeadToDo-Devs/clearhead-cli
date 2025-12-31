use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tree_sitter::{Parser, Tree};
use uuid::Uuid;

#[derive(Debug)]
struct DocumentState {
    text: String,
    tree: Tree,
}

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: DashMap<Url, DocumentState>,
}

impl Backend {
    fn get_parser() -> Parser {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_actions::LANGUAGE.into())
            .expect("Error loading actions grammar");
        parser
    }

    fn update_document(&self, uri: Url, text: String) {
        let mut parser = Self::get_parser();
        if let Some(tree) = parser.parse(&text, None) {
            self.documents.insert(uri, DocumentState { text, tree });
        }
    }
}

#[tower_lsp::async_trait]
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
                ..Default::default()
            },
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.update_document(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            self.update_document(params.text_document.uri, change.text);
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let mut actions = Vec::new();

        if let Some(doc) = self.documents.get(&uri) {
            let mut cursor = doc.tree.walk();
            
            // Traverse the tree to find actions missing IDs
            // Note: This is a simple linear scan of the tree for now.
            // We can optimize this by checking nodes within the requested range.
            let mut nodes_to_check = vec![doc.tree.root_node()];
            
            while let Some(node) = nodes_to_check.pop() {
                let node: tree_sitter::Node = node;
                // Check if this node is an action (root_action, depth1_action, etc.)
                if node.kind().contains("action") && !node.kind().contains("marker") && !node.kind().contains("list") {
                    
                    // Check if it has an 'id' metadata child
                    let has_id = node.children_by_field_name("metadata", &mut node.walk())
                        .any(|child: tree_sitter::Node| child.kind() == "id");

                    if !has_id {
                        let range = Range {
                            start: Position::new(node.start_position().row as u32, node.start_position().column as u32),
                            end: Position::new(node.end_position().row as u32, node.end_position().column as u32),
                        };

                        // Only suggest if the cursor is near the action
                        if range.start.line <= params.range.start.line && range.end.line >= params.range.end.line {
                            let uuid = Uuid::now_v7(); // UUIDv7 for better sortability
                            let new_text = format!(" #{}", uuid);
                            
                            // Insert at the end of the action body (before children)
                            // Usually this is at the end of the first line of the action
                            let insert_pos = Position::new(
                                node.start_position().row as u32,
                                node.end_position().column as u32,
                            );

                            let mut changes = std::collections::HashMap::new();
                            changes.insert(uri.clone(), vec![TextEdit {
                                range: Range::new(insert_pos, insert_pos),
                                new_text,
                            }]);

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
        }

        Ok(Some(actions))
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