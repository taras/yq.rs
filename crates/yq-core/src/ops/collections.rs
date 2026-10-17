//! Array and map operators: sort, sort_by, sort_keys, reverse, unique,
//! unique_by, group_by, flatten, first, filter, pick, omit, pivot, contains,
//! to_number and the entries family.

use std::cmp::Ordering;

use crate::context::Context;
use crate::eval::{collect_together, create_boolean_candidate, cross_function, Evaluator};
use crate::node::{find_in_array, find_key_in_map, Kind, Node, NodeId};
use crate::operation::{ExprNode, OpType, Operation, Prefs, TraversePrefs};
use crate::ops::control::select_operator;
use crate::ops::traverse::{splat, traverse_map_by_key};
use crate::util::{is_truthy_node, parse_float, parse_int, parse_int64};
use crate::yaml::resolve::go_parse_float;
use crate::Error;

pub fn sort_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let tree = ExprNode {
        op: expr.op.clone(),
        lhs: None,
        rhs: Some(Box::new(ExprNode::leaf(Operation::new(
            OpType::SelfReference,
        )))),
    };
    sort_by_operator(d, context, &tree)
}

struct Sortable {
    node: NodeId,
    compare: Vec<NodeId>,
}

fn compare_for_sort(d: &mut Evaluator, lhs: NodeId, rhs: NodeId, layout: &str) -> Ordering {
    let mut lhs_tag = d.arena.get(lhs).tag.clone();
    let mut rhs_tag = d.arena.get(rhs).tag.clone();
    if !lhs_tag.starts_with("!!") {
        lhs_tag = d.arena.guess_tag_from_custom_type(lhs);
    }
    if !rhs_tag.starts_with("!!") {
        rhs_tag = d.arena.guess_tag_from_custom_type(rhs);
    }
    let lhs_value = d.arena.get(lhs).value.clone();
    let rhs_value = d.arena.get(rhs).value.clone();
    let mut is_datetime = lhs_tag == "!!timestamp" && rhs_tag == "!!timestamp";
    if lhs_tag == "!!str" && layout != crate::context::RFC3339 {
        is_datetime = crate::ops::datetime::parse_datetime(layout, &lhs_value).is_ok()
            && crate::ops::datetime::parse_datetime(layout, &rhs_value).is_ok();
    }
    if lhs_tag == "!!null" && rhs_tag != "!!null" {
        return Ordering::Less;
    }
    if lhs_tag != "!!null" && rhs_tag == "!!null" {
        return Ordering::Greater;
    }
    if lhs_tag == "!!bool" && rhs_tag != "!!bool" {
        return Ordering::Less;
    }
    if lhs_tag != "!!bool" && rhs_tag == "!!bool" {
        return Ordering::Greater;
    }
    if lhs_tag == "!!bool" && rhs_tag == "!!bool" {
        let lhs_truthy = is_truthy_node(d.arena.get(lhs));
        let rhs_truthy = is_truthy_node(d.arena.get(rhs));
        return lhs_truthy.cmp(&rhs_truthy);
    }
    if is_datetime {
        let lhs_time = crate::ops::datetime::parse_datetime(layout, &lhs_value);
        let rhs_time = crate::ops::datetime::parse_datetime(layout, &rhs_value);
        return match (lhs_time, rhs_time) {
            (Ok(l), Ok(r)) => l.cmp(&r),
            _ => lhs_value.cmp(&rhs_value),
        };
    }
    if lhs_tag == "!!int" && rhs_tag == "!!int" {
        if let (Ok((_, l)), Ok((_, r))) = (parse_int64(&lhs_value), parse_int64(&rhs_value)) {
            return l.cmp(&r);
        }
    }
    let numeric = |tag: &str| tag == "!!int" || tag == "!!float";
    if numeric(&lhs_tag) && numeric(&rhs_tag) {
        if let (Ok(l), Ok(r)) = (parse_float(&lhs_value), parse_float(&rhs_value)) {
            return l.partial_cmp(&r).unwrap_or(Ordering::Equal);
        }
    }
    lhs_value.cmp(&rhs_value)
}

