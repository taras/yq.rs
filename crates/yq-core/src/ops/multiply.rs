//! `operator_multiply.go`.

use crate::context::Context;
use crate::eval::{compound_assign_function, create_traversal_tree, cross_function, Evaluator};
use crate::node::{AssignPrefs, Kind, NodeId};
use crate::operation::{
    ExprNode, MultiplyPrefs, OpType, Operation, Prefs, RecursiveDescentPrefs, TraversePrefs,
};
use crate::ops::traverse::recursive_descent;
use crate::util::{
    checked_int64_arithmetic, format_float, format_int, parse_float, parse_int, parse_int64,
};
use crate::Error;

fn create_multiply_op(op: &Operation, lhs: ExprNode, rhs: ExprNode) -> ExprNode {
    ExprNode::binary(
        Operation::with_prefs(OpType::Multiply, op.prefs.clone()),
        lhs,
        rhs,
    )
}

pub fn multiply_assign_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    compound_assign_function(d, context, expr, create_multiply_op)
}

fn multiply_prefs(prefs: &Prefs) -> MultiplyPrefs {
    match prefs {
        Prefs::Multiply(prefs) => *prefs,
        _ => MultiplyPrefs::default(),
    }
}

pub fn multiply_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let prefs = multiply_prefs(&expr.op.prefs);
    let calculation = move |d: &mut Evaluator,
                            context: &Context,
                            lhs: Option<NodeId>,
                            rhs: Option<NodeId>| {
        match (lhs, rhs) {
            (Some(lhs), Some(rhs)) => multiply(d, context, lhs, rhs, prefs).map(Some),
            _ => Ok(None),
        }
    };
    cross_function(d, context.read_only_clone(), expr, &calculation, false)
}

pub fn multiply(
    d: &mut Evaluator,
    context: &Context,
    lhs: NodeId,
    rhs: NodeId,
    prefs: MultiplyPrefs,
) -> Result<NodeId, Error> {
    let (leading_content, head_comment, foot_comment) = {
        let lhs_node = d.arena.get(lhs);
        let rhs_node = d.arena.get(rhs);
        let mut leading = rhs_node.leading_content.clone();
        let mut head = rhs_node.head_comment.clone();
        let mut foot = rhs_node.foot_comment.clone();
        if !lhs_node.head_comment.is_empty() || !lhs_node.leading_content.is_empty() {
            head = lhs_node.head_comment.clone();
            leading = lhs_node.leading_content.clone();
        }
        if !lhs_node.foot_comment.is_empty() {
            foot = lhs_node.foot_comment.clone();
        }
        (leading, head, foot)
    };
    if d.arena.get(rhs).tag == "!!null" {
        return Ok(d.arena.copy(lhs));
    }
    let lhs_kind = d.arena.get(lhs).kind;
    let rhs_kind = d.arena.get(rhs).kind;
    let lhs_null = d.arena.get(lhs).tag == "!!null";
    let mergeable = (lhs_kind == Kind::Mapping && rhs_kind == Kind::Mapping)
        || (lhs_null && rhs_kind == Kind::Mapping)
        || (lhs_kind == Kind::Sequence && rhs_kind == Kind::Sequence)
        || (lhs_null && rhs_kind == Kind::Sequence);
    if mergeable {
        let blank = d.arena.copy(lhs);
        {
            let node = d.arena.get_mut(blank);
            node.leading_content = leading_content;
            node.head_comment = head_comment;
            node.foot_comment = foot_comment;
        }
        return merge_objects(d, context.writable_clone(), blank, rhs, prefs);
    }
    multiply_scalars(d, lhs, rhs)
}

fn multiply_scalars(d: &mut Evaluator, lhs: NodeId, rhs: NodeId) -> Result<NodeId, Error> {
    let original_lhs_tag = d.arena.get(lhs).tag.clone();
    let mut lhs_tag = original_lhs_tag.clone();
    let rhs_tag = d.arena.guess_tag_from_custom_type(rhs);
    let mut lhs_is_custom = false;
    if !lhs_tag.starts_with("!!") {
        lhs_tag = d.arena.guess_tag_from_custom_type(lhs);
        lhs_is_custom = true;
    }
    if lhs_tag == "!!int" && rhs_tag == "!!int" {
        return multiply_integers(d, lhs, rhs);
    }
    if (lhs_tag == "!!int" || lhs_tag == "!!float") && (rhs_tag == "!!int" || rhs_tag == "!!float")
    {
        return multiply_floats(d, lhs, rhs, lhs_is_custom);
    }
    if (lhs_tag == "!!str" && rhs_tag == "!!int") || (lhs_tag == "!!int" && rhs_tag == "!!str") {
        return repeat_string(d, lhs, rhs);
    }
    Err(Error(format!(
        "cannot multiply {} with {}",
        original_lhs_tag,
        d.arena.get(rhs).tag
    )))
}

