//! `operator_keys.go` and `operator_has.go`.

use crate::context::Context;
use crate::eval::{create_boolean_candidate, Evaluator};
use crate::node::{Kind, Node, NodeId};
use crate::operation::ExprNode;
use crate::Error;

pub fn is_key_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let is_key = d.arena.get(candidate).is_map_key;
        results.push(create_boolean_candidate(
            &mut d.arena,
            Some(candidate),
            is_key,
        ));
    }
    Ok(context.child_context(results))
}

pub fn get_key_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if let Some(key) = d.arena.get(candidate).key {
            results.push(key);
        }
    }
    Ok(context.child_context(results))
}

pub fn keys_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let target = match d.arena.get(candidate).kind {
            Kind::Mapping => get_map_keys(d, candidate),
            Kind::Sequence => get_indices(d, candidate),
            _ => {
                return Err(Error(format!(
                    "cannot get keys of {}, keys only works for maps and arrays",
                    d.arena.get(candidate).tag
                )))
            }
        };
        results.push(target);
    }
    Ok(context.child_context(results))
}

fn new_seq(d: &mut Evaluator) -> NodeId {
    d.arena.push(Node {
        kind: Kind::Sequence,
        tag: "!!seq".to_string(),
        ..Node::default()
    })
}

fn get_map_keys(d: &mut Evaluator, node: NodeId) -> NodeId {
    let keys: Vec<NodeId> = d
        .arena
        .get(node)
        .content
        .iter()
        .step_by(2)
        .copied()
        .collect();
    let seq = new_seq(d);
    d.arena.add_children(seq, &keys);
    seq
}

fn get_indices(d: &mut Evaluator, node: NodeId) -> NodeId {
    let count = d.arena.get(node).content.len();
    let mut indices = Vec::new();
    for index in 0..count {
        indices.push(d.arena.push(Node::scalar("!!int", &index.to_string())));
    }
    let seq = new_seq(d);
    d.arena.add_children(seq, &indices);
    seq
}

pub fn has_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let rhs = d.get_matching_nodes_opt(context.read_only_clone(), expr.rhs())?;
    let (wanted_key, wanted_tag) = match rhs.first() {
        Some(wanted) => (
            d.arena.get(wanted).value.clone(),
            d.arena.get(wanted).tag.clone(),
        ),
        None => ("null".to_string(), "!!null".to_string()),
    };
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let node = d.arena.get(candidate);
        let has = match node.kind {
            Kind::Mapping => node
                .content
                .iter()
                .step_by(2)
                .any(|key| d.arena.get(*key).value == wanted_key),
            Kind::Sequence if wanted_tag == "!!int" => {
                let number: i64 = wanted_key.parse().map_err(|_| {
                    Error(format!(
                        "strconv.ParseInt: parsing \"{wanted_key}\": invalid syntax"
                    ))
                })?;
                number >= 0 && (node.content.len() as i64) > number
            }
            _ => false,
        };
        results.push(create_boolean_candidate(&mut d.arena, Some(candidate), has));
    }
    Ok(context.child_context(results))
}