pub fn sort_by_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    let layout = context.datetime_layout().to_string();
    for candidate in context.nodes().to_vec() {
        if !d.arena.can_visit_values(candidate) {
            return Err(Error(format!(
                "node at path [{}] is not an array or map (it's a {})",
                d.arena.get_nice_path(candidate),
                d.arena.get(candidate).tag
            )));
        }
        let mut sortable = Vec::new();
        for value in d.arena.value_children(candidate) {
            let compare =
                d.get_matching_nodes_opt(context.single_readonly_child_context(value), expr.rhs())?;
            sortable.push(Sortable {
                node: value,
                compare: compare.nodes().to_vec(),
            });
        }
        // Insertion sort keeps the comparison stable and lets it borrow `d`.
        let mut sorted: Vec<Sortable> = Vec::new();
        for item in sortable {
            let mut position = sorted.len();
            while position > 0 && less(d, &item, &sorted[position - 1], &layout) {
                position -= 1;
            }
            sorted.insert(position, item);
        }
        let sorted_list = d.arena.copy_without_content(candidate);
        match d.arena.get(candidate).kind {
            Kind::Mapping => {
                for item in &sorted {
                    if let Some(key) = d.arena.get(item.node).key {
                        d.arena.add_key_value_child(sorted_list, key, item.node);
                    }
                }
            }
            Kind::Sequence => {
                for item in &sorted {
                    d.arena.add_child(sorted_list, item.node);
                }
            }
            _ => {}
        }
        results.push(sorted_list);
    }
    Ok(context.child_context(results))
}

fn less(d: &mut Evaluator, a: &Sortable, b: &Sortable, layout: &str) -> bool {
    let mut index = 0;
    while index < a.compare.len() && index < b.compare.len() {
        match compare_for_sort(d, a.compare[index], b.compare[index], layout) {
            Ordering::Less => return true,
            Ordering::Greater => return false,
            Ordering::Equal => {}
        }
        index += 1;
    }
    a.compare.len() < b.compare.len()
}

pub fn sort_keys_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    for candidate in context.nodes().to_vec() {
        let rhs =
            d.get_matching_nodes_opt(context.single_readonly_child_context(candidate), expr.rhs())?;
        for node in rhs.nodes().to_vec() {
            if d.arena.get(node).kind == Kind::Mapping {
                sort_keys(d, node);
            }
        }
    }
    Ok(context)
}

fn sort_keys(d: &mut Evaluator, node: NodeId) {
    let content = d.arena.get(node).content.clone();
    let mut pairs: Vec<(String, NodeId, NodeId)> = Vec::new();
    let mut index = 0;
    while index + 1 < content.len() {
        let key = content[index];
        pairs.push((d.arena.get(key).value.clone(), key, content[index + 1]));
        index += 2;
    }
    // Go keeps the last pair for duplicate keys because the buckets are maps.
    let mut unique: Vec<(String, NodeId, NodeId)> = Vec::new();
    for pair in pairs {
        if let Some(existing) = unique.iter_mut().find(|p| p.0 == pair.0) {
            existing.1 = pair.1;
            existing.2 = pair.2;
        } else {
            unique.push(pair);
        }
    }
    unique.sort_by(|a, b| a.0.cmp(&b.0));
    let mut sorted = Vec::new();
    for (_, key, value) in unique {
        sorted.push(key);
        sorted.push(value);
    }
    d.arena.get_mut(node).content = sorted;
}

pub fn reverse_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.get(candidate).kind != Kind::Sequence {
            return Err(Error(format!(
                "node at path [{}] is not an array (it's a {})",
                d.arena.get_nice_path(candidate),
                d.arena.get(candidate).tag
            )));
        }
        let style = d.arena.get(candidate).style;
        let reversed =
            d.arena
                .create_replacement_with_comments(candidate, Kind::Sequence, "!!seq", style);
        let mut content = d.arena.get(candidate).content.clone();
        content.reverse();
        d.arena.add_children(reversed, &content);
        results.push(reversed);
    }
    Ok(context.child_context(results))
}

