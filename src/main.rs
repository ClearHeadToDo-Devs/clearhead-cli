pub mod model;
pub mod settings;
use std::collections::HashMap;

fn main() {
    let config = settings::generate_settings();

    println!(
        "{:?}",
        config.try_deserialize::<HashMap<String, String>>().unwrap()
    );
}
