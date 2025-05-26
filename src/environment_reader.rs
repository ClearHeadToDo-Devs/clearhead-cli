use dirs::config_dir;
use std::{collections::HashMap, path::PathBuf};

use config::Config;

pub fn get_config_map(extra_config: Option<PathBuf>) -> HashMap<String, String> {
    let default_config_location = PathBuf::from(format!(
        "{}/cliche/settings.toml",
        config_dir().unwrap().display()
    ));

    let settings = Config::builder()
        .add_source(config::Environment::with_prefix("CLICHE"))
        .add_source(config::File::from(
            extra_config.unwrap_or(default_config_location),
        ))
        .build()
        .unwrap();

    return settings
        .try_deserialize::<HashMap<String, String>>()
        .unwrap();
}
