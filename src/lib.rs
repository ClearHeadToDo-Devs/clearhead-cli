use std::collections::HashMap;

use serde_json::Value;

//

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
