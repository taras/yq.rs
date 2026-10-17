//! Small operators: pipe, union, self, value, reference, if, with, map,
//! length, error, select, expression and the empty block.

use crate::context::Context;
use crate::eval::{collect_together, Evaluator};
use crate::node::{Kind, Node};
use crate::operation::{ExprNode, OpType, Operation};
use crate::ops::assign::assign_update_operator;
use crate::ops::traverse::splat;
use crate::ops::variables::variable_loop;
use crate::util::is_truthy_node;
use crate::Error;

pub fn pipe_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let lhs_exp = d.lhs(expr)?;
    if lhs_exp.op.op_type == OpType::AssignVariable {
        return variable_loop(d, context, expr);
    }
    let lhs = d.get_matching_nodes(context.clone_context(), lhs_exp)?;
    let rhs_context = context.child_context_shared(lhs.matching_nodes.clone());
    let rhs = d.get_matching_nodes(rhs_context, d.rhs(expr)?)?;
    Ok(context.child_context_shared(rhs.matching_nodes.clone()))
}

pub fn union_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let lhs = d.get_matching_nodes(context.clone_context(), d.lhs(expr)?)?;
    let rhs = d.get_matching_nodes(context.clone_context(), d.rhs(expr)?)?;
    let mut results: Vec<_> = lhs.nodes().to_vec();
    if !rhs.same_list(&lhs) {
        results.extend(rhs.nodes().iter().copied());
    }
    Ok(lhs.child_context(results))
}

pub fn self_operator(
    _d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    Ok(context)
}

pub fn empty_operator(
    _d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    Ok(context.child_context(Vec::new()))
}

pub fn reference_operator(
    _d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    match expr.op.reference {
        Some(id) => Ok(context.single_child_context(id)),
        None => Err(Error("reference operator without a node".to_string())),
    }
}

pub fn value_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let template = expr
        .op
        .node
        .clone()
        .unwrap_or_else(|| Node::scalar("!!null", "null"));
    if context.is_empty() {
        let id = d.arena.push(template);
        return Ok(context.single_child_context(id));
    }
    let mut results = Vec::new();
    for _ in 0..context.len() {
        results.push(d.arena.push(template.clone()));
    }
    Ok(context.child_context(results))
}

pub fn expression_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let expression = match &expr.op.prefs {
        crate::operation::Prefs::Expression(text) => text.clone(),
        _ => String::new(),
    };
    let parsed = crate::parser::parse_expression(&expression)?;
    d.get_matching_nodes_opt(context, parsed.as_ref())
}

pub fn error_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let rhs = d.get_matching_nodes_opt(context.read_only_clone(), expr.rhs())?;
    let message = match rhs.first() {
        Some(id) => d.arena.get(id).value.clone(),
        None => "aborted".to_string(),
    };
    Err(Error(message))
}

pub fn length_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let node = d.arena.get(candidate);
        let length = match node.kind {
            Kind::Scalar => {
                if node.tag == "!!null" {
                    0
                } else {
                    node.value.len()
                }
            }
            Kind::Mapping => node.content.len() / 2,
            Kind::Sequence => node.content.len(),
            Kind::Alias => 0,
        };
        let result =
            d.arena
                .create_replacement(candidate, Kind::Scalar, "!!int", &length.to_string());
        results.push(result);
    }
    Ok(context.child_context(results))
}

pub fn select_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    let rhs_exp = d.rhs(expr)?;
    for candidate in context.nodes().to_vec() {
        let rhs =
            d.get_matching_nodes(context.single_readonly_child_context(candidate), rhs_exp)?;
        let include = rhs
            .nodes()
            .iter()
            .any(|result| is_truthy_node(d.arena.get(*result)));
        if include {
            results.push(candidate);
        }
    }
    Ok(context.child_context(results))
}

fn if_then_else(
    d: &mut Evaluator,
    context: Context,
    condition_exp: Option<&ExprNode>,
    then_exp: Option<&ExprNode>,
    else_exp: Option<&ExprNode>,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let conditions = d.get_matching_nodes_opt(
            context.single_readonly_child_context(candidate),
            condition_exp,
        )?;
        let mut outcomes: Vec<Option<_>> = conditions.nodes().iter().map(|id| Some(*id)).collect();
        if outcomes.is_empty() {
            outcomes.push(None);
        }
        for condition in outcomes {
            let truthy = condition.is_some_and(|id| is_truthy_node(d.arena.get(id)));
            let branch_exp = if truthy { then_exp } else { else_exp };
            let branch =
                d.get_matching_nodes_opt(context.single_child_context(candidate), branch_exp)?;
            results.extend(branch.nodes().iter().copied());
        }
    }
    Ok(context.child_context(results))
}

pub fn if_then_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    if_then_else(d, context, expr.lhs(), expr.rhs(), None)
}

pub fn if_else_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let then_node = d.lhs(expr)?;
    if_then_else(d, context, then_node.lhs(), then_node.rhs(), expr.rhs())
}

pub fn with_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let rhs = d.rhs(expr)?;
    if rhs.op.op_type != OpType::Block {
        return Err(Error(format!(
            "with must be given a block (;), got {} instead",
            rhs.op.op_type.name()
        )));
    }
    let update_context = d.get_matching_nodes_opt(context.clone_context(), rhs.lhs())?;
    let update_exp = rhs.rhs();
    for candidate in update_context.nodes().to_vec() {
        d.get_matching_nodes_opt(update_context.single_child_context(candidate), update_exp)?;
    }
    Ok(context)
}

pub fn map_values_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    for candidate in context.nodes().to_vec() {
        let splatted = splat(
            d,
            context.single_child_context(candidate),
            crate::operation::TraversePrefs::default(),
        )?;
        let mut assign_op = Operation::new(OpType::Assign);
        assign_op.update_assign = true;
        let tree = ExprNode {
            op: assign_op,
            lhs: None,
            rhs: expr.rhs.clone(),
        };
        assign_update_operator(d, splatted, &tree)?;
    }
    Ok(context)
}

pub fn map_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    let rhs_exp = d.rhs(expr)?;
    for candidate in context.nodes().to_vec() {
        let splatted = splat(
            d,
            context.single_child_context(candidate),
            crate::operation::TraversePrefs::default(),
        )?;
        if splatted.is_empty() {
            results.push(d.arena.copy(candidate));
            continue;
        }
        let result = d.get_matching_nodes(splatted, rhs_exp)?;
        let self_exp = ExprNode::leaf(Operation::new(OpType::SelfReference));
        let collected = collect_together(d, &result, &self_exp)?;
        let style = d.arena.get(candidate).style;
        d.arena.get_mut(collected).style = style;
        results.push(collected);
    }
    Ok(context.child_context(results))
}
