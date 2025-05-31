use std::collections::HashMap;

use serde_json::Value;

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

// this is the function where we actually use treesitter to parse the actions into the tree, and
// translate that into a proper vector of hashmaps so that we are passing back plain data
fn get_action_list(opts: &HashMap<String, Value>, actions: String) -> Vec<HashMap<String, Value>> {
    let mut action_parser = tree_sitter::Parser::new();

    action_parser
        .set_language(&tree_sitter_actions::LANGUAGE.into())
        .expect("Failed to set language for tree-sitter parser");

    let tree = action_parser
        .parse(actions.as_bytes(), None)
        .expect("Failed to parse actions");

    let root_node = tree.root_node();
}

struct RootAction {
    core: CoreActionProperties,
    story: Option<String>,
    children: Vec<ChildAction>,
}

struct ChildAction {
    core: CoreActionProperties,
    grandchildren: Vec<GrandChildAction>,
}

struct GrandChildAction {
    core: CoreActionProperties,
    great_grandchildren: Vec<GrandChildAction>,
}

struct GreatGrandChildAction {
    core: CoreActionProperties,
    great_great_grandchildren: Vec<GrandChildAction>,
}

struct DoubleGreatGrandChildAction {
    core: CoreActionProperties,
    leaf_actions: Vec<LeafAction>,
}

struct LeafAction {
    core: CoreActionProperties,
}

struct CoreActionProperties {
    name: String,
    state: ActionState,
    description: String,
    priority: u8,
    context_list: Vec<String>,
    do_date: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActionState {
    NotStarted,
    Completed,
    InProgress,
    BlockedorAwaiting,
    Cancelled,
}

struct ExtendedDateTime<Tz: chrono::TimeZone> {
    date: chrono::DateTime<Tz>,
    recurrance: Option<Recurrance>,
}

enum Recurrance {
    None,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}
