//! `expression_parser.go`: tokens to postfix to an expression tree.

use crate::lexer::tokenise;
use crate::operation::{ExprNode, OpType, Operation};
use crate::postfix::convert_to_postfix;
use crate::Error;

pub fn parse_expression(expression: &str) -> Result<Option<ExprNode>, Error> {
    let tokens = tokenise(expression)?;
    let operations = convert_to_postfix(tokens)?;
    create_expression_tree(operations)
}

fn create_expression_tree(postfix: Vec<Operation>) -> Result<Option<ExprNode>, Error> {
    if postfix.is_empty() {
        return Ok(None);
    }
    let mut stack: Vec<ExprNode> = Vec::new();
    for op in postfix {
        let num_args = op.op_type.info().num_args;
        let mut node = ExprNode::leaf(op);
        match num_args {
            1 => {
                if stack.is_empty() {
                    // `first` may be called without an argument.
                    if node.op.op_type != OpType::First {
                        return Err(Error(format!(
                            "'{}' expects 1 arg but received none",
                            node.op.string_value.trim()
                        )));
                    }
                } else {
                    let rhs = stack.pop();
                    node.rhs = rhs.map(Box::new);
                }
            }
            2 => {
                if stack.len() < 2 {
                    return Err(Error(format!(
                        "'{}' expects 2 args but there is {}",
                        node.op.string_value.trim(),
                        stack.len()
                    )));
                }
                let rhs = stack.pop();
                let lhs = stack.pop();
                node.lhs = lhs.map(Box::new);
                node.rhs = rhs.map(Box::new);
            }
            _ => {}
        }
        stack.push(node);
    }
    if stack.len() != 1 {
        return Err(Error(
            "bad expression, please check expression syntax".to_string(),
        ));
    }
    Ok(stack.pop())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(node: &ExprNode) -> String {
        let mut out = node.op.op_type.name().to_string();
        if node.lhs.is_some() || node.rhs.is_some() {
            out.push('(');
            if let Some(lhs) = node.lhs() {
                out.push_str(&shape(lhs));
            }
            out.push(',');
            if let Some(rhs) = node.rhs() {
                out.push_str(&shape(rhs));
            }
            out.push(')');
        }
        out
    }

    #[test]
    fn builds_trees() {
        let tree = parse_expression(".a.b | .c").unwrap().unwrap();
        assert_eq!(
            shape(&tree),
            "PIPE(SHORT_PIPE(TRAVERSE_PATH,TRAVERSE_PATH),TRAVERSE_PATH)"
        );
        let tree = parse_expression(".a = 1").unwrap().unwrap();
        assert_eq!(shape(&tree), "ASSIGN(TRAVERSE_PATH,VALUE)");
        let tree = parse_expression("[.a, .b]").unwrap().unwrap();
        assert_eq!(shape(&tree), "COLLECT(,UNION(TRAVERSE_PATH,TRAVERSE_PATH))");
        let tree = parse_expression("{\"a\": .b}").unwrap().unwrap();
        assert_eq!(
            shape(&tree),
            "SHORT_PIPE(CREATE_MAP(STRING_INT,TRAVERSE_PATH),COLLECT_OBJECT)"
        );
        let tree = parse_expression("if .a then .b else .c end")
            .unwrap()
            .unwrap();
        assert_eq!(
            shape(&tree),
            "IF_ELSE(IF_THEN(TRAVERSE_PATH,TRAVERSE_PATH),TRAVERSE_PATH)"
        );
    }
}
