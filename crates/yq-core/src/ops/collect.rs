//! `operator_collect.go`.

use crate::context::Context;
use crate::eval::{evaluate_all_together, Evaluator};
use crate::node::{Kind, Node};
use crate::operation::ExprNode;
use crate::Error;

pub fn collect_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    if context.is_empty() {
        let node = d.arena.push(Node {
            kind: Kind::Sequence,
            tag: "!!seq".to_string(),
            value: "[]".to_string(),
            ..Node::default()
        });
        return Ok(context.single_child_context(node));
    }
    let rhs_exp = d.rhs(expr)?;
    if evaluate_all_together(&d.arena, &context) {
        let collected = crate::eval::collect_together(d, &context, rhs_exp)?;
        return Ok(context.single_child_context(collected));
    }
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let collect_candidate = d
            .arena
            .create_replacement(candidate, Kind::Sequence, "!!seq", "");
        let collect_results =
            d.get_matching_nodes(context.single_child_context(candidate), rhs_exp)?;
        for result in collect_results.nodes().to_vec() {
            d.arena.add_child(collect_candidate, result);
        }
        results.push(collect_candidate);
    }
    Ok(context.child_context(results))
}
