//! The plain string operators from `operator_strings.go`: trim, upcase,
//! downcase, to_string, join and split.

use crate::context::Context;
use crate::eval::Evaluator;
use crate::node::{Kind, DOUBLE_QUOTED_STYLE};
use crate::operation::{ExprNode, Prefs};
use crate::ops::strings::{encode_to_yaml_string, split_string};
use crate::Error;

pub fn trim_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.guess_tag_from_custom_type(candidate) != "!!str" {
            return Err(Error(format!(
                "cannot trim {}, can only operate on strings. ",
                d.arena.get(candidate).tag
            )));
        }
        let node = d.arena.get(candidate).clone();
        let trimmed = node.value.trim_matches(|c: char| c.is_whitespace());
        let result = d
            .arena
            .create_replacement(candidate, Kind::Scalar, &node.tag, trimmed);
        d.arena.get_mut(result).style = node.style;
        results.push(result);
    }
    Ok(context.child_context(results))
}

pub fn to_string_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let node = d.arena.get(candidate).clone();
        let result = if node.tag == "!!str" {
            d.arena
                .create_replacement(candidate, Kind::Scalar, "!!str", &node.value)
        } else if node.kind == Kind::Scalar {
            let result = d
                .arena
                .create_replacement(candidate, Kind::Scalar, "!!str", &node.value);
            d.arena.get_mut(result).style = DOUBLE_QUOTED_STYLE;
            result
        } else {
            let text = encode_to_yaml_string(d, candidate)?;
            let result = d
                .arena
                .create_replacement(candidate, Kind::Scalar, "!!str", &text);
            d.arena.get_mut(result).style = DOUBLE_QUOTED_STYLE;
            result
        };
        d.arena.get_mut(result).tag = "!!str".to_string();
        results.push(result);
    }
    Ok(context.child_context(results))
}

pub fn change_case_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let upper = matches!(expr.op.prefs, Prefs::ChangeCase { upper: true });
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.guess_tag_from_custom_type(candidate) != "!!str" {
            return Err(Error(format!(
                "cannot change case with {}, can only operate on strings. ",
                d.arena.get(candidate).tag
            )));
        }
        let node = d.arena.get(candidate).clone();
        let value = if upper {
            node.value.to_uppercase()
        } else {
            node.value.to_lowercase()
        };
        let result = d
            .arena
            .create_replacement(candidate, Kind::Scalar, &node.tag, &value);
        d.arena.get_mut(result).style = node.style;
        results.push(result);
    }
    Ok(context.child_context(results))
}

pub fn join_string_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let rhs = d.get_matching_nodes_opt(context.read_only_clone(), expr.rhs())?;
    let join_str = match rhs.first() {
        Some(first) => d.arena.get(first).value.clone(),
        None => String::new(),
    };
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.get(candidate).kind != Kind::Sequence {
            return Err(Error(format!(
                "cannot join with {}, can only join arrays of scalars",
                d.arena.get(candidate).tag
            )));
        }
        let pieces: Vec<String> = d
            .arena
            .get(candidate)
            .content
            .iter()
            .map(|child| {
                let node = d.arena.get(*child);
                if node.tag == "!!null" {
                    String::new()
                } else {
                    node.value.clone()
                }
            })
            .collect();
        let joined = pieces.join(&join_str);
        results.push(
            d.arena
                .create_replacement(candidate, Kind::Scalar, "!!str", &joined),
        );
    }
    Ok(context.child_context(results))
}

pub fn split_string_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let rhs = d.get_matching_nodes_opt(context.read_only_clone(), expr.rhs())?;
    let split_str = match rhs.first() {
        Some(first) => d.arena.get(first).value.clone(),
        None => String::new(),
    };
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.get(candidate).tag == "!!null" {
            continue;
        }
        if d.arena.guess_tag_from_custom_type(candidate) != "!!str" {
            return Err(Error(format!(
                "cannot split {}, can only split strings",
                d.arena.get(candidate).tag
            )));
        }
        let value = d.arena.get(candidate).value.clone();
        let pieces = split_string(d, &value, &split_str);
        let result = d
            .arena
            .create_replacement(candidate, Kind::Sequence, "!!seq", "");
        d.arena.add_children(result, &pieces);
        results.push(result);
    }
    Ok(context.child_context(results))
}
