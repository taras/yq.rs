//! `operator_variables.go` and `operator_reduce.go`.

use crate::context::Context;
use crate::eval::{evaluate_all_together, Evaluator};
use crate::operation::{ExprNode, OpType, Prefs};
use crate::Error;

pub fn get_variable_operator(
    _d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let name = &expr.op.string_value;
    let result = context.get_variable(name);
    Ok(match result {
        Some(list) => context.child_context_shared(list),
        None => context.child_context(Vec::new()),
    })
}

pub fn use_with_pipe(
    _d: &mut Evaluator,
    _context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    Err(Error(
        "must use variable with a pipe, e.g. `exp as $x | ...`".to_string(),
    ))
}

pub fn variable_loop(
    d: &mut Evaluator,
    context: Context,
    original: &ExprNode,
) -> Result<Context, Error> {
    if evaluate_all_together(&d.arena, &context) {
        return variable_loop_single_child(d, context, original);
    }
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let result =
            variable_loop_single_child(d, context.single_child_context(candidate), original)?;
        results.extend(result.nodes().iter().copied());
    }
    Ok(context.child_context(results))
}

fn variable_loop_single_child(
    d: &mut Evaluator,
    context: Context,
    original: &ExprNode,
) -> Result<Context, Error> {
    let variable_exp = d.lhs(original)?;
    let lhs = d.get_matching_nodes_opt(context.read_only_clone(), variable_exp.lhs())?;
    let variable_rhs = d.rhs(variable_exp)?;
    if variable_rhs.op.op_type != OpType::GetVariable {
        return Err(Error(
            "RHS of 'as' operator must be a variable name e.g. $foo".to_string(),
        ));
    }
    let variable_name = variable_rhs.op.string_value.clone();
    let is_reference = matches!(
        variable_exp.op.prefs,
        Prefs::AssignVar { is_reference: true }
    );
    let rhs_exp = d.rhs(original)?;
    let mut results = Vec::new();
    for candidate in lhs.nodes().to_vec() {
        let value = if is_reference {
            candidate
        } else {
            d.arena.copy(candidate)
        };
        let mut new_context = context.child_context_shared(context.matching_nodes.clone());
        new_context.set_variable(&variable_name, vec![value]);
        let rhs = d.get_matching_nodes(new_context, rhs_exp)?;
        results.extend(rhs.nodes().iter().copied());
    }
    if lhs.is_empty() {
        return d.get_matching_nodes(context, rhs_exp);
    }
    Ok(context.child_context(results))
}

pub fn reduce_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let lhs = d.lhs(expr)?;
    let rhs = d.rhs(expr)?;
    if lhs.op.op_type != OpType::AssignVariable {
        return Err(Error(format!(
            "reduce must be given a variables assignment, got {} instead",
            lhs.op.op_type.name()
        )));
    }
    if rhs.op.op_type != OpType::Block {
        return Err(Error(format!(
            "reduce must be given a block, got {} instead",
            rhs.op.op_type.name()
        )));
    }
    if evaluate_all_together(&d.arena, &context) {
        return reduce_single(d, context, expr);
    }
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let result = reduce_single(d, context.single_child_context(candidate), expr)?;
        results.extend(result.nodes().iter().copied());
    }
    Ok(context.child_context(results))
}

fn reduce_single(d: &mut Evaluator, context: Context, expr: &ExprNode) -> Result<Context, Error> {
    let lhs = d.lhs(expr)?;
    let rhs = d.rhs(expr)?;
    let array = d.get_matching_nodes_opt(context.clone_context(), lhs.lhs())?;
    let variable_name = d.rhs(lhs)?.op.string_value.clone();
    let mut accum = d.get_matching_nodes_opt(context.clone_context(), rhs.lhs())?;
    let block_exp = rhs.rhs();
    for candidate in array.nodes().to_vec() {
        accum.set_variable(&variable_name, vec![candidate]);
        accum = d.get_matching_nodes_opt(accum, block_exp)?;
    }
    Ok(accum)
}
