//! `operator_divide.go` and `operator_modulo.go`.

use crate::context::Context;
use crate::eval::{cross_function, Evaluator};
use crate::node::{Kind, NodeId};
use crate::operation::ExprNode;
use crate::ops::strings::split_string;
use crate::util::{format_float, format_int, parse_float, parse_int64};
use crate::Error;

pub fn divide_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    cross_function(d, context.read_only_clone(), expr, &divide, false)
}

fn cannot(d: &Evaluator, verb: &str, lhs: NodeId, rhs: NodeId) -> Error {
    Error(format!(
        "{} ({}) cannot {} {} ({})",
        d.arena.get(lhs).tag,
        d.arena.get_nice_path(lhs),
        verb,
        d.arena.get(rhs).tag,
        d.arena.get_nice_path(rhs)
    ))
}

fn divide(
    d: &mut Evaluator,
    _context: &Context,
    lhs: Option<NodeId>,
    rhs: Option<NodeId>,
) -> Result<Option<NodeId>, Error> {
    let (lhs, rhs) = match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => (lhs, rhs),
        _ => return Ok(None),
    };
    if d.arena.get(lhs).tag == "!!null" {
        return Err(cannot(d, "be divided by", lhs, rhs));
    }
    let target = d.arena.copy_without_content(lhs);
    if d.arena.get(lhs).kind == Kind::Scalar && d.arena.get(rhs).kind == Kind::Scalar {
        divide_scalars(d, target, lhs, rhs)?;
    } else {
        return Err(cannot(d, "be divided by", lhs, rhs));
    }
    Ok(Some(target))
}

fn divide_scalars(
    d: &mut Evaluator,
    target: NodeId,
    lhs: NodeId,
    rhs: NodeId,
) -> Result<(), Error> {
    let original_lhs_tag = d.arena.get(lhs).tag.clone();
    let mut lhs_tag = original_lhs_tag.clone();
    let rhs_tag = d.arena.guess_tag_from_custom_type(rhs);
    let mut lhs_is_custom = false;
    if !lhs_tag.starts_with("!!") {
        lhs_tag = d.arena.guess_tag_from_custom_type(lhs);
        lhs_is_custom = true;
    }
    let lhs_value = d.arena.get(lhs).value.clone();
    let rhs_value = d.arena.get(rhs).value.clone();
    if lhs_tag == "!!str" && rhs_tag == "!!str" {
        let parts = split_string(d, &lhs_value, &rhs_value);
        {
            let node = d.arena.get_mut(target);
            node.kind = Kind::Sequence;
            node.tag = "!!seq".to_string();
        }
        d.arena.add_children(target, &parts);
    } else if (lhs_tag == "!!int" || lhs_tag == "!!float")
        && (rhs_tag == "!!int" || rhs_tag == "!!float")
    {
        let quotient = parse_float(&lhs_value)? / parse_float(&rhs_value)?;
        let style = d.arena.get(lhs).style;
        let node = d.arena.get_mut(target);
        node.kind = Kind::Scalar;
        node.style = style;
        node.tag = if lhs_is_custom {
            original_lhs_tag
        } else {
            "!!float".to_string()
        };
        node.value = format_float(quotient);
    } else {
        return Err(Error(format!("{lhs_tag} cannot be divided by {rhs_tag}")));
    }
    Ok(())
}

pub fn modulo_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    cross_function(d, context.read_only_clone(), expr, &modulo, false)
}

fn modulo(
    d: &mut Evaluator,
    _context: &Context,
    lhs: Option<NodeId>,
    rhs: Option<NodeId>,
) -> Result<Option<NodeId>, Error> {
    let (lhs, rhs) = match (lhs, rhs) {
        (Some(lhs), Some(rhs)) => (lhs, rhs),
        _ => return Ok(None),
    };
    if d.arena.get(lhs).tag == "!!null" {
        return Err(cannot(d, "modulo by", lhs, rhs));
    }
    let target = d.arena.copy_without_content(lhs);
    if d.arena.get(lhs).kind == Kind::Scalar && d.arena.get(rhs).kind == Kind::Scalar {
        modulo_scalars(d, target, lhs, rhs)?;
    } else {
        return Err(cannot(d, "modulo by", lhs, rhs));
    }
    Ok(Some(target))
}

fn modulo_scalars(
    d: &mut Evaluator,
    target: NodeId,
    lhs: NodeId,
    rhs: NodeId,
) -> Result<(), Error> {
    let original_lhs_tag = d.arena.get(lhs).tag.clone();
    let mut lhs_tag = original_lhs_tag.clone();
    let rhs_tag = d.arena.guess_tag_from_custom_type(rhs);
    let mut lhs_is_custom = false;
    if !lhs_tag.starts_with("!!") {
        lhs_tag = d.arena.guess_tag_from_custom_type(lhs);
        lhs_is_custom = true;
    }
    let lhs_value = d.arena.get(lhs).value.clone();
    let rhs_value = d.arena.get(rhs).value.clone();
    let style = d.arena.get(lhs).style;
    if lhs_tag == "!!int" && rhs_tag == "!!int" {
        let (format, lhs_num) = parse_int64(&lhs_value)?;
        let (_, rhs_num) = parse_int64(&rhs_value)?;
        if rhs_num == 0 {
            return Err(Error("cannot modulo by 0".to_string()));
        }
        let remainder = lhs_num.wrapping_rem(rhs_num);
        let node = d.arena.get_mut(target);
        node.kind = Kind::Scalar;
        node.style = style;
        node.tag = original_lhs_tag;
        node.value = format_int(format, remainder);
    } else if (lhs_tag == "!!int" || lhs_tag == "!!float")
        && (rhs_tag == "!!int" || rhs_tag == "!!float")
    {
        let remainder = parse_float(&lhs_value)? % parse_float(&rhs_value)?;
        let node = d.arena.get_mut(target);
        node.kind = Kind::Scalar;
        node.style = style;
        node.tag = if lhs_is_custom {
            original_lhs_tag
        } else {
            "!!float".to_string()
        };
        node.value = format_float(remainder);
    } else {
        return Err(Error(format!("{lhs_tag} cannot modulo by {rhs_tag}")));
    }
    Ok(())
}