pub fn unique_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let tree = ExprNode {
        op: Operation::new(OpType::UniqueBy),
        lhs: None,
        rhs: Some(Box::new(ExprNode::leaf(Operation::new(
            OpType::SelfReference,
        )))),
    };
    unique_by_operator(d, context, &tree)
}

fn unique_key_value(d: &mut Evaluator, rhs: &Context) -> Result<String, Error> {
    match rhs.first() {
        Some(first) => {
            if d.arena.get(first).kind != Kind::Scalar {
                return crate::ops::encoder::encode_to_string(
                    d,
                    first,
                    crate::operation::EncodeFormat::Yaml,
                    0,
                );
            }
            Ok(d.arena.get(first).value.clone())
        }
        None => Ok("null".to_string()),
    }
}

pub fn unique_by_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.get(candidate).kind != Kind::Sequence {
            return Err(Error("only arrays are supported for unique".to_string()));
        }
        let mut seen: Vec<String> = Vec::new();
        let mut kept: Vec<NodeId> = Vec::new();
        for child in d.arena.get(candidate).content.clone() {
            let rhs =
                d.get_matching_nodes_opt(context.single_readonly_child_context(child), expr.rhs())?;
            let key = unique_key_value(d, &rhs)?;
            if !seen.contains(&key) {
                seen.push(key);
                kept.push(child);
            }
        }
        let style = d.arena.get(candidate).style;
        let result =
            d.arena
                .create_replacement_with_comments(candidate, Kind::Sequence, "!!seq", style);
        for child in kept {
            d.arena.add_child(result, child);
        }
        results.push(result);
    }
    Ok(context.child_context(results))
}

pub fn group_by_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.get(candidate).kind != Kind::Sequence {
            return Err(Error("only arrays are supported for group by".to_string()));
        }
        let mut groups: Vec<(String, Vec<NodeId>)> = Vec::new();
        for child in d.arena.get(candidate).content.clone() {
            let rhs =
                d.get_matching_nodes_opt(context.single_readonly_child_context(child), expr.rhs())?;
            let key = match rhs.first() {
                Some(first) => d.arena.get(first).value.clone(),
                None => "null".to_string(),
            };
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, members)) => members.push(child),
                None => groups.push((key, vec![child])),
            }
        }
        let result = d
            .arena
            .create_replacement(candidate, Kind::Sequence, "!!seq", "");
        for (_, members) in groups {
            let group = d.arena.push(Node {
                kind: Kind::Sequence,
                tag: "!!seq".to_string(),
                ..Node::default()
            });
            for member in members {
                d.arena.add_child(group, member);
            }
            d.arena.add_child(result, group);
        }
        results.push(result);
    }
    Ok(context.child_context(results))
}

fn flatten(d: &mut Evaluator, node: NodeId, depth: i64) {
    if depth == 0 || d.arena.get(node).kind != Kind::Sequence {
        return;
    }
    let content = d.arena.get(node).content.clone();
    let mut new_seq = Vec::new();
    for child in content {
        if d.arena.get(child).kind == Kind::Sequence {
            flatten(d, child, depth - 1);
            new_seq.extend(d.arena.get(child).content.iter().copied());
        } else {
            new_seq.push(child);
        }
    }
    d.arena.get_mut(node).content = Vec::new();
    d.arena.add_children(node, &new_seq);
}

pub fn flatten_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let depth = match expr.op.prefs {
        Prefs::Flatten { depth } => depth,
        _ => -1,
    };
    for candidate in context.nodes().to_vec() {
        if d.arena.get(candidate).kind != Kind::Sequence {
            return Err(Error("only arrays are supported for flatten".to_string()));
        }
        flatten(d, candidate, depth);
    }
    Ok(context)
}

pub fn first_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let rhs_exp = match expr.rhs() {
            Some(rhs) => rhs,
            None => {
                if let Some(first) = d.arena.get(candidate).content.first().copied() {
                    results.push(first);
                }
                continue;
            }
        };
        let splatted = splat(
            d,
            context.single_child_context(candidate),
            TraversePrefs::default(),
        )?;
        for splat_candidate in splatted.nodes().to_vec() {
            let rhs =
                d.get_matching_nodes(context.single_child_context(splat_candidate), rhs_exp)?;
            let include = rhs
                .nodes()
                .iter()
                .any(|result| is_truthy_node(d.arena.get(*result)));
            if include {
                results.push(splat_candidate);
                break;
            }
        }
    }
    Ok(context.child_context(results))
}

