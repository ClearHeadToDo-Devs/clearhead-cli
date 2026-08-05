//! Structured results for mutation verbs (query_output.md, "Errors as data").
//!
//! A verb outcome — success or failure — is data an automated loop can branch
//! on, not prose it has to parse. When stdout is not a terminal the verbs emit
//! one JSON object per invocation; a human at a terminal keeps the prose.
//! `id` is canonical identity exactly as the query contract exports it
//! (`urn:uuid:…`), so read and write halves speak the same spelling.
//!
//! `conflict` joins the taxonomy when the write path gains compare-and-swap;
//! today no conflicting interleave is observable, so it is not modeled.

use serde::Serialize;
use std::io::IsTerminal;
use uuid::Uuid;

/// Canonical identity as the query contract exports it.
pub fn canonical_id(id: Uuid) -> String {
    format!("urn:uuid:{id}")
}

fn bare(id: &str) -> &str {
    id.trim_start_matches("urn:uuid:")
}

/// A mutation verb that applied.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum VerbOutcome {
    Completed { id: String, children: usize },
    Cancelled { id: String, children: usize },
    Updated { id: String },
}

impl VerbOutcome {
    fn prose(&self) -> String {
        match self {
            VerbOutcome::Completed { id, children } => {
                format!("Completed action {} (+{} children)", bare(id), children)
            }
            VerbOutcome::Cancelled { id, children } => {
                format!("Cancelled action {} (+{} children)", bare(id), children)
            }
            VerbOutcome::Updated { id } => format!("Updated action {}", bare(id)),
        }
    }

    /// Print the outcome: JSON when piped, prose at a terminal.
    pub fn emit(&self) {
        if std::io::stdout().is_terminal() {
            println!("{}", self.prose());
        } else {
            println!(
                "{}",
                serde_json::to_string(self).expect("verb outcome serializes")
            );
        }
    }
}

/// A mutation verb that could not apply. Carried through `anyhow` and
/// downcast in `main`, which emits it as JSON when stdout is piped.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum VerbError {
    /// Nothing open or closed matches the query.
    NotFound { query: String },
    /// More than one action matched the strongest canonical reference tier.
    Ambiguous {
        query: String,
        candidates: Vec<String>,
    },
    /// The query resolves, but to an action already in a completed archive —
    /// an idempotent loop can branch on this as effectively-done.
    AlreadyClosed {
        id: String,
        state: String,
        query: String,
    },
}

impl std::fmt::Display for VerbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerbError::NotFound { query } => {
                write!(f, "No open action found matching '{query}'")
            }
            VerbError::Ambiguous { query, candidates } => write!(
                f,
                "Ambiguous action reference '{query}'; candidates: {}",
                candidates
                    .iter()
                    .map(|id| bare(id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            VerbError::AlreadyClosed { id, state, .. } => {
                write!(f, "Action {} is already closed ({state})", bare(id))
            }
        }
    }
}

impl std::error::Error for VerbError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcome_serializes_with_kind_tag_and_canonical_id() {
        let id = Uuid::parse_str("01951111-0000-7000-8000-000000000001").unwrap();
        let json = serde_json::to_string(&VerbOutcome::Completed {
            id: canonical_id(id),
            children: 2,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"kind":"completed","id":"urn:uuid:01951111-0000-7000-8000-000000000001","children":2}"#
        );
    }

    #[test]
    fn errors_serialize_branchable_kinds() {
        let not_found = serde_json::to_string(&VerbError::NotFound { query: "x".into() }).unwrap();
        assert_eq!(not_found, r#"{"kind":"not-found","query":"x"}"#);

        let ambiguous = serde_json::to_string(&VerbError::Ambiguous {
            query: "dead".into(),
            candidates: vec![
                "urn:uuid:dead0000-0000-7000-8000-000000000001".into(),
                "urn:uuid:deadffff-0000-7000-8000-000000000002".into(),
            ],
        })
        .unwrap();
        assert!(ambiguous.starts_with(r#"{"kind":"ambiguous""#));

        let closed = serde_json::to_string(&VerbError::AlreadyClosed {
            id: "urn:uuid:01951111-0000-7000-8000-000000000001".into(),
            state: "Completed".into(),
            query: "x".into(),
        })
        .unwrap();
        assert!(
            closed.starts_with(r#"{"kind":"already-closed""#),
            "got: {closed}"
        );
    }
}
