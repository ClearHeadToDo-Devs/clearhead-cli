use serde_json::{Map, Value};
use tree_sitter::Tree;

pub mod treesitter;

pub mod entities;
use entities::ActionList;

pub mod format;
pub use format::{format, OutputFormat};

// merging json hashmaps as our universal structure
pub fn merge_hashmaps(
    left: &Map<String, Value>,
    right: &Map<String, Value>,
) -> Result<Value, String> {
    let mut merged = left.clone();
    for (key, value) in right {
        merged.insert(key.clone(), value.clone());
    }
    Ok(Value::Object(merged))
}

/// Parse a .actions file into a structured ActionList
///
/// # Arguments
/// * `_opts` - Configuration options (currently unused)
/// * `actions` - The .actions file content as a string
///
/// # Returns
/// A Vec<Action> representing the flat list of parsed actions
pub fn get_action_list_struct(_opts: &Value, actions: &str) -> Result<ActionList, String> {
    let tree = get_action_list_tree(actions)?;
    let tree_wrapper = treesitter::TreeWrapper {
        tree,
        source: actions.to_string(),
    };
    let action_list: ActionList = tree_wrapper.try_into().map_err(|e: &str| e.to_string())?;
    Ok(action_list)
}

/// Parse a .actions file and return as JSON Value
///
/// This is a convenience wrapper around get_action_list_struct that
/// serializes the result to JSON for data-centric workflows.
///
/// # Arguments
/// * `opts` - Configuration options (passed through)
/// * `actions` - The .actions file content as a string
///
/// # Returns
/// A JSON Value containing the serialized ActionList
pub fn get_action_list(opts: &Value, actions: String) -> Result<Value, String> {
    let action_list = get_action_list_struct(opts, &actions)?;
    serde_json::to_value(&action_list)
        .map_err(|e| format!("Failed to serialize actions to JSON: {}", e))
}

use tree_sitter::{Query, QueryCursor, StreamingIterator};

/// Parse a .actions file into a tree-sitter Tree
///
/// This is a low-level function that returns the raw tree-sitter parse tree.
/// Most users should use get_action_list_struct() instead.
///
/// # Arguments
/// * `actions` - The .actions file content as a string
///
/// # Returns
/// A tree-sitter Tree representing the parsed structure
pub fn get_action_list_tree(actions: &str) -> Result<Tree, String> {
    let mut action_parser = tree_sitter::Parser::new();

    action_parser
        .set_language(&tree_sitter_actions::LANGUAGE.into())
        .expect("Failed to set language for tree-sitter parser");

    action_parser
        .parse(actions, None)
        .ok_or("Failed to parse tree".to_string())
}

/// Filter actions using a Tree-sitter query
///
/// # Arguments
/// * `actions` - The .actions file content as a string
/// * `query_source` - The Scheme query content
///
/// # Returns
/// A filtered ActionList
pub fn run_query(actions: &str, query_source: &str) -> Result<ActionList, String> {
    let tree = get_action_list_tree(actions)?;
    let language = tree_sitter_actions::LANGUAGE.into();
    let query = Query::new(&language, query_source)
        .map_err(|e| format!("Invalid query: {}", e))?;
    
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), actions.as_bytes());

    let mut filtered_actions = ActionList::new();
    let mut seen_ids = std::collections::HashSet::new();

    // Re-use the existing parsing logic from entities
    while let Some(m) = matches.next() {
        for capture in m.captures {
            let node = capture.node;
            // Only process nodes that are valid actions
            if node.kind().contains("action") {
                // Find parent ID if possible
                let parent_id = find_parent_id(node, actions);
                
                let wrapper = treesitter::create_node_wrapper(node, actions.to_string());
                
                // We use the recursive parser but just take the first result (the node itself)
                // to avoid duplicating the entire subtree if the query also matches children
                if let Ok(parsed_actions) = entities::parse_action_recursive(wrapper, parent_id) {
                    if let Some(action) = parsed_actions.first() {
                        if seen_ids.insert(action.id) {
                            filtered_actions.push(action.clone());
                        }
                    }
                }
            }
        }
    }

    Ok(filtered_actions)
}

fn find_parent_id(node: tree_sitter::Node, source: &str) -> Option<uuid::Uuid> {
    let mut parent = node.parent();
    while let Some(p) = parent {
        if p.kind().contains("action") {
            // Found an action ancestor, try to extract its ID
            // We assume the file is normalized for this to work reliably
            let wrapper = treesitter::create_node_wrapper(p, source.to_string());
            // This is a bit expensive (re-parsing), but safe. 
            // In a tight loop we might want a lightweight ID extractor.
            if let Ok(actions) = entities::parse_action_recursive(wrapper, None) {
                 if let Some(action) = actions.first() {
                     return Some(action.id);
                 }
            }
            // Stop at the immediate parent action
            return None;
        }
        parent = p.parent();
    }
    None
}

/// Patch a primary ActionList with updates from a secondary list

/// Patch a primary ActionList with updates from a secondary list
///
/// Matches actions by UUID. If an action exists in both, it is updated
/// in the primary list. If it only exists in the secondary list, it is
/// appended to the primary list.
///
/// # Arguments
/// * `primary` - The source of truth list to be modified
/// * `secondary` - The list containing updates/additions
pub fn patch_action_list(primary: &mut ActionList, secondary: &ActionList) {
    for patch_action in secondary {
        if let Some(original_action) = primary.iter_mut().find(|a| a.id == patch_action.id) {
            // Update existing action
            *original_action = patch_action.clone();
        } else {
            // Add new action
            primary.push(patch_action.clone());
        }
    }
}
