use std::collections::HashMap;

use serde_json::Value;
use tree_sitter::Tree;

mod entities;

// this is the function where we actually use treesitter to parse the actions into the tree, and
// translate that into a proper vector of hashmaps so that we are passing back plain data
fn get_action_list(
    opts: &HashMap<String, Value>,
    actions: String,
) -> Result<Vec<HashMap<String, Value>>, String> {
    let tree = match get_action_list_tree(&actions) {
        Ok(tree) => tree,
        Err(e) => {
            return Err(format!("Failed to parse actions: {}", e));
        }
    };

    let root = tree.root_node();

    let mut cursor = root.walk();

    Ok(root
        .children(&mut cursor)
        .filter(|n| n.kind() == "root_action")
        .map(|n| node_to_map(n, &actions))
        .collect())
}

pub fn get_action_list_tree(actions: &str) -> Result<Tree, String> {
    let mut action_parser = tree_sitter::Parser::new();

    action_parser
        .set_language(&tree_sitter_actions::LANGUAGE.into())
        .expect("Failed to set language for tree-sitter parser");

    return match action_parser.parse(actions, None) {
        Some(tree) => Ok(tree),
        None => Err("Failed to parse actions".to_string()),
    };
}

fn get_tree_sexp(tree: &Tree) -> String {
    return tree.root_node().to_sexp();
}

fn node_to_map(node: tree_sitter::Node, source: &str) -> HashMap<String, Value> {
    let mut map = HashMap::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let kind = child.kind();
        let val = match child.child_count() {
            0 => Value::String(
                child
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            ),
            _ => {
                // For lists or nested actions, recurse
                if kind.ends_with("_list") {
                    let mut arr = Vec::new();
                    let mut list_cursor = child.walk();
                    for n in child.children(&mut list_cursor) {
                        arr.push(serde_json::to_value(node_to_map(n, source)).unwrap());
                    }
                    Value::Array(arr)
                } else {
                    serde_json::to_value(node_to_map(child, source)).unwrap()
                }
            }
        };
        // Insert, handling duplicate keys as arrays
        if let Some(existing) = map.get_mut(kind) {
            match existing {
                Value::Array(arr) => arr.push(val),
                old => *old = Value::Array(vec![old.take(), val]),
            }
        } else {
            map.insert(kind.to_string(), val);
        }
    }
    map
}

// Merges two hashmaps using json as Values.
// // If a key exists in both, the value from `args` will overwrite the value from `config`.
pub fn merge_hashmaps(
    left_map: &HashMap<String, Value>,
    right_map: &HashMap<String, Value>,
) -> HashMap<String, Value> {
    let mut merged = left_map.clone();
    for (key, value) in right_map.iter() {
        // If null, we not only skip overwriting, but also remove the key if it exists in the
        // merged map, since our approach to nulls is to just not include them.
        if value.is_null() {
            if let Some(existing_value) = merged.get(key) {
                if existing_value.is_null() {
                    merged.remove(key); // Remove the key if both are null
                }
            }
            continue;
        }

        // if the value is a map, we need to recursively merge it
        if let Some(existing_value) = merged.get(key) {
            if existing_value.is_object() && value.is_object() {
                let existing_map = existing_value.as_object().unwrap();
                let new_map = value.as_object().unwrap();
                let merged_map = merge_hashmaps(
                    &existing_map
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                    &new_map
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect(),
                );
                merged.insert(
                    key.clone(),
                    Value::Object(serde_json::Map::from_iter(merged_map)),
                );
                continue;
            }
        }
        merged.insert(key.clone(), value.clone());
    }
    merged
}
