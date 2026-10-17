//! `operator_path.go`: path, setpath, del_paths.

use crate::context::Context;
use crate::eval::{create_traversal_tree, Evaluator};
use crate::node::{Kind, Node, NodeId, PathElem};
use crate::operation::{ExprNode, OpType, Operation, TraversePrefs};
use crate::util::parse_int;
use crate::Error;

fn path_array_from_node(
    d: &Evaluator,
    func_name: &str,
    node: NodeId,
) -> Result<Vec<PathElem>, Error> {
    if d.arena.get(node).kind != Kind::Sequence {
        return Err(Error(format!(
            "{func_name}: expected path array, but got {} instead",
            d.arena.get(node).tag
        )));
    }
    let mut path = Vec::new();
    for child in &d.arena.get(node).content {
        let child_node = d.arena.get(*child);
        match child_node.tag.as_str() {
            "!!str" => path.push(PathElem::Key(child_node.value.clone())),
            "!!int" => {
                let number = parse_int(&child_node.value).map_err(|e| {
                    Error(format!(
                        "{func_name}: could not parse {} as an int: {}",
                        child_node.value, e.0
                    ))
                })?;
                path.push(PathElem::Index(number));
            }
            other => {
                return Err(Error(format!(
                "{func_name}: expected either a !!str or !!int in the path, found {other} instead"
            )))
            }
        }
    }
    Ok(path)
}

pub fn set_path_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let rhs = d.rhs(expr)?;
    if rhs.op.op_type != OpType::Block {
        return Err(Error(format!(
            "SETPATH must be given a block (;), got {} instead",
            rhs.op.op_type.name()
        )));
    }
    let lhs_path_context = d.get_matching_nodes_opt(context.read_only_clone(), rhs.lhs())?;
    if lhs_path_context.len() != 1 {
        return Err(Error(format!(
            "SETPATH: expected single path but found {} results instead",
            lhs_path_context.len()
        )));
    }
    let lhs_value = lhs_path_context.first().unwrap_or(0);
    let lhs_path = path_array_from_node(d, "SETPATH", lhs_value)?;
    let traversal = create_traversal_tree(&lhs_path, TraversePrefs::default(), false);
    for candidate in context.nodes().to_vec() {
        let target =
            d.get_matching_nodes_opt(context.single_readonly_child_context(candidate), rhs.rhs())?;
        if target.len() != 1 {
            return Err(Error(format!(
                "SETPATH: expected single value on RHS but found {}",
                target.len()
            )));
        }
        let value = target.first().unwrap_or(0);
        let tree = ExprNode::binary(
            Operation::new(OpType::Assign),
            traversal.clone(),
            ExprNode::leaf(Operation::reference(value)),
        );
        d.get_matching_nodes(context.single_child_context(candidate), &tree)?;
    }
    Ok(context)
}

pub fn del_paths_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let path_arrays = d.get_matching_nodes_opt(context.read_only_clone(), expr.rhs())?;
    if path_arrays.len() != 1 {
        return Err(Error(format!(
            "DELPATHS: expected single value but found {}",
            path_arrays.len()
        )));
    }
    let paths_node = path_arrays.first().unwrap_or(0);
    if d.arena.get(paths_node).tag != "!!seq" {
        return Err(Error(format!(
            "DELPATHS: expected a sequence of sequences, but found {}",
            d.arena.get(paths_node).tag
        )));
    }
    let mut updated = context;
    for (i, child) in d
        .arena
        .get(paths_node)
        .content
        .clone()
        .into_iter()
        .enumerate()
    {
        if d.arena.get(child).tag != "!!seq" {
            return Err(Error(format!(
                "DELPATHS: expected entry [{i}] to be a sequence, but its a {}. Note that delpaths takes an array of path arrays, e.g. [[\"a\", \"b\"]]",
                d.arena.get(child).tag
            )));
        }
        let child_path = path_array_from_node(d, "DELPATHS", child)?;
        let tree = ExprNode {
            op: Operation::new(OpType::Delete),
            lhs: None,
            rhs: Some(Box::new(create_traversal_tree(
                &child_path,
                TraversePrefs::default(),
                false,
            ))),
        };
        updated = d.get_matching_nodes(updated, &tree)?;
    }
    Ok(updated)
}

pub fn get_path_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let node = d
            .arena
            .create_replacement(candidate, Kind::Sequence, "!!seq", "");
        let path = d.arena.get_path(candidate);
        let mut content = Vec::new();
        for element in path {
            let child = match element {
                PathElem::Key(key) => Node::scalar("!!str", &key),
                PathElem::Index(index) => Node::scalar("!!int", &index.to_string()),
            };
            content.push(d.arena.push(child));
        }
        d.arena.add_children(node, &content);
        results.push(node);
    }
    Ok(context.child_context(results))
}
