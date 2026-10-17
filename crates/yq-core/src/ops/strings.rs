//! `operator_strings.go`: string interpolation and the string helpers the
//! arithmetic operators share. Regex operators live in `regex_ops.rs`.

use crate::context::Context;
use crate::eval::Evaluator;
use crate::node::{Kind, Node, NodeId};
use crate::operation::ExprNode;
use crate::Error;

/// `encodeToYamlString`: a node rendered as YAML with the trailing newline
/// chomped.
pub fn encode_to_yaml_string(d: &mut Evaluator, node: NodeId) -> Result<String, Error> {
    let prefs = crate::yaml::encode::YamlPrefs {
        indent: 2,
        unwrap_scalar: false,
        print_doc_separators: true,
        compact_sequence_indent: false,
    };
    let text = crate::yaml::encode::encode(&d.arena, node, &prefs)?;
    Ok(text.trim_end_matches('\n').to_string())
}

fn evaluate(d: &mut Evaluator, context: &Context, expression: &str) -> Result<String, Error> {
    let parsed = crate::parser::parse_expression(expression)?;
    let result = d.get_matching_nodes_opt(context.clone_context(), parsed.as_ref())?;
    let first = match result.first() {
        Some(first) => first,
        None => return Ok(String::new()),
    };
    if d.arena.get(first).kind != Kind::Scalar {
        return encode_to_yaml_string(d, first);
    }
    Ok(d.arena.get(first).value.clone())
}

fn interpolate(d: &mut Evaluator, context: &Context, text: &str) -> Result<String, Error> {
    let runes: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut expression = String::new();
    let mut in_expression = false;
    let mut nested = 0;
    let mut i = 0;
    while i < runes.len() {
        let c = runes[i];
        if !in_expression {
            if c == '\\' && i + 1 < runes.len() {
                match runes[i + 1] {
                    '(' => {
                        in_expression = true;
                        i += 2;
                        continue;
                    }
                    '\\' => {
                        i += 1;
                    }
                    _ => {}
                }
            }
            out.push(c);
        } else if c == ')' {
            if nested == 0 {
                let value = evaluate(d, context, &expression)?;
                in_expression = false;
                expression.clear();
                out.push_str(&value);
                i += 1;
                continue;
            }
            nested -= 1;
            expression.push(c);
        } else if c == '(' {
            nested += 1;
            expression.push(c);
        } else if c == '\\' && i + 1 < runes.len() && matches!(runes[i + 1], ')' | '\\') {
            expression.push(runes[i + 1]);
            i += 2;
            continue;
        } else {
            expression.push(c);
        }
        i += 1;
    }
    if in_expression {
        return Ok(text.to_string());
    }
    Ok(out)
}

pub fn string_interpolation_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let text = expr.op.string_value.clone();
    if context.is_empty() {
        let value = interpolate(d, &context, &text)?;
        let node = d.arena.push(Node::scalar("!!str", &value));
        return Ok(context.single_child_context(node));
    }
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let value = interpolate(d, &context.single_child_context(candidate), &text)?;
        results.push(d.arena.push(Node::scalar("!!str", &value)));
    }
    Ok(context.child_context(results))
}

/// `split`: the pieces of a string as `!!str` nodes (none for an empty string).
pub fn split_string(d: &mut Evaluator, value: &str, separator: &str) -> Vec<NodeId> {
    if value.is_empty() {
        return Vec::new();
    }
    let pieces: Vec<String> = if separator.is_empty() {
        value.chars().map(|c| c.to_string()).collect()
    } else {
        value.split(separator).map(|s| s.to_string()).collect()
    };
    pieces
        .iter()
        .map(|piece| d.arena.push(Node::scalar("!!str", piece)))
        .collect()
}
