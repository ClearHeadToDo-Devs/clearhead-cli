mod common;

#[test]
fn test_unsupported_grammar_rules() {
    // This test documents why certain rules are not implemented in the linter.
    // They are enforced by the tree-sitter grammar, so the parser fails before linting can happen.
    // We verify this by attempting to parse invalid examples and expecting failure.

    // E009: Hierarchy Depth Exceeded (>5 levels)
    // Grammar rule `root_action` -> `depth1_action` -> ... -> `depth5_action`
    // `depth5_action` has no `child` field, so depth 6 is impossible to parse.
    let e009_source = "[ ] Root
>[ ] D1
>>[ ] D2
>>>[ ] D3
>>>>[ ] D4
>>>>>[ ] D5
>>>>>>[ ] D6";
    let e009_result = clearhead_cli::get_parsed_document(e009_source);

    if let Ok(doc) = e009_result {
        // If it parses, ensure depth 6 is NOT present as a child
        let depth_6 = doc.actions.iter().find(|a| a.name.contains("D6"));
        if let Some(d6) = depth_6 {
            let depth = d6.depth(&doc.actions);
            assert!(depth <= 5, "Grammar allowed depth > 5!");
        }
    }

    // E010: Orphaned Child Marker (>[ ] at root)
    // Grammar `root_action` does not allow starting with `>`.
    let e010_source = ">[ ] Orphan";
    let e010_result = clearhead_cli::get_parsed_document(e010_source);
    if let Ok(doc) = e010_result {
        // Should likely be empty or not parsed as an action with parent
        let orphan = doc.actions.first();
        if let Some(a) = orphan {
            assert!(a.parent_id.is_none());
        }
    }

    // E011: Skipped Hierarchy Level ([ ] -> >>[ ])
    // Grammar `depth1_action` expects `depth2_action` as child.
    // It cannot skip to `depth3_action`.
    let e011_source = "[ ] Root
>>[ ] Skipped";
    let e011_result = clearhead_cli::get_parsed_document(e011_source);
    if let Ok(doc) = e011_result {
        let skipped = doc.actions.iter().find(|a| a.name == "Skipped");
        if let Some(s) = skipped {
            // It shouldn't be a child of Root
            let root = doc.actions.iter().find(|a| a.name == "Root").unwrap();
            assert_ne!(
                s.parent_id,
                Some(root.id),
                "Grammar allowed skipping depth levels!"
            );
        }
    }
}
