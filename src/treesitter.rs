use tree_sitter::{Node, Tree};
pub fn get_node_text(node: &Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}
pub fn create_tree_wrapper(tree: Tree, source: String) -> TreeWrapper {
    TreeWrapper { tree, source }
}

/// Validate a tree-sitter tree for syntax errors
pub fn validate_tree(tree: &Tree) -> Result<(), String> {
    let root = tree.root_node();
    if root.has_error() {
        // Find the specific error node
        let mut cursor = root.walk();
        let mut error_node = None;
        
        // Depth-first search for the first ERROR or MISSING node
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.is_error() || node.is_missing() {
                error_node = Some(node);
                break;
            }
            for child in node.children(&mut cursor) {
                stack.push(child);
            }
        }

        if let Some(err) = error_node {
            let start = err.start_position();
            return Err(format!(
                "Syntax error at line {}, column {}: {}",
                start.row + 1,
                start.column + 1,
                if err.is_missing() { format!("missing '{}'", err.kind()) } else { "unexpected token".to_string() }
            ));
        }
        return Err("Syntax error in actions file".to_string());
    }
    Ok(())
}

// we need both the tree and the source to do our type conversions properly
pub struct TreeWrapper {
    pub tree: Tree,
    pub source: String,
}

pub fn create_node_wrapper(node: Node, source: String) -> NodeWrapper {
    NodeWrapper { node, source }
}

// same goes for the nodes, infact, we are going to be passing a cloned version of the string so
// everything has what they need early
pub struct NodeWrapper<'a> {
    pub node: Node<'a>,
    pub source: String,
}
