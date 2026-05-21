//! Display module — visual rendering for TTY output.
//!
//! All functions here are TTY-only. They consume `DomainModel` or other core
//! types directly and produce human-readable strings. Nothing here belongs in
//! `clearhead-core`; the library has no business knowing about terminal display.
//!
//! When stdout is a pipe, callers should emit JSON-LD instead of calling these.

pub mod tree;
pub use tree::{render_charter_tree, render_domain_tree};
