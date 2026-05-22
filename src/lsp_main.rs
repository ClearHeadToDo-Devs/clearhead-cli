//! Entry point for the standalone `clearhead-lsp` binary.
//!
//! The LSP server reads JSON-RPC messages from stdin and writes to stdout,
//! following the Language Server Protocol specification.
//!
//! Usage: pipe `clearhead-lsp` as the language server command in your editor
//! configuration. Example for Neovim:
//!
//! ```lua
//! require('lspconfig').clearhead.setup {
//!   cmd = { 'clearhead-lsp' },
//! }
//! ```
//!
//! Alternatively, the combined binary supports `clearhead lsp` when compiled
//! with the `lsp` feature (the default).

#[cfg(feature = "lsp")]
mod lsp;

fn main() {
    #[cfg(feature = "lsp")]
    {
        use tracing_subscriber::{EnvFilter, fmt};
        fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .init();

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build tokio runtime");

        rt.block_on(lsp::start_lsp());
    }

    #[cfg(not(feature = "lsp"))]
    {
        eprintln!("This binary was compiled without LSP support.");
        std::process::exit(1);
    }
}
