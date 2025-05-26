use std::collections::HashMap;
use std::path::PathBuf;

pub enum ConfigHashValue {
    HashMap(HashMap<String, CommandHashValue>),
    Int(u8),
    Path(Option<PathBuf>),
}

#[derive(Clone)]
pub enum CommandHashValue {
    String(String),
    Bool(bool),
}

impl Into<Option<PathBuf>> for &ConfigHashValue {
    fn into(self) -> Option<PathBuf> {
        match self {
            ConfigHashValue::Path(path) => path.clone(),
            _ => None,
        }
    }
}

impl Into<u8> for &ConfigHashValue {
    fn into(self) -> u8 {
        match self {
            ConfigHashValue::Int(value) => *value,
            _ => 0,
        }
    }
}

impl Into<HashMap<String, CommandHashValue>> for &ConfigHashValue {
    fn into(self) -> HashMap<String, CommandHashValue> {
        match self {
            ConfigHashValue::HashMap(map) => map.clone(),
            _ => HashMap::new(),
        }
    }
}

impl Into<String> for &CommandHashValue {
    fn into(self) -> String {
        match self {
            CommandHashValue::String(value) => value.clone(),
            _ => String::new(),
        }
    }
}

impl Into<bool> for &CommandHashValue {
    fn into(self) -> bool {
        match self {
            CommandHashValue::Bool(value) => *value,
            _ => false,
        }
    }
}