pub fn filter_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let splatted = splat(
            d,
            context.single_child_context(candidate),
            TraversePrefs::default(),
        )?;
        let filtered = select_operator(d, splatted, expr)?;
        let self_exp = ExprNode::leaf(Operation::new(OpType::SelfReference));
        let collected = collect_together(d, &filtered, &self_exp)?;
        let style = d.arena.get(candidate).style;
        d.arena.get_mut(collected).style = style;
        results.push(collected);
    }
    Ok(context.child_context(results))
}

fn pick_map(d: &mut Evaluator, original: NodeId, indices: NodeId) -> NodeId {
    let mut filtered = Vec::new();
    for key_to_find in d.arena.get(indices).content.clone() {
        if let Some(index) = find_key_in_map(&mut d.arena, original, key_to_find) {
            let key = d.arena.get(original).content[index];
            let value = d.arena.get(original).content[index + 1];
            let cloned_key = d.arena.copy(key);
            let cloned_value = d.arena.copy(value);
            filtered.push(cloned_key);
            filtered.push(cloned_value);
        }
    }
    let new_node = d.arena.copy_without_content(original);
    d.arena.add_children(new_node, &filtered);
    new_node
}

fn pick_sequence(d: &mut Evaluator, original: NodeId, indices: NodeId) -> Result<NodeId, Error> {
    let mut filtered = Vec::new();
    for index_node in d.arena.get(indices).content.clone() {
        let text = d.arena.get(index_node).value.clone();
        let index =
            parse_int(&text).map_err(|_| Error(format!("cannot index array with {text}")))?;
        let length = d.arena.get(original).content.len() as i64;
        if index > -1 && index < length {
            let child = d.arena.get(original).content[index as usize];
            filtered.push(d.arena.copy(child));
        }
    }
    let new_node = d.arena.copy_without_content(original);
    d.arena.add_children(new_node, &filtered);
    Ok(new_node)
}

pub fn pick_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let indices_context = d.get_matching_nodes_opt(context.clone_context(), expr.rhs())?;
    let indices = match indices_context.first() {
        Some(first) => first,
        None => d.arena.push(Node::default()),
    };
    let mut results = Vec::new();
    for node in context.nodes().to_vec() {
        let replacement = match d.arena.get(node).kind {
            Kind::Mapping => pick_map(d, node, indices),
            Kind::Sequence => pick_sequence(d, node, indices)?,
            _ => {
                return Err(Error(format!(
                    "cannot pick indices from type {} ({})",
                    d.arena.get(node).tag,
                    d.arena.get_nice_path(node)
                )))
            }
        };
        let leading = d.arena.get(node).leading_content.clone();
        d.arena.get_mut(replacement).leading_content = leading;
        results.push(replacement);
    }
    Ok(context.child_context(results))
}

pub fn omit_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let indices_context = d.get_matching_nodes_opt(context.clone_context(), expr.rhs())?;
    let indices = match indices_context.first() {
        Some(first) => first,
        None => d.arena.push(Node::default()),
    };
    if d.arena.get(indices).content.is_empty() {
        return Ok(context);
    }
    let mut results = Vec::new();
    for node in context.nodes().to_vec() {
        let replacement = match d.arena.get(node).kind {
            Kind::Mapping => {
                let content = d.arena.get(node).content.clone();
                let mut filtered = Vec::new();
                let mut index = 0;
                while index + 1 < content.len() {
                    if find_in_array(&mut d.arena, indices, content[index]).is_none() {
                        let cloned_key = d.arena.copy(content[index]);
                        let cloned_value = d.arena.copy(content[index + 1]);
                        filtered.push(cloned_key);
                        filtered.push(cloned_value);
                    }
                    index += 2;
                }
                let result = d.arena.copy_without_content(node);
                d.arena.add_children(result, &filtered);
                result
            }
            Kind::Sequence => {
                let content = d.arena.get(node).content.clone();
                let mut filtered = Vec::new();
                for (index, child) in content.iter().enumerate() {
                    let probe = d.arena.push(Node::scalar("!!int", &index.to_string()));
                    if find_in_array(&mut d.arena, indices, probe).is_none() {
                        filtered.push(d.arena.copy(*child));
                    }
                }
                let result = d.arena.copy_without_content(node);
                d.arena.add_children(result, &filtered);
                result
            }
            _ => return Ok(context),
        };
        let leading = d.arena.get(node).leading_content.clone();
        d.arena.get_mut(replacement).leading_content = leading;
        results.push(replacement);
    }
    Ok(context.child_context(results))
}

