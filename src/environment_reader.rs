use dirs::config_dir;
use serde_json::{Map, Value};
use std::path::PathBuf;

use config::Config as ConfigBuilder;

type Config = Map<String, Value>;

pub fn get_config_map(extra_config: Option<PathBuf>) -> Config {
    let default_config_location = PathBuf::from(format!(
        "{}/cliche/settings.toml",
        config_dir().unwrap().display()
    ));

    let settings = ConfigBuilder::builder()
        .add_source(config::Environment::with_prefix("CLICHE"))
        .add_source(config::File::from(
            extra_config.unwrap_or(default_config_location),
        ))
        .build()
        .unwrap();

    return settings.try_deserialize::<Config>().unwrap();
}
