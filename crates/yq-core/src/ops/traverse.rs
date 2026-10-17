//! `operator_traverse_path.go`, `operator_recursive_descent.go` and
//! `matchKeyString.go`.

use std::collections::HashSet;

use crate::context::Context;
use crate::eval::Evaluator;
use crate::node::{Arena, Kind, Node, NodeId};
use crate::operation::{
    ExprNode, OpType, OpValue, Operation, Prefs, RecursiveDescentPrefs, TraversePrefs,
};
use crate::util::parse_int;
use crate::Error;

pub fn match_key(name: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return name == pattern;
    }
    if pattern == "*" {
        return true;
    }
    deep_match(name, pattern)
}

fn deep_match(name: &str, pattern: &str) -> bool {
    let name = name.as_bytes();
    let pattern = pattern.as_bytes();
    let (mut px, mut nx, mut next_px, mut next_nx) = (0usize, 0usize, 0usize, 0usize);
    while px < pattern.len() || nx < name.len() {
        if px < pattern.len() {
            match pattern[px] {
                b'?' => {
                    if nx < name.len() {
                        px += 1;
                        nx += 1;
                        continue;
                    }
                }
                b'*' => {
                    next_px = px;
                    next_nx = nx + 1;
                    px += 1;
                    continue;
                }
                c => {
                    if nx < name.len() && name[nx] == c {
                        px += 1;
                        nx += 1;
                        continue;
                    }
                }
            }
        }
        if 0 < next_nx && next_nx <= name.len() {
            px = next_px;
            nx = next_nx;
            continue;
        }
        return false;
    }
    true
}

pub fn splat(d: &mut Evaluator, context: Context, prefs: TraversePrefs) -> Result<Context, Error> {
    traverse_nodes_with_array_indices(d, context, &[], prefs)
}

pub fn traverse_path_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut matches = Vec::new();
    for candidate in context.nodes().to_vec() {
        let new_nodes = traverse(d, &context, candidate, &expr.op)?;
        matches.extend(new_nodes);
    }
    Ok(context.child_context(matches))
}

pub fn resolve_alias_chain(arena: &Arena, mut node: NodeId) -> Result<NodeId, Error> {
    let mut visited = HashSet::new();
    while arena.get(node).kind == Kind::Alias {
        if !visited.insert(node) {
            return Err(Error("alias cycle detected".to_string()));
        }
        node = match arena.get(node).alias {
            Some(target) => target,
            None => return Ok(node),
        };
    }
    Ok(node)
}

fn traverse(
    d: &mut Evaluator,
    context: &Context,
    matching: NodeId,
    operation: &Operation,
) -> Result<Vec<NodeId>, Error> {
    let matching = resolve_alias_chain(&d.arena, matching)?;
    let prefs = operation.prefs.traverse();
    let is_empty_brackets = matches!(&operation.value, OpValue::Str(s) if s == "[]");
    if d.arena.get(matching).tag == "!!null" && !is_empty_brackets && !context.dont_auto_create {
        let node = d.arena.get_mut(matching);
        node.kind = match operation.value {
            OpValue::Int(_) => Kind::Sequence,
            _ => Kind::Mapping,
        };
        node.tag = String::new();
    }
    match d.arena.get(matching).kind {
        Kind::Mapping => {
            let key = d.arena.push(Node::string_scalar(&operation.string_value));
            traverse_map(d, context, matching, key, prefs, false)
        }
        Kind::Sequence => {
            let index = d.arena.push(Node {
                value: operation.string_value.clone(),
                ..Node::default()
            });
            traverse_array_with_indices(d, matching, &[index], prefs)
        }
        _ => Ok(Vec::new()),
    }
}

pub fn traverse_array_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let lhs_exp = d.lhs(expr)?;
    let rhs_exp = d.rhs(expr)?;
    if let Some(slice) = rhs_exp.rhs() {
        if slice.op.op_type == OpType::CreateMap {
            let lhs_context = d.get_matching_nodes(context.clone_context(), lhs_exp)?;
            return crate::ops::slice::slice_array_operator(d, lhs_context, slice);
        }
    }
    let lhs = d.get_matching_nodes(context.clone_context(), lhs_exp)?;
    let rhs = d.get_matching_nodes(context.read_only_clone(), rhs_exp)?;
    let prefs = match &expr.op.prefs {
        Prefs::Traverse(prefs) => *prefs,
        _ => TraversePrefs::default(),
    };
    let indices = match rhs.first() {
        Some(first) => d.arena.get(first).content.clone(),
        None => return Err(Error("traverse array: no indices".to_string())),
    };
    let result = traverse_nodes_with_array_indices(d, lhs, &indices, prefs)?;
    Ok(context.child_context_shared(result.matching_nodes.clone()))
}