fn pad_with_nulls(d: &mut Evaluator, items: &mut Vec<NodeId>, length: usize) {
    while items.len() < length {
        items.push(d.arena.push(Node::scalar("!!null", "")));
    }
}

fn pivot_sequences(d: &mut Evaluator, seq: NodeId) -> NodeId {
    let rows = d.arena.get(seq).content.clone();
    let size = rows.len();
    if size == 0 {
        return seq;
    }
    let mut columns: Vec<Vec<NodeId>> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let cells = d.arena.get(*row).content.clone();
        for (j, cell) in cells.into_iter().enumerate() {
            while columns.len() <= j {
                columns.push(Vec::new());
            }
            pad_with_nulls(d, &mut columns[j], i);
            columns[j].push(cell);
        }
    }
    let result = d.arena.push(Node {
        kind: Kind::Sequence,
        ..Node::default()
    });
    for mut column in columns {
        pad_with_nulls(d, &mut column, size);
        let entry = d.arena.push(Node {
            kind: Kind::Sequence,
            ..Node::default()
        });
        d.arena.add_children(entry, &column);
        d.arena.add_child(result, entry);
    }
    result
}

fn pivot_maps(d: &mut Evaluator, seq: NodeId) -> NodeId {
    let rows = d.arena.get(seq).content.clone();
    let size = rows.len();
    if size == 0 {
        return d.arena.push(Node {
            kind: Kind::Mapping,
            ..Node::default()
        });
    }
    let mut columns: Vec<(String, Vec<NodeId>)> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let content = d.arena.get(*row).content.clone();
        let mut j = 0;
        while j + 1 < content.len() {
            let key = d.arena.get(content[j]).value.clone();
            let value = content[j + 1];
            let position = match columns.iter().position(|(k, _)| *k == key) {
                Some(position) => position,
                None => {
                    columns.push((key, Vec::new()));
                    columns.len() - 1
                }
            };
            pad_with_nulls(d, &mut columns[position].1, i);
            columns[position].1.push(value);
            j += 2;
        }
    }
    let result = d.arena.push(Node {
        kind: Kind::Mapping,
        ..Node::default()
    });
    for (key, mut values) in columns {
        pad_with_nulls(d, &mut values, size);
        let row = d.arena.push(Node {
            kind: Kind::Sequence,
            ..Node::default()
        });
        d.arena.add_children(row, &values);
        let key_node = d.arena.push(Node::scalar("!!str", &key));
        d.arena.add_key_value_child(result, key_node, row);
    }
    result
}

pub fn pivot_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.get(candidate).tag != "!!seq" {
            return Err(Error(format!(
                "cannot pivot node of type {}",
                d.arena.get(candidate).tag
            )));
        }
        let content = d.arena.get(candidate).content.clone();
        let mut tag = String::new();
        if let Some(first) = content.first() {
            tag = d.arena.get(*first).tag.clone();
            for child in content.iter().skip(1) {
                let other = d.arena.get(*child).tag.clone();
                if other != tag {
                    return Err(Error(format!(
                        "sequence contains elements of {tag} and {other} types"
                    )));
                }
            }
        }
        let pivot = match tag.as_str() {
            "!!seq" => pivot_sequences(d, candidate),
            "!!map" => pivot_maps(d, candidate),
            other => {
                return Err(Error(format!(
                    "can only pivot elements of !!seq or !!map types, received {other}"
                )))
            }
        };
        results.push(pivot);
    }
    Ok(context.child_context(results))
}

