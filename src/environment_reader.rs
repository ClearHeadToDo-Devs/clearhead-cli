use dirs::{config_dir, data_dir};
use serde_json::{Map, Value};
use std::path::PathBuf;

use config::Config as ConfigBuilder;

type Config = Map<String, Value>;

pub fn get_config_map(custom_config_loc: Option<PathBuf>) -> Config {
    let default_config_location = PathBuf::from(format!(
        "{}/cliche/settings.toml",
        config_dir().unwrap().display()
    ));
    let default_action_location = format!("{}/clhd/active.action", data_dir().unwrap().display());

    if custom_config_loc.is_none() {
        ensure_path_exists(&default_config_location);
    }

    ensure_path_exists(&PathBuf::from(&default_action_location));

    let settings = ConfigBuilder::builder()
        .add_source(config::Environment::with_prefix("CLICHE"))
        .add_source(config::File::from(
            custom_config_loc.unwrap_or(default_config_location),
        ))
        .set_default("action_path", default_action_location)
        .unwrap()
        .build()
        .unwrap_or_else(|e| {
            panic!("Failed to build configuration: {}", e);
        });

    return settings.try_deserialize::<Map<String, Value>>().unwrap();
}
pub fn ensure_path_exists(path: &PathBuf) {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).expect("Failed to create parent directory");
            }
        }
        std::fs::File::create(path).expect("Failed to create file");
    }
}
