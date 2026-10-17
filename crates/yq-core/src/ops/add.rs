//! `operator_add.go`.

use crate::context::Context;
use crate::eval::{compound_assign_function, cross_function, Evaluator};
use crate::node::{find_key_in_map, Kind, NodeId};
use crate::operation::{ExprNode, OpType, Operation};
use crate::util::{checked_int64_arithmetic, format_float, format_int, parse_float, parse_int64};
use crate::Error;

fn create_add_op(_op: &Operation, lhs: ExprNode, rhs: ExprNode) -> ExprNode {
    ExprNode::binary(Operation::new(OpType::Add), lhs, rhs)
}

pub fn add_assign_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    compound_assign_function(d, context, expr, create_add_op)
}

pub fn add_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let calc_when_empty = !context.is_empty();
    cross_function(d, context.read_only_clone(), expr, &add, calc_when_empty)
}

fn to_nodes(d: &mut Evaluator, candidate: NodeId, lhs: NodeId) -> Vec<NodeId> {
    if d.arena.get(candidate).tag == "!!null" {
        return Vec::new();
    }
    let clone = d.arena.copy(candidate);
    match d.arena.get(candidate).kind {
        Kind::Sequence => d.arena.get(clone).content.clone(),
        _ => {
            if let Some(first) = d.arena.get(lhs).content.first().copied() {
                let style = d.arena.get(first).style;
                d.arena.get_mut(clone).style = style;
            }
            vec![clone]
        }
    }
}

pub fn add(
    d: &mut Evaluator,
    context: &Context,
    lhs: Option<NodeId>,
    rhs: Option<NodeId>,
) -> Result<Option<NodeId>, Error> {
    let (lhs, rhs) = match (lhs, rhs) {
        (None, None) => return Ok(None),
        (None, Some(rhs)) => return Ok(Some(d.arena.copy(rhs))),
        (Some(lhs), None) => return Ok(Some(d.arena.copy(lhs))),
        (Some(lhs), Some(rhs)) => (lhs, rhs),
    };
    if d.arena.get(lhs).tag == "!!null" {
        return Ok(Some(d.arena.copy_as_replacement(lhs, rhs)));
    }
    let target = d.arena.copy_without_content(lhs);
    match d.arena.get(lhs).kind {
        Kind::Mapping => {
            if d.arena.get(rhs).kind != Kind::Mapping {
                return Err(Error(format!(
                    "{} ({}) cannot be added to a {} ({})",
                    d.arena.get(rhs).tag,
                    d.arena.get_nice_path(rhs),
                    d.arena.get(lhs).tag,
                    d.arena.get_nice_path(lhs)
                )));
            }
            add_maps(d, target, lhs, rhs);
        }
        Kind::Sequence => add_sequences(d, target, lhs, rhs),
        Kind::Scalar => {
            if d.arena.get(rhs).kind != Kind::Scalar {
                return Err(Error(format!(
                    "{} ({}) cannot be added to a {} ({})",
                    d.arena.get(rhs).tag,
                    d.arena.get_nice_path(rhs),
                    d.arena.get(lhs).tag,
                    d.arena.get_nice_path(lhs)
                )));
            }
            let style = d.arena.get(lhs).style;
            {
                let node = d.arena.get_mut(target);
                node.kind = Kind::Scalar;
                node.style = style;
            }
            add_scalars(d, context, target, lhs, rhs)?;
        }
        Kind::Alias => {}
    }
    Ok(Some(target))
}

