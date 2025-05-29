use dirs::config_dir;
use serde_json::Value;
use std::path::PathBuf;

use config::Config as ConfigBuilder;

use std::collections::HashMap;

type Config = HashMap<String, Value>;

pub fn get_config_map(custom_config_loc: Option<PathBuf>) -> Config {
    let default_config_location = PathBuf::from(format!(
        "{}/cliche/settings.toml",
        config_dir().unwrap().display()
    ));

    if custom_config_loc.is_none() && !default_config_location.exists() {
        // first, check if the default config directory exists
        if !default_config_location.parent().unwrap().exists() {
            // if it doesn't, create the parent directory
            std::fs::create_dir_all(default_config_location.parent().unwrap())
                .expect("Failed to create config directory");

            // then copy the default config file from the examples directory in-repo
            let example_config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples")
                .join("default")
                .join("settings.toml");
            std::fs::copy(example_config_path, &default_config_location)
                .expect("Failed to copy default config file");
        }
    }

    let settings = ConfigBuilder::builder()
        .add_source(config::Environment::with_prefix("CLICHE"))
        .add_source(config::File::from(
            custom_config_loc.unwrap_or(default_config_location),
        ))
        .build()
        .unwrap_or_else(|e| {
            panic!("Failed to build configuration: {}", e);
        });

    return settings
        .try_deserialize::<HashMap<String, Value>>()
        .unwrap();
}