fn contains(d: &mut Evaluator, lhs: NodeId, rhs: NodeId) -> Result<bool, Error> {
    let lhs_node = d.arena.get(lhs).clone();
    let rhs_node = d.arena.get(rhs).clone();
    match lhs_node.kind {
        Kind::Mapping => {
            if rhs_node.kind != Kind::Mapping {
                return Ok(false);
            }
            let mut index = 0;
            while index + 1 < rhs_node.content.len() {
                let rhs_key = rhs_node.content[index];
                let rhs_value = rhs_node.content[index + 1];
                let lhs_index = match find_key_in_map(&mut d.arena, lhs, rhs_key) {
                    Some(found) => found,
                    None => return Ok(false),
                };
                let lhs_value = d.arena.get(lhs).content[lhs_index + 1];
                if !contains(d, lhs_value, rhs_value)? {
                    return Ok(false);
                }
                index += 2;
            }
            Ok(true)
        }
        Kind::Sequence => {
            if rhs_node.kind != Kind::Sequence {
                return contains_array_element(d, lhs, rhs);
            }
            for item in rhs_node.content {
                if !contains_array_element(d, lhs, item)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Kind::Scalar => {
            if rhs_node.kind != Kind::Scalar || lhs_node.tag != rhs_node.tag {
                return Ok(false);
            }
            if lhs_node.tag == "!!null" {
                return Ok(rhs_node.tag == "!!null");
            }
            if lhs_node.tag == "!!str" {
                return Ok(lhs_node.value.contains(&rhs_node.value));
            }
            Ok(lhs_node.value == rhs_node.value)
        }
        Kind::Alias => Err(Error(format!(
            "{} not yet supported for contains",
            lhs_node.tag
        ))),
    }
}

fn contains_array_element(d: &mut Evaluator, array: NodeId, item: NodeId) -> Result<bool, Error> {
    for child in d.arena.get(array).content.clone() {
        if contains(d, child, item)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn contains_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let calculation = |d: &mut Evaluator,
                       _ctx: &Context,
                       lhs: Option<NodeId>,
                       rhs: Option<NodeId>|
     -> Result<Option<NodeId>, Error> {
        let (lhs, rhs) = match (lhs, rhs) {
            (Some(lhs), Some(rhs)) => (lhs, rhs),
            _ => return Ok(None),
        };
        if d.arena.get(lhs).kind != d.arena.get(rhs).kind {
            return Err(Error(format!(
                "{} cannot check contained in {}",
                d.arena.get(rhs).tag,
                d.arena.get(lhs).tag
            )));
        }
        let result = contains(d, lhs, rhs)?;
        Ok(Some(create_boolean_candidate(
            &mut d.arena,
            Some(lhs),
            result,
        )))
    };
    cross_function(d, context.read_only_clone(), expr, &calculation, false)
}

pub fn to_number_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let node = d.arena.get(candidate).clone();
        if node.kind != Kind::Scalar {
            return Err(Error(format!(
                "cannot convert node at path {} of tag {} to number",
                d.arena.get_nice_path(candidate),
                node.tag
            )));
        }
        if node.tag == "!!int" || node.tag == "!!float" {
            results.push(candidate);
            continue;
        }
        let tag = if parse_int64(&node.value).is_ok() {
            "!!int"
        } else if go_parse_float(&node.value).is_some() {
            "!!float"
        } else {
            return Err(Error(format!(
                "cannot convert node value [{}] at path {} of tag {} to number",
                node.value,
                d.arena.get_nice_path(candidate),
                node.tag
            )));
        };
        results.push(
            d.arena
                .create_replacement(candidate, Kind::Scalar, tag, &node.value),
        );
    }
    Ok(context.child_context(results))
}

fn entry_seq_for(d: &mut Evaluator, key: NodeId, value: NodeId) -> NodeId {
    let key_key = d.arena.push(Node::scalar("!!str", "key"));
    let value_key = d.arena.push(Node::scalar("!!str", "value"));
    let candidate = d.arena.push(Node {
        kind: Kind::Mapping,
        tag: "!!map".to_string(),
        ..Node::default()
    });
    d.arena.add_key_value_child(candidate, key_key, key);
    d.arena.add_key_value_child(candidate, value_key, value);
    candidate
}

pub fn to_entries_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let node = d.arena.get(candidate).clone();
        match node.kind {
            Kind::Mapping => {
                let sequence =
                    d.arena
                        .create_replacement_with_comments(candidate, Kind::Sequence, "!!seq", 0);
                let mut index = 0;
                while index + 1 < node.content.len() {
                    let entry = entry_seq_for(d, node.content[index], node.content[index + 1]);
                    d.arena.add_child(sequence, entry);
                    index += 2;
                }
                results.push(sequence);
            }
            Kind::Sequence => {
                let sequence =
                    d.arena
                        .create_replacement_with_comments(candidate, Kind::Sequence, "!!seq", 0);
                for (index, value) in node.content.iter().enumerate() {
                    let key = d.arena.push(Node::scalar("!!int", &index.to_string()));
                    let entry = entry_seq_for(d, key, *value);
                    d.arena.add_child(sequence, entry);
                }
                results.push(sequence);
            }
            _ => {
                if node.tag != "!!null" {
                    return Err(Error(format!("{} has no keys", node.tag)));
                }
            }
        }
    }
    Ok(context.child_context(results))
}

