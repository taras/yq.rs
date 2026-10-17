//! `operator_booleans.go` and `operator_alternative.go`.

use crate::context::Context;
use crate::eval::{create_boolean_candidate, cross_function_with_prefs, CrossPrefs, Evaluator};
use crate::node::{Kind, NodeId};
use crate::operation::ExprNode;
use crate::util::is_truthy_node;
use crate::Error;

fn truthy(d: &Evaluator, node: Option<NodeId>) -> bool {
    node.is_some_and(|id| is_truthy_node(d.arena.get(id)))
}

fn return_rhs_truthy(
    d: &mut Evaluator,
    _context: &Context,
    lhs: Option<NodeId>,
    rhs: Option<NodeId>,
) -> Result<Option<NodeId>, Error> {
    let owner = lhs.or(rhs);
    let value = truthy(d, rhs);
    Ok(Some(create_boolean_candidate(&mut d.arena, owner, value)))
}

fn boolean_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
    target: bool,
) -> Result<Context, Error> {
    let lhs_result =
        move |d: &mut Evaluator, lhs: Option<NodeId>| -> Result<Option<NodeId>, Error> {
            if truthy(d, lhs) != target {
                return Ok(None);
            }
            Ok(Some(create_boolean_candidate(&mut d.arena, lhs, target)))
        };
    let prefs = CrossPrefs {
        calc_when_empty: true,
        lhs_result_value: Some(&lhs_result),
        calculation: &return_rhs_truthy,
    };
    cross_function_with_prefs(d, context.read_only_clone(), expr, &prefs)
}

pub fn or_operator(d: &mut Evaluator, context: Context, expr: &ExprNode) -> Result<Context, Error> {
    boolean_operator(d, context, expr, true)
}

pub fn and_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    boolean_operator(d, context, expr, false)
}

pub fn not_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let value = is_truthy_node(d.arena.get(candidate));
        results.push(create_boolean_candidate(
            &mut d.arena,
            Some(candidate),
            !value,
        ));
    }
    Ok(context.child_context(results))
}

fn find_boolean(
    want: bool,
    d: &mut Evaluator,
    context: &Context,
    expr: Option<&ExprNode>,
    sequence: NodeId,
) -> Result<bool, Error> {
    for child in d.arena.get(sequence).content.clone() {
        let mut node = child;
        if let Some(expr) = expr {
            let rhs = d.get_matching_nodes(context.single_readonly_child_context(child), expr)?;
            match rhs.first() {
                Some(first) => node = first,
                None => continue,
            }
        }
        if is_truthy_node(d.arena.get(node)) == want {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn all_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.get(candidate).kind != Kind::Sequence {
            return Err(Error(format!(
                "all only supports arrays, was {}",
                d.arena.get(candidate).tag
            )));
        }
        let found = find_boolean(false, d, &context, expr.rhs(), candidate)?;
        results.push(create_boolean_candidate(
            &mut d.arena,
            Some(candidate),
            !found,
        ));
    }
    Ok(context.child_context(results))
}

pub fn any_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.get(candidate).kind != Kind::Sequence {
            return Err(Error(format!(
                "any only supports arrays, was {}",
                d.arena.get(candidate).tag
            )));
        }
        let found = find_boolean(true, d, &context, expr.rhs(), candidate)?;
        results.push(create_boolean_candidate(
            &mut d.arena,
            Some(candidate),
            found,
        ));
    }
    Ok(context.child_context(results))
}

pub fn alternative_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let lhs_result = |d: &mut Evaluator, lhs: Option<NodeId>| -> Result<Option<NodeId>, Error> {
        match lhs {
            Some(id) if is_truthy_node(d.arena.get(id)) => Ok(Some(id)),
            _ => Ok(None),
        }
    };
    let calculation = |d: &mut Evaluator,
                       _ctx: &Context,
                       lhs: Option<NodeId>,
                       rhs: Option<NodeId>|
     -> Result<Option<NodeId>, Error> {
        match (lhs, rhs) {
            (None, rhs) => Ok(rhs),
            (lhs, None) => Ok(lhs),
            (Some(lhs), Some(rhs)) => {
                if is_truthy_node(d.arena.get(lhs)) {
                    Ok(Some(lhs))
                } else {
                    Ok(Some(rhs))
                }
            }
        }
    };
    let prefs = CrossPrefs {
        calc_when_empty: true,
        lhs_result_value: Some(&lhs_result),
        calculation: &calculation,
    };
    cross_function_with_prefs(d, context, expr, &prefs)
}
