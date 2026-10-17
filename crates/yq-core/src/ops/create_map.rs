//! `operator_create_map.go`.

use crate::context::Context;
use crate::eval::{cross_function, Evaluator};
use crate::node::{Kind, Node, NodeId, FLOW_STYLE};
use crate::operation::ExprNode;
use crate::Error;

pub fn create_map_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut sequences = Vec::new();
    if !context.is_empty() {
        for matching in context.nodes().to_vec() {
            sequences.push(sequence_for(d, &context, Some(matching), expr)?);
        }
    } else {
        sequences.push(sequence_for(d, &context, None, expr)?);
    }
    let node = list_to_node_seq(d, &sequences);
    Ok(context.single_child_context(node))
}

fn sequence_for(
    d: &mut Evaluator,
    context: &Context,
    matching: Option<NodeId>,
    expr: &ExprNode,
) -> Result<NodeId, Error> {
    let (document, filename, file_index) = match matching {
        Some(id) => (
            d.arena.get_document(id),
            d.arena.get_filename(id).to_string(),
            d.arena.get_file_index(id),
        ),
        None => (0, String::new(), 0),
    };
    let matches: Vec<NodeId> = matching.into_iter().collect();
    let filename_for_calc = filename.clone();
    let calculation = move |d: &mut Evaluator,
                            _ctx: &Context,
                            lhs: Option<NodeId>,
                            rhs: Option<NodeId>|
          -> Result<Option<NodeId>, Error> {
        let (lhs, rhs) = match (lhs, rhs) {
            (Some(lhs), Some(rhs)) => (lhs, rhs),
            _ => return Ok(None),
        };
        let node = d.arena.push(Node {
            kind: Kind::Mapping,
            tag: "!!map".to_string(),
            document,
            file_index,
            filename: filename_for_calc.clone(),
            ..Node::default()
        });
        d.arena.add_key_value_child(node, lhs, rhs);
        Ok(Some(node))
    };
    let map_pairs = cross_function(d, context.child_context(matches), expr, &calculation, false)?;
    let inner = list_to_node_seq(d, map_pairs.nodes());
    let node = d.arena.get_mut(inner);
    node.style = FLOW_STYLE;
    node.document = document;
    node.file_index = file_index;
    node.filename = filename;
    Ok(inner)
}

pub fn list_to_node_seq(d: &mut Evaluator, entries: &[NodeId]) -> NodeId {
    let node = d.arena.push(Node {
        kind: Kind::Sequence,
        tag: "!!seq".to_string(),
        ..Node::default()
    });
    for entry in entries {
        d.arena.add_child(node, *entry);
    }
    node
}
