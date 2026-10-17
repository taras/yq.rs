//! `operator_collect_object.go`.

use crate::context::Context;
use crate::eval::Evaluator;
use crate::node::{Kind, Node, NodeId};
use crate::operation::{ExprNode, MultiplyPrefs, TraversePrefs};
use crate::ops::multiply::multiply;
use crate::ops::traverse::splat;
use crate::Error;

pub fn collect_object_operator(
    d: &mut Evaluator,
    original: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let context = original.writable_clone();
    if context.is_empty() {
        let candidate = d.arena.push(Node {
            kind: Kind::Mapping,
            tag: "!!map".to_string(),
            value: "{}".to_string(),
            ..Node::default()
        });
        return Ok(context.single_child_context(candidate));
    }
    let first = context.first().unwrap_or(0);
    let first_len = d.arena.get(first).content.len();
    let mut rotated: Vec<Vec<NodeId>> = vec![Vec::new(); first_len];
    for candidate in context.nodes().to_vec() {
        let content = d.arena.get(candidate).content.clone();
        if content.len() < first_len {
            return Err(Error(
                "CollectObject: mismatching node sizes; are you creating a map with mismatching key value pairs?".to_string(),
            ));
        }
        for (i, slot) in rotated.iter_mut().enumerate() {
            slot.push(content[i]);
        }
    }
    let mut new_object = Vec::new();
    for remaining in rotated {
        let additions = collect(d, context.child_context(Vec::new()), remaining)?;
        for addition in additions.nodes().to_vec() {
            let copy = d.arena.copy(addition);
            let node = d.arena.get_mut(copy);
            node.parent = None;
            node.key = None;
            new_object.push(copy);
        }
    }
    Ok(context.child_context(new_object))
}

fn collect(
    d: &mut Evaluator,
    context: Context,
    mut remaining: Vec<NodeId>,
) -> Result<Context, Error> {
    if remaining.is_empty() {
        return Ok(context);
    }
    let candidate = remaining.remove(0);
    let splatted = splat(
        d,
        context.single_child_context(candidate),
        TraversePrefs {
            dont_follow_alias: true,
            include_map_keys: false,
            ..TraversePrefs::default()
        },
    )?;
    if context.is_empty() {
        return collect(d, splatted, remaining);
    }
    let mut new_agg = Vec::new();
    for agg_candidate in context.nodes().to_vec() {
        for splat_candidate in splatted.nodes().to_vec() {
            let new_candidate = d.arena.copy(agg_candidate);
            let merged = multiply(
                d,
                &context,
                new_candidate,
                splat_candidate,
                MultiplyPrefs::default(),
            )?;
            new_agg.push(merged);
        }
    }
    collect(d, context.child_context(new_agg), remaining)
}
