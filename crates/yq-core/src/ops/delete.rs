//! `operator_delete.go`.

use crate::context::Context;
use crate::eval::Evaluator;
use crate::node::{Kind, NodeId, PathElem, FLOW_STYLE};
use crate::operation::ExprNode;
use crate::Error;

pub fn delete_child_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let to_delete = d.get_matching_nodes_opt(context.read_only_clone(), expr.rhs())?;
    for candidate in to_delete.nodes().iter().rev().copied().collect::<Vec<_>>() {
        let parent = match d.arena.get(candidate).parent {
            Some(parent) => parent,
            None => {
                let remaining: Vec<NodeId> = context
                    .nodes()
                    .iter()
                    .copied()
                    .filter(|id| *id != candidate)
                    .collect();
                return Ok(context.child_context(remaining));
            }
        };
        let path = d.arena.get_path(candidate);
        let child_path = match path.last() {
            Some(last) => last.clone(),
            None => continue,
        };
        match d.arena.get(parent).kind {
            Kind::Mapping => delete_from_map(d, parent, &child_path),
            Kind::Sequence => delete_from_array(d, parent, &child_path),
            _ => {
                return Err(Error(format!(
                    "cannot delete nodes from parent of tag {}",
                    d.arena.get(parent).tag
                )))
            }
        }
    }
    Ok(context)
}

fn delete_from_map(d: &mut Evaluator, node: NodeId, child_path: &PathElem) {
    let contents = d.arena.get(node).content.clone();
    let wanted = child_path.to_string();
    let mut new_contents = Vec::new();
    let mut index = 0;
    while index + 1 < contents.len() {
        let key = contents[index];
        let value = contents[index + 1];
        if d.arena.get(key).value != wanted {
            new_contents.push(key);
            new_contents.push(value);
        }
        index += 2;
    }
    d.arena.get_mut(node).content = new_contents;
    normalise_empty_collection_map_key_comment(d, node);
}

pub fn normalise_empty_collection_map_key_comment(d: &mut Evaluator, node: NodeId) {
    let current = d.arena.get(node);
    if !matches!(current.kind, Kind::Sequence | Kind::Mapping)
        || !current.content.is_empty()
        || !current.line_comment.is_empty()
    {
        return;
    }
    let mut key = current.key;
    if let Some(parent) = current.parent {
        if d.arena.get(parent).kind == Kind::Mapping {
            let content = d.arena.get(parent).content.clone();
            let mut index = 0;
            while index + 1 < content.len() {
                if content[index + 1] == node {
                    key = Some(content[index]);
                    break;
                }
                index += 2;
            }
        }
    }
    let key = match key {
        Some(key) if !d.arena.get(key).line_comment.is_empty() => key,
        _ => return,
    };
    let comment = std::mem::take(&mut d.arena.get_mut(key).line_comment);
    let target = d.arena.get_mut(node);
    target.line_comment = comment;
    target.style = FLOW_STYLE;
}

fn delete_from_array(d: &mut Evaluator, node: NodeId, child_path: &PathElem) {
    let contents = d.arena.get(node).content.clone();
    let wanted = child_path.to_string();
    let mut new_contents: Vec<NodeId> = Vec::new();
    for (index, value) in contents.iter().enumerate() {
        if index.to_string() != wanted {
            let position = new_contents.len().to_string();
            if let Some(key) = d.arena.get(*value).key {
                d.arena.get_mut(key).value = position;
            }
            new_contents.push(*value);
        }
    }
    d.arena.get_mut(node).content = new_contents;
    normalise_empty_collection_map_key_comment(d, node);
}