pub fn traverse_nodes_with_array_indices(
    d: &mut Evaluator,
    context: Context,
    indices: &[NodeId],
    prefs: TraversePrefs,
) -> Result<Context, Error> {
    let mut matches = Vec::new();
    for candidate in context.nodes().to_vec() {
        let new_nodes = traverse_array_indices(d, &context, candidate, indices, prefs)?;
        matches.extend(new_nodes);
    }
    Ok(context.child_context(matches))
}

fn traverse_array_indices(
    d: &mut Evaluator,
    context: &Context,
    matching: NodeId,
    indices: &[NodeId],
    prefs: TraversePrefs,
) -> Result<Vec<NodeId>, Error> {
    let matching = resolve_alias_chain(&d.arena, matching)?;
    if d.arena.get(matching).tag == "!!null" {
        let first_is_int = indices
            .first()
            .is_none_or(|index| d.arena.get(*index).tag == "!!int");
        let node = d.arena.get_mut(matching);
        node.tag = String::new();
        node.kind = if first_is_int {
            Kind::Sequence
        } else {
            Kind::Mapping
        };
    }
    match d.arena.get(matching).kind {
        Kind::Sequence => traverse_array_with_indices(d, matching, indices, prefs),
        Kind::Mapping => traverse_map_with_indices(d, context, matching, indices, prefs),
        _ => Ok(Vec::new()),
    }
}

fn traverse_map_with_indices(
    d: &mut Evaluator,
    context: &Context,
    candidate: NodeId,
    indices: &[NodeId],
    prefs: TraversePrefs,
) -> Result<Vec<NodeId>, Error> {
    if indices.is_empty() {
        let key = d.arena.push(Node::string_scalar(""));
        return traverse_map(d, context, candidate, key, prefs, true);
    }
    let mut matches = Vec::new();
    for index in indices {
        let new_nodes = traverse_map(d, context, candidate, *index, prefs, false)?;
        matches.extend(new_nodes);
    }
    Ok(matches)
}

fn traverse_array_with_indices(
    d: &mut Evaluator,
    node: NodeId,
    indices: &[NodeId],
    prefs: TraversePrefs,
) -> Result<Vec<NodeId>, Error> {
    if indices.is_empty() {
        return Ok(d.arena.get(node).content.clone());
    }
    let mut matches = Vec::new();
    for index_node in indices {
        let index_text = d.arena.get(*index_node).value.clone();
        let index = match parse_int(&index_text) {
            Ok(index) => index,
            Err(error) => {
                if prefs.optional_traverse {
                    continue;
                }
                return Err(Error(format!(
                    "cannot index array with '{index_text}' ({error})"
                )));
            }
        };
        let mut content_length = d.arena.get(node).content.len() as i64;
        while content_length <= index {
            if content_length == 0 {
                d.arena.get_mut(node).style = 0;
            }
            let value = d.arena.push(Node::scalar("!!null", "null"));
            d.arena.add_child(node, value);
            content_length = d.arena.get(node).content.len() as i64;
        }
        let mut index_to_use = index;
        if index_to_use < 0 {
            index_to_use += content_length;
        }
        if index_to_use < 0 {
            return Err(Error(format!(
                "index [{index}] out of range, array size is {content_length}"
            )));
        }
        matches.push(d.arena.get(node).content[index_to_use as usize]);
    }
    Ok(matches)
}

fn key_matches(arena: &Arena, key: NodeId, wanted: &str, exact: bool) -> bool {
    let value = &arena.get(key).value;
    if exact {
        return value == wanted;
    }
    match_key(value, wanted)
}

/// An insertion-ordered set keyed by `GetKey`, as the Go `orderedmap`.
struct OrderedMatches {
    keys: Vec<String>,
    values: Vec<NodeId>,
}

impl OrderedMatches {
    fn new() -> OrderedMatches {
        OrderedMatches {
            keys: Vec::new(),
            values: Vec::new(),
        }
    }

    fn set(&mut self, key: String, value: NodeId) {
        if let Some(position) = self.keys.iter().position(|k| *k == key) {
            self.values[position] = value;
        } else {
            self.keys.push(key);
            self.values.push(value);
        }
    }
}

