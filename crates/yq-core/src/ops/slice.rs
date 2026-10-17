//! `operator_slice.go`.

use crate::context::Context;
use crate::eval::Evaluator;
use crate::node::{Kind, NodeId};
use crate::operation::ExprNode;
use crate::util::parse_int;
use crate::Error;

fn get_slice_number(
    d: &mut Evaluator,
    context: &Context,
    node: NodeId,
    expr: Option<&ExprNode>,
) -> Result<i64, Error> {
    let result = d.get_matching_nodes_opt(context.single_child_context(node), expr)?;
    if result.len() != 1 {
        return Err(Error(format!(
            "expected to find 1 number, got {} instead",
            result.len()
        )));
    }
    let first = result.first().unwrap_or(0);
    parse_int(&d.arena.get(first).value)
}

fn clamp(index: i64, length: i64) -> i64 {
    let mut index = index;
    if index < 0 {
        index += length;
    }
    index.clamp(0, length)
}

pub fn slice_array_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for lhs in context.nodes().to_vec() {
        let first = get_slice_number(d, &context, lhs, expr.lhs())?;
        let second = get_slice_number(d, &context, lhs, expr.rhs())?;
        let node = d.arena.get(lhs).clone();
        if node.kind == Kind::Scalar && d.arena.guess_tag_from_custom_type(lhs) == "!!str" {
            let runes: Vec<char> = node.value.chars().collect();
            let length = runes.len() as i64;
            let start = clamp(first, length);
            let mut end = clamp(second, length);
            if end < start {
                end = start;
            }
            let sliced: String = runes[start as usize..end as usize].iter().collect();
            let replacement = d
                .arena
                .create_replacement(lhs, Kind::Scalar, &node.tag, &sliced);
            d.arena.get_mut(replacement).style = node.style;
            results.push(replacement);
            continue;
        }
        let length = node.content.len() as i64;
        let start = clamp(first, length);
        let end = clamp(second, length);
        let mut new_results = Vec::new();
        let mut i = start;
        while i < end {
            new_results.push(node.content[i as usize]);
            i += 1;
        }
        let slice = d
            .arena
            .create_replacement(lhs, Kind::Sequence, &node.tag, "");
        d.arena.add_children(slice, &new_results);
        results.push(slice);
    }
    Ok(context.child_context(results))
}
