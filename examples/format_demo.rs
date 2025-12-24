use clearhead_cli::*;
use tree_sitter_actions::get_test_data;

fn main() {
    let test_config = serde_json::json!({});
    let test_data = get_test_data()["children"]["with_children"]["content"].clone();
    let actions = get_action_list_struct(&test_config, &test_data).unwrap();

    println!("=== ACTIONS FORMAT ===");
    println!("{}", format(&actions, OutputFormat::Actions).unwrap());

    println!("\n=== JSON FORMAT ===");
    println!("{}", format(&actions, OutputFormat::Json).unwrap());

    println!("\n=== XML FORMAT ===");
    println!("{}", format(&actions, OutputFormat::Xml).unwrap());

    println!("\n=== TABLE FORMAT ===");
    println!("{}", format(&actions, OutputFormat::Table).unwrap());
}