fn traverse_map(
    d: &mut Evaluator,
    context: &Context,
    matching: NodeId,
    key_node: NodeId,
    prefs: TraversePrefs,
    splat: bool,
) -> Result<Vec<NodeId>, Error> {
    let mut new_matches = OrderedMatches::new();
    let wanted = d.arena.get(key_node).value.clone();
    do_traverse_map(d, &mut new_matches, matching, &wanted, prefs, splat)?;
    if !splat
        && !prefs.dont_auto_create
        && !context.dont_auto_create
        && new_matches.values.is_empty()
    {
        let value = d.arena.push(Node {
            kind: Kind::Scalar,
            tag: "!!null".to_string(),
            value: "null".to_string(),
            parent: Some(matching),
            ..Node::default()
        });
        if d.arena.get(matching).content.is_empty() {
            d.arena.get_mut(matching).style = 0;
        }
        let (key, value) = d.arena.add_key_value_child(matching, key_node, value);
        if prefs.include_map_keys {
            new_matches.set(d.arena.get_key(key), key);
        }
        if !prefs.dont_include_map_values {
            new_matches.set(d.arena.get_key(value), value);
        }
    }
    Ok(new_matches.values)
}

fn do_traverse_map(
    d: &mut Evaluator,
    new_matches: &mut OrderedMatches,
    node: NodeId,
    wanted: &str,
    prefs: TraversePrefs,
    splat: bool,
) -> Result<(), Error> {
    let contents = d.arena.get(node).content.clone();
    let fix_to_spec = d.fix_merge_anchor_to_spec;
    if !prefs.dont_follow_alias && fix_to_spec {
        // Merge keys go first so explicit keys override them, later ones
        // first so earlier merge keys win.
        let mut index = contents.len();
        while index >= 2 {
            index -= 2;
            let key = contents[index];
            if d.arena.get(key).tag == "!!merge" {
                traverse_merge_anchor(d, new_matches, contents[index + 1], wanted, prefs, splat)?;
            }
        }
    }
    let mut index = 0;
    while index + 1 < contents.len() {
        let key = contents[index];
        let value = contents[index + 1];
        let key_tag_is_merge = d.arena.get(key).tag == "!!merge";
        if key_tag_is_merge && !prefs.dont_follow_alias && wanted != d.arena.get(key).value {
            if !fix_to_spec {
                traverse_merge_anchor(d, new_matches, value, wanted, prefs, splat)?;
            }
        } else if splat || key_matches(&d.arena, key, wanted, prefs.exact_key_match) {
            if prefs.include_map_keys {
                new_matches.set(d.arena.get_key(key), key);
            }
            if !prefs.dont_include_map_values {
                new_matches.set(d.arena.get_key(value), value);
            }
        }
        index += 2;
    }
    Ok(())
}

fn traverse_merge_anchor(
    d: &mut Evaluator,
    new_matches: &mut OrderedMatches,
    merge: NodeId,
    wanted: &str,
    prefs: TraversePrefs,
    splat: bool,
) -> Result<(), Error> {
    let mut merge = merge;
    if d.arena.get(merge).kind == Kind::Alias {
        merge = match d.arena.get(merge).alias {
            Some(target) => target,
            None => return Ok(()),
        };
    }
    match d.arena.get(merge).kind {
        Kind::Mapping => do_traverse_map(d, new_matches, merge, wanted, prefs, splat),
        Kind::Sequence => {
            let mut content = d.arena.get(merge).content.clone();
            if d.fix_merge_anchor_to_spec {
                content.reverse();
            }
            for child in content {
                let mut child = child;
                if d.arena.get(child).kind == Kind::Alias {
                    child = match d.arena.get(child).alias {
                        Some(target) => target,
                        None => return Ok(()),
                    };
                }
                if d.arena.get(child).kind != Kind::Mapping {
                    return Ok(());
                }
                do_traverse_map(d, new_matches, child, wanted, prefs, splat)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// `traverseMap` with an empty context, as the entries operators use it.
pub fn traverse_map_by_key(
    d: &mut Evaluator,
    node: NodeId,
    key: &str,
    prefs: TraversePrefs,
) -> Result<Vec<NodeId>, Error> {
    let key_node = d.arena.push(Node::string_scalar(key));
    traverse_map(d, &Context::default(), node, key_node, prefs, false)
}

pub fn recursive_descent_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let prefs = match &expr.op.prefs {
        Prefs::RecursiveDescent(prefs) => *prefs,
        _ => RecursiveDescentPrefs::default(),
    };
    let mut results = Vec::new();
    recursive_descent(d, &mut results, context.clone_context(), prefs)?;
    Ok(context.child_context(results))
}

pub fn recursive_descent(
    d: &mut Evaluator,
    results: &mut Vec<NodeId>,
    context: Context,
    prefs: RecursiveDescentPrefs,
) -> Result<(), Error> {
    for candidate in context.nodes().to_vec() {
        results.push(candidate);
        let node = d.arena.get(candidate);
        let recurse = node.kind != Kind::Alias
            && !node.content.is_empty()
            && (prefs.recurse_array || node.kind != Kind::Sequence);
        if recurse {
            let children = splat(
                d,
                context.single_child_context(candidate),
                prefs.traverse_prefs,
            )?;
            recursive_descent(d, results, children, prefs)?;
        }
    }
    Ok(())
}