fn add_scalars(
    d: &mut Evaluator,
    context: &Context,
    target: NodeId,
    lhs: NodeId,
    rhs: NodeId,
) -> Result<(), Error> {
    let mut lhs_tag = d.arena.get(lhs).tag.clone();
    let rhs_tag = d.arena.guess_tag_from_custom_type(rhs);
    let mut lhs_is_custom = false;
    if !lhs_tag.starts_with("!!") {
        lhs_tag = d.arena.guess_tag_from_custom_type(lhs);
        lhs_is_custom = true;
    }
    let lhs_value = d.arena.get(lhs).value.clone();
    let rhs_value = d.arena.get(rhs).value.clone();
    let mut is_datetime = d.arena.get(lhs).tag == "!!timestamp";
    if lhs_tag == "!!str" && context.datetime_layout() != crate::context::RFC3339 {
        is_datetime =
            crate::ops::datetime::parse_datetime(context.datetime_layout(), &lhs_value).is_ok();
    }
    if is_datetime {
        let layout = context.datetime_layout().to_string();
        let value = crate::ops::datetime::add_duration(&layout, &lhs_value, &rhs_value)?;
        d.arena.get_mut(target).value = value;
        return Ok(());
    }
    let original_lhs_tag = d.arena.get(lhs).tag.clone();
    if lhs_tag == "!!str" {
        let node = d.arena.get_mut(target);
        node.tag = original_lhs_tag;
        node.value = if rhs_tag == "!!null" {
            lhs_value
        } else {
            format!("{lhs_value}{rhs_value}")
        };
    } else if rhs_tag == "!!str" {
        let rhs_original_tag = d.arena.get(rhs).tag.clone();
        let node = d.arena.get_mut(target);
        node.tag = rhs_original_tag;
        node.value = format!("{lhs_value}{rhs_value}");
    } else if lhs_tag == "!!int" && rhs_tag == "!!int" {
        let (format, lhs_num) = parse_int64(&lhs_value)?;
        let (_, rhs_num) = parse_int64(&rhs_value)?;
        let sum = checked_int64_arithmetic('+', lhs_num, rhs_num)?;
        let node = d.arena.get_mut(target);
        node.tag = original_lhs_tag;
        node.value = format_int(format, sum);
    } else if (lhs_tag == "!!int" || lhs_tag == "!!float")
        && (rhs_tag == "!!int" || rhs_tag == "!!float")
    {
        let sum = parse_float(&lhs_value)? + parse_float(&rhs_value)?;
        let node = d.arena.get_mut(target);
        node.tag = if lhs_is_custom {
            original_lhs_tag
        } else {
            "!!float".to_string()
        };
        node.value = format_float(sum);
    } else {
        return Err(Error(format!("{lhs_tag} cannot be added to {rhs_tag}")));
    }
    Ok(())
}

fn add_sequences(d: &mut Evaluator, target: NodeId, lhs: NodeId, rhs: NodeId) {
    let lhs_tag = d.arena.get(lhs).tag.clone();
    let lhs_content = d.arena.get(lhs).content.clone();
    {
        let node = d.arena.get_mut(target);
        node.kind = Kind::Sequence;
        if lhs_content.is_empty() {
            node.style = 0;
        }
        node.tag = lhs_tag;
    }
    let extra = to_nodes(d, rhs, lhs);
    d.arena.add_children(target, &lhs_content);
    d.arena.add_children(target, &extra);
}

fn add_maps(d: &mut Evaluator, target: NodeId, lhs: NodeId, rhs: NodeId) {
    let lhs_content = d.arena.get(lhs).content.clone();
    if lhs_content.is_empty() {
        d.arena.get_mut(target).style = 0;
    }
    d.arena.get_mut(target).content = Vec::new();
    d.arena.get_mut(target).kind = Kind::Mapping;
    d.arena.add_children(target, &lhs_content);
    let rhs_content = d.arena.get(rhs).content.clone();
    let mut index = 0;
    while index + 1 < rhs_content.len() {
        let key = rhs_content[index];
        let value = rhs_content[index + 1];
        match find_key_in_map(&mut d.arena, target, key) {
            None => {
                d.arena.add_key_value_child(target, key, value);
            }
            Some(index_in_lhs) => {
                let old_value = d.arena.get(target).content[index_in_lhs + 1];
                let new_value = d.arena.copy_as_replacement(old_value, value);
                d.arena.get_mut(target).content[index_in_lhs + 1] = new_value;
            }
        }
        index += 2;
    }
    let lhs_style = d.arena.get(lhs).style;
    let lhs_tag = d.arena.get(lhs).tag.clone();
    let node = d.arena.get_mut(target);
    node.kind = Kind::Mapping;
    if !lhs_content.is_empty() {
        node.style = lhs_style;
    }
    node.tag = lhs_tag;
}
