//! Document processing and save orchestration
//!
//! This module coordinates the parse → format pipeline for document saves.
//! It produces a canonical formatted version of the document and indicates
//! whether the content changed, allowing the LSP to apply edits only when needed.

use clearhead_core::{format, OutputFormat};
use crate::get_parsed_document;

/// Result of processing a document save
#[derive(Debug)]
pub enum SaveResult {
    /// Content is already in canonical form — preserve user's formatting
    NoChange,
    /// Content differs from canonical — new formatted content available
    Changed {
        /// The canonical formatted content
        new_content: String,
    },
}

/// Process a document save event
///
/// Parses the current buffer, formats it canonically, and compares with the
/// input. Returns `NoChange` if the buffer is already canonical, or `Changed`
/// with the new content if formatting would alter it.
///
/// The formatter is idempotent: calling `process_save` on already-formatted
/// content always returns `NoChange`.
pub fn process_save(current_content: &str) -> Result<SaveResult, String> {
    let parsed = get_parsed_document(current_content)?;

    let canonical = format(&parsed.actions, OutputFormat::Actions, None, None)
        .map_err(|e| format!("Failed to format actions: {}", e))?;

    if canonical == current_content {
        Ok(SaveResult::NoChange)
    } else {
        Ok(SaveResult::Changed { new_content: canonical })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_save_adds_id() {
        let content = "[ ] Task without ID\n";

        let result = process_save(content).unwrap();

        match result {
            SaveResult::Changed { new_content } => {
                assert!(new_content.contains("#"), "Expected an ID to be added");
                assert!(new_content.contains("Task without ID"));
            }
            SaveResult::NoChange => {
                panic!("Expected Changed when action is missing an ID");
            }
        }
    }

    #[test]
    fn test_process_save_idempotent() {
        let content = "[ ] Task without ID\n";

        // First pass: get canonical form with ID
        let first = process_save(content).unwrap();
        let canonical = match first {
            SaveResult::Changed { new_content } => new_content,
            SaveResult::NoChange => panic!("Expected Changed on first pass"),
        };

        // Second pass: already canonical — should be NoChange
        let second = process_save(&canonical).unwrap();
        match second {
            SaveResult::NoChange => {}
            SaveResult::Changed { .. } => {
                panic!("Formatter should be idempotent: second pass should not change content");
            }
        }
    }
}
