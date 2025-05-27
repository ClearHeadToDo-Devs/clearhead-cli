use std::{collections::HashMap, path::PathBuf};

use rpds::{HashTrieMap, Vector};

type PMap = HashTrieMap<String, PHashValue>;
pub fn parse_actions(config: PMap, actions: String) -> Result<Vector<PMap>, String> {
    let result = Vector::new();
    for action in actions.split(',') {
        let action = action.trim();
        if let Some(value) = config.get(action) {
            match value {
                PHashValue::HashMap(map) => result.push_back(map.clone()),
                _ => return Err(format!("Action '{}' is not a valid map", action)),
            }
        } else {
            return Err(format!("Action '{}' not found in configuration", action));
        };
    }
    Ok(result)
}

pub enum PHashValue {
    HashMap(HashTrieMap<String, PHashValue>),
    Vector(Vector<PHashValue>),
    String(String),
    Int(u8),
    Bool(bool),
    Path(PathBuf),
    Null,
}
