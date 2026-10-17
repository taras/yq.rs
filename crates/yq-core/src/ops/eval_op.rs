//! `operator_eval.go`.

use crate::context::Context;
use crate::eval::Evaluator;
use crate::operation::ExprNode;
use crate::Error;

pub fn eval_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let expressions_context = d.get_matching_nodes_opt(context.read_only_clone(), expr.rhs())?;
    let mut expressions = Vec::new();
    for candidate in expressions_context.nodes().to_vec() {
        let text = d.arena.get(candidate).value.clone();
        expressions.push(crate::parser::parse_expression(&text)?);
    }
    let mut results = Vec::new();
    for _ in context.nodes().to_vec() {
        for expression in &expressions {
            let result = d.get_matching_nodes_opt(context.clone_context(), expression.as_ref())?;
            results.extend(result.nodes().iter().copied());
        }
    }
    Ok(context.child_context(results))
}
