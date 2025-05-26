use std::collections::HashMap;
pub fn parse_actions(
    config: HashMap<String, String>,
    actions: HashMap<String, String>,
) -> Result<Vec<HashMap<String, String>>, String> {
    let example_hash = HashMap::from([
        ("action1".to_string(), "value1".to_string()),
        ("action2".to_string(), "value2".to_string()),
    ]);

    let example_vector = vec![example_hash];

    return Ok(example_vector);
}