fn multiply_floats(
    d: &mut Evaluator,
    lhs: NodeId,
    rhs: NodeId,
    lhs_is_custom: bool,
) -> Result<NodeId, Error> {
    let target = d.arena.copy_without_content(lhs);
    let lhs_node = d.arena.get(lhs).clone();
    let rhs_value = d.arena.get(rhs).value.clone();
    let product = parse_float(&lhs_node.value)? * parse_float(&rhs_value)?;
    let node = d.arena.get_mut(target);
    node.kind = Kind::Scalar;
    node.style = lhs_node.style;
    node.tag = if lhs_is_custom {
        lhs_node.tag
    } else {
        "!!float".to_string()
    };
    node.value = format_float(product);
    Ok(target)
}

fn multiply_integers(d: &mut Evaluator, lhs: NodeId, rhs: NodeId) -> Result<NodeId, Error> {
    let target = d.arena.copy_without_content(lhs);
    let lhs_node = d.arena.get(lhs).clone();
    let rhs_value = d.arena.get(rhs).value.clone();
    let (format, lhs_num) = parse_int64(&lhs_node.value)?;
    let (_, rhs_num) = parse_int64(&rhs_value)?;
    let product = checked_int64_arithmetic('*', lhs_num, rhs_num)?;
    let node = d.arena.get_mut(target);
    node.kind = Kind::Scalar;
    node.style = lhs_node.style;
    node.tag = lhs_node.tag;
    node.value = format_int(format, product);
    Ok(target)
}

fn repeat_string(d: &mut Evaluator, lhs: NodeId, rhs: NodeId) -> Result<NodeId, Error> {
    let (string_node, int_node) = if d.arena.get(lhs).tag == "!!str" {
        (lhs, rhs)
    } else {
        (rhs, lhs)
    };
    let target = d.arena.copy_without_content(lhs);
    d.arena
        .update_attributes_from(target, string_node, AssignPrefs::default());
    let count = parse_int(&d.arena.get(int_node).value)?;
    if count < 0 {
        return Err(Error(format!(
            "cannot repeat string by a negative number ({count})"
        )));
    }
    let text = d.arena.get(string_node).value.clone();
    let max_len: i64 = 10 * 1024 * 1024;
    if count > 0 && text.len() as i64 > max_len / count {
        return Err(Error(format!(
            "result of repeating string ({} bytes) by {} would exceed {} bytes",
            text.len(),
            count,
            max_len
        )));
    }
    d.arena.get_mut(target).value = text.repeat(count as usize);
    Ok(target)
}

fn merge_objects(
    d: &mut Evaluator,
    context: Context,
    lhs: NodeId,
    rhs: NodeId,
    prefs: MultiplyPrefs,
) -> Result<NodeId, Error> {
    let descent_prefs = RecursiveDescentPrefs {
        recurse_array: prefs.deep_merge_arrays,
        traverse_prefs: TraversePrefs {
            dont_follow_alias: true,
            include_map_keys: true,
            exact_key_match: true,
            ..TraversePrefs::default()
        },
    };
    let mut results = Vec::new();
    recursive_descent(
        d,
        &mut results,
        context.single_child_context(rhs),
        descent_prefs,
    )?;
    let path_index_to_start_from = match results.first() {
        Some(first) => d.arena.get_path(*first).len(),
        None => 0,
    };
    for candidate in results {
        apply_assignment(d, &context, path_index_to_start_from, lhs, candidate, prefs)?;
    }
    Ok(lhs)
}

fn apply_assignment(
    d: &mut Evaluator,
    context: &Context,
    path_index_to_start_from: usize,
    lhs: NodeId,
    rhs: NodeId,
    prefs: MultiplyPrefs,
) -> Result<(), Error> {
    let full_path = d.arena.get_path(rhs);
    let lhs_path = if full_path.len() >= path_index_to_start_from {
        full_path[path_index_to_start_from..].to_vec()
    } else {
        Vec::new()
    };
    let rhs_kind = d.arena.get(rhs).kind;
    let mut assignment =
        Operation::with_prefs(OpType::AssignAttributes, Prefs::Assign(prefs.assign_prefs));
    if prefs.append_arrays && rhs_kind == Kind::Sequence {
        assignment.op_type = OpType::AddAssign;
    } else if (!prefs.deep_merge_arrays && rhs_kind == Kind::Sequence)
        || rhs_kind == Kind::Scalar
        || rhs_kind == Kind::Alias
    {
        assignment.op_type = OpType::Assign;
        assignment.update_assign = false;
    }
    let is_map_key = d.arena.get(rhs).is_map_key;
    let tree = ExprNode::binary(
        assignment,
        create_traversal_tree(&lhs_path, prefs.traverse_prefs, is_map_key),
        ExprNode::leaf(Operation::reference(rhs)),
    );
    d.get_matching_nodes(context.single_child_context(lhs), &tree)?;
    Ok(())
}
