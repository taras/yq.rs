//! `operator_assign.go`.

use crate::context::Context;
use crate::eval::{cross_function, Evaluator};
use crate::node::{AssignPrefs, NodeId};
use crate::operation::ExprNode;
use crate::Error;

pub fn assign_update_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let lhs = d.get_matching_nodes_opt(context.writable_clone(), expr.lhs())?;
    let prefs = expr.op.prefs.assign();
    if !expr.op.update_assign {
        let calculation =
            move |d: &mut Evaluator, _ctx: &Context, lhs: Option<NodeId>, rhs: Option<NodeId>| {
                if let (Some(lhs), Some(rhs)) = (lhs, rhs) {
                    if !prefs.only_write_null || d.arena.get(lhs).tag == "!!null" {
                        d.arena.update_from(lhs, rhs, prefs);
                    }
                }
                Ok(lhs)
            };
        cross_function(d, context.read_only_clone(), expr, &calculation, false)?;
        return Ok(context);
    }
    let rhs_exp = d.rhs(expr)?;
    for candidate in lhs.nodes().iter().rev().copied().collect::<Vec<_>>() {
        let rhs = d.get_matching_nodes(context.single_child_context(candidate), rhs_exp)?;
        if let Some(first) = rhs.first() {
            d.arena.update_from(candidate, first, prefs);
        }
    }
    Ok(context)
}

pub fn assign_attributes_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let lhs = d.get_matching_nodes_opt(context.writable_clone(), expr.lhs())?;
    let rhs_exp = d.rhs(expr)?;
    for candidate in lhs.nodes().to_vec() {
        let rhs =
            d.get_matching_nodes(context.single_readonly_child_context(candidate), rhs_exp)?;
        if let Some(first) = rhs.first() {
            let prefs = match &expr.op.prefs {
                crate::operation::Prefs::Assign(prefs) => *prefs,
                _ => AssignPrefs::default(),
            };
            if !prefs.only_write_null || d.arena.get(candidate).tag == "!!null" {
                d.arena.update_attributes_from(candidate, first, prefs);
            }
        }
    }
    Ok(context)
}
