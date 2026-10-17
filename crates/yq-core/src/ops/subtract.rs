//! `operator_subtract.go`.

use crate::context::Context;
use crate::eval::{compound_assign_function, cross_function, Evaluator};
use crate::node::{recursive_node_equal, Kind, NodeId};
use crate::operation::{ExprNode, OpType, Operation};
use crate::util::{checked_int64_arithmetic, format_float, format_int, parse_float, parse_int64};
use crate::Error;

fn create_subtract_op(_op: &Operation, lhs: ExprNode, rhs: ExprNode) -> ExprNode {
    ExprNode::binary(Operation::new(OpType::Subtract), lhs, rhs)
}

pub fn subtract_assign_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    compound_assign_function(d, context, expr, create_subtract_op)
}

pub fn subtract_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    cross_function(d, context.read_only_clone(), expr, &subtract, false)
}

fn subtract_array(d: &mut Evaluator, lhs: NodeId, rhs: NodeId) -> Vec<NodeId> {
    let lhs_content = d.arena.get(lhs).content.clone();
    let rhs_content = d.arena.get(rhs).content.clone();
    let mut result = Vec::new();
    for item in lhs_content {
        let mut include = true;
        for other in &rhs_content {
            if recursive_node_equal(&mut d.arena, item, *other) {
                include = false;
                break;
            }
        }
        if include {
            result.push(item);
        }
    }
    result
}

fn subtract(
    d: &mut Evaluator,
    context: &Context,
    lhs: Option<NodeId>,
    rhs: Option<NodeId>,
) -> Result<Option<NodeId>, Error> {
    let (lhs, rhs) = match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => (lhs, rhs),
        _ => return Ok(None),
    };
    if d.arena.get(lhs).tag == "!!null" {
        return Ok(Some(d.arena.copy_as_replacement(lhs, rhs)));
    }
    let target = d.arena.copy_without_content(lhs);
    match d.arena.get(lhs).kind {
        Kind::Mapping => {
            return Err(Error("maps not yet supported for subtraction".to_string()));
        }
        Kind::Sequence => {
            if d.arena.get(rhs).kind != Kind::Sequence {
                return Err(Error(format!(
                    "{} ({}) cannot be subtracted from {}",
                    d.arena.get(rhs).tag,
                    d.arena.get_nice_path(rhs),
                    d.arena.get(lhs).tag
                )));
            }
            let content = subtract_array(d, lhs, rhs);
            d.arena.get_mut(target).content = content;
        }
        Kind::Scalar => {
            if d.arena.get(rhs).kind != Kind::Scalar {
                return Err(Error(format!(
                    "{} ({}) cannot be subtracted from {}",
                    d.arena.get(rhs).tag,
                    d.arena.get_nice_path(rhs),
                    d.arena.get(lhs).tag
                )));
            }
            let style = d.arena.get(lhs).style;
            {
                let node = d.arena.get_mut(target);
                node.kind = Kind::Scalar;
                node.style = style;
            }
            subtract_scalars(d, context, target, lhs, rhs)?;
        }
        Kind::Alias => {}
    }
    Ok(Some(target))
}

fn subtract_scalars(
    d: &mut Evaluator,
    context: &Context,
    target: NodeId,
    lhs: NodeId,
    rhs: NodeId,
) -> Result<(), Error> {
    let original_lhs_tag = d.arena.get(lhs).tag.clone();
    let original_rhs_tag = d.arena.get(rhs).tag.clone();
    let mut lhs_tag = original_lhs_tag.clone();
    let mut rhs_tag = original_rhs_tag.clone();
    let mut lhs_is_custom = false;
    if !lhs_tag.starts_with("!!") {
        lhs_tag = d.arena.guess_tag_from_custom_type(lhs);
        lhs_is_custom = true;
    }
    if !rhs_tag.starts_with("!!") {
        rhs_tag = d.arena.guess_tag_from_custom_type(rhs);
    }
    let lhs_value = d.arena.get(lhs).value.clone();
    let rhs_value = d.arena.get(rhs).value.clone();
    let mut is_datetime = lhs_tag == "!!timestamp";
    if lhs_tag == "!!str" && context.datetime_layout() != crate::context::RFC3339 {
        is_datetime =
            crate::ops::datetime::parse_datetime(context.datetime_layout(), &lhs_value).is_ok();
    }
    if is_datetime {
        let layout = context.datetime_layout().to_string();
        let duration = if let Some(stripped) = rhs_value.strip_prefix('-') {
            stripped.to_string()
        } else {
            format!("-{rhs_value}")
        };
        let value = crate::ops::datetime::add_duration(&layout, &lhs_value, &duration)?;
        d.arena.get_mut(target).value = value;
        return Ok(());
    }
    if lhs_tag == "!!str" {
        return Err(Error("strings cannot be subtracted".to_string()));
    }
    if lhs_tag == "!!int" && rhs_tag == "!!int" {
        let (format, lhs_num) = parse_int64(&lhs_value)?;
        let (_, rhs_num) = parse_int64(&rhs_value)?;
        let result = checked_int64_arithmetic('-', lhs_num, rhs_num)?;
        let node = d.arena.get_mut(target);
        node.tag = original_lhs_tag;
        node.value = format_int(format, result);
    } else if (lhs_tag == "!!int" || lhs_tag == "!!float")
        && (rhs_tag == "!!int" || rhs_tag == "!!float")
    {
        let result = parse_float(&lhs_value)? - parse_float(&rhs_value)?;
        let node = d.arena.get_mut(target);
        node.tag = if lhs_is_custom {
            original_lhs_tag
        } else {
            "!!float".to_string()
        };
        node.value = format_float(result);
    } else {
        return Err(Error(format!(
            "{original_lhs_tag} cannot be added to {original_rhs_tag}"
        )));
    }
    Ok(())
}
