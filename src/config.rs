use config_rs;
use directories::BaseDirs;
use serde::Deserialize;
use serde::Serialize;
use std::path::Path;
use toml;

#[derive(Serialize, Deserialize)]
struct Settings {
    config_dir: String,
    data_dir: String,
}

pub fn save_default_settings() {
    let default_toml = toml::to_string(&generate_default_settings()).unwrap();

    let config_location = Path::new("config/default.toml");

    std::fs::write(config_location, default_toml).unwrap();
}

fn generate_default_settings() -> Settings {
    return Settings {
        config_dir: get_default_settings_dir(),
        data_dir: get_default_data_dir(),
    };
}

fn get_default_settings_dir() -> String {
    if let Some(base_dirs) = BaseDirs::new() {
        return String::from(base_dirs.config_dir().to_str().unwrap());
    } else {
        panic!("cant find your config directory!")
    }
}

fn get_default_data_dir() -> String {
    if let Some(base_dirs) = BaseDirs::new() {
        return String::from(base_dirs.data_dir().to_str().unwrap());
    } else {
        panic!("cant find your data directory directory!")
    }
}
