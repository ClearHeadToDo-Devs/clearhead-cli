use config::{Config, Environment, File};
use directories::BaseDirs;
use serde::Deserialize;
use serde::Serialize;
use std::fs::DirBuilder;
use std::path::Path;
use toml;

pub fn generate_settings() -> Config {
    check_default_settings();

    return Config::builder()
        .set_default("data", get_default_data_dir())
        .unwrap()
        .add_source(File::with_name(&get_default_user_settings_file()))
        .add_source(Environment::with_prefix("CLEARHEAD").separator("_"))
        .build()
        .unwrap();
}

fn check_default_settings() {
    check_user_settings_dir();
    let default_toml = toml::to_string(&generate_default_settings()).unwrap();
    let default_file_location = get_default_user_settings_dir() + "/config.toml";

    let config_location = Path::new(&default_file_location);

    match std::fs::exists(&default_file_location).unwrap() {
        true => (),
        false => std::fs::write(config_location, default_toml).unwrap(),
    }
}

fn generate_default_settings() -> Settings {
    return Settings {
        data: get_default_data_dir(),
    };
}

#[derive(Serialize, Deserialize)]
struct Settings {
    data: String,
}

fn get_default_data_dir() -> String {
    if let Some(base_dirs) = BaseDirs::new() {
        return String::from(base_dirs.data_dir().to_str().unwrap());
    } else {
        panic!("cant find your data directory directory!")
    }
}

fn check_user_settings_dir() {
    match DirBuilder::new()
        .recursive(true)
        .create(get_user_settings_dir())
    {
        Ok(_) => (),
        Err(e) => panic!("Error creating user settings directory: {}", e),
    }
}

fn get_user_settings_dir() -> String {
    match std::env::vars().find(|x| x.0 == "CLEARHEAD_CONFIG_DIR") {
        None => return get_default_user_settings_dir(),
        Some((_, dir)) => return dir,
    }
}

fn get_default_user_settings_file() -> String {
    return get_default_user_settings_dir() + "/config.toml";
}

fn get_default_user_settings_dir() -> String {
    if let Some(base_dirs) = BaseDirs::new() {
        return String::from(base_dirs.config_dir().to_str().unwrap());
    } else {
        panic!("cant find your config directory!")
    }
}
