use clearhead_cli::get_action_list_struct;
use serde_json::json;

#[test]
fn test_syntax_error_reporting() {
    // Missing closing bracket for state
    let source = "[ Buy milk";
    
    let result = get_action_list_struct(&json!({}), source);
    
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("Syntax error"));
    assert!(err.contains("line 1"));
    // It should mention missing "]" or unexpected token
}