fn parse_entry(
    d: &mut Evaluator,
    candidate: NodeId,
    position: usize,
) -> Result<(NodeId, NodeId), Error> {
    let prefs = TraversePrefs {
        dont_auto_create: true,
        ..TraversePrefs::default()
    };
    let keys = traverse_map_by_key(d, candidate, "key", prefs)?;
    if keys.len() != 1 {
        return Err(Error(format!(
            "expected to find one 'key' entry but found {} in position {position}",
            keys.len()
        )));
    }
    let values = traverse_map_by_key(d, candidate, "value", prefs)?;
    if values.len() != 1 {
        return Err(Error(format!(
            "expected to find one 'value' entry but found {} in position {position}",
            values.len()
        )));
    }
    Ok((keys[0], values[0]))
}

fn from_entries(d: &mut Evaluator, candidate: NodeId) -> Result<NodeId, Error> {
    let node = d.arena.copy_without_content(candidate);
    for (index, entry) in d
        .arena
        .get(candidate)
        .content
        .clone()
        .into_iter()
        .enumerate()
    {
        let (key, value) = parse_entry(d, entry, index)?;
        d.arena.add_key_value_child(node, key, value);
    }
    let result = d.arena.get_mut(node);
    result.kind = Kind::Mapping;
    result.tag = "!!map".to_string();
    Ok(node)
}

pub fn from_entries_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.get(candidate).kind != Kind::Sequence {
            return Err(Error("from entries only runs against arrays".to_string()));
        }
        results.push(from_entries(d, candidate)?);
    }
    Ok(context.child_context(results))
}

pub fn with_entries_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let to_entries = to_entries_operator(d, context.clone_context(), expr)?;
    let mut results = Vec::new();
    for candidate in to_entries.nodes().to_vec() {
        let splatted = splat(
            d,
            context.single_child_context(candidate),
            TraversePrefs::default(),
        )?;
        let mut new_results = Vec::new();
        for item in splatted.nodes().to_vec() {
            let result =
                d.get_matching_nodes_opt(splatted.single_child_context(item), expr.rhs())?;
            new_results.extend(result.nodes().iter().copied());
        }
        let self_exp = ExprNode::leaf(Operation::new(OpType::SelfReference));
        let collected = collect_together(d, &splatted.child_context(new_results), &self_exp)?;
        let source = d.arena.get(candidate).clone();
        {
            let node = d.arena.get_mut(collected);
            node.leading_content = source.leading_content;
            node.head_comment = source.head_comment;
            node.foot_comment = source.foot_comment;
        }
        let from = from_entries_operator(d, context.single_child_context(collected), expr)?;
        results.extend(from.nodes().iter().copied());
    }
    Ok(context.child_context(results))
}
