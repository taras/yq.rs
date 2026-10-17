//! The regular expression operators from `operator_strings.go`: sub, match,
//! capture and test. Go's RE2 syntax is close enough to `regex-lite` for
//! the patterns yq users write.

use regex_lite::Regex;

use crate::context::Context;
use crate::eval::{create_boolean_candidate, Evaluator};
use crate::node::{Kind, Node, NodeId};
use crate::operation::{ExprNode, OpType};
use crate::Error;

fn compile(pattern: &str) -> Result<Regex, Error> {
    Regex::new(pattern).map_err(|e| Error(format!("error parsing regexp: {e}")))
}

/// Go's `$name` / `${name}` expansion for `ReplaceAllString`, which also
/// accepts `$1`.
fn expand(regex: &Regex, caps: &regex_lite::Captures<'_>, replacement: &str) -> String {
    let mut out = String::new();
    let mut rest = replacement;
    while let Some(index) = rest.find('$') {
        out.push_str(&rest[..index]);
        rest = &rest[index + 1..];
        if rest.starts_with('$') {
            out.push('$');
            rest = &rest[1..];
            continue;
        }
        let (name, remaining) = if let Some(stripped) = rest.strip_prefix('{') {
            match stripped.find('}') {
                Some(end) => (&stripped[..end], &stripped[end + 1..]),
                None => {
                    out.push('$');
                    continue;
                }
            }
        } else {
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            (&rest[..end], &rest[end..])
        };
        if name.is_empty() {
            out.push('$');
            continue;
        }
        if let Ok(number) = name.parse::<usize>() {
            if let Some(m) = caps.get(number) {
                out.push_str(m.as_str());
            }
        } else if regex.capture_names().any(|n| n == Some(name)) {
            if let Some(m) = caps.name(name) {
                out.push_str(m.as_str());
            }
        }
        rest = remaining;
    }
    out.push_str(rest);
    out
}

pub fn substitute_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let block = d.rhs(expr)?;
    let regex_nodes = d.get_matching_nodes_opt(context.read_only_clone(), block.lhs())?;
    let pattern = match regex_nodes.first() {
        Some(first) => d.arena.get(first).value.clone(),
        None => String::new(),
    };
    let replacement_nodes = d.get_matching_nodes_opt(context.clone_context(), block.rhs())?;
    let replacement = match replacement_nodes.first() {
        Some(first) => d.arena.get(first).value.clone(),
        None => String::new(),
    };
    let regex = compile(&pattern)?;
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.guess_tag_from_custom_type(candidate) != "!!str" {
            return Err(Error(format!(
                "cannot substitute with {}, can only substitute strings. Hint: Most often you'll want to use '|=' over '=' for this operation",
                d.arena.get(candidate).tag
            )));
        }
        let value = d.arena.get(candidate).value.clone();
        let replaced = regex.replace_all(&value, |caps: &regex_lite::Captures<'_>| {
            expand(&regex, caps, &replacement)
        });
        results.push(
            d.arena
                .create_replacement(candidate, Kind::Scalar, "!!str", &replaced),
        );
    }
    Ok(context.child_context(results))
}

struct MatchArgs {
    regex: Regex,
    global: bool,
}

fn extract_match_arguments(
    d: &mut Evaluator,
    context: &Context,
    expr: &ExprNode,
) -> Result<MatchArgs, Error> {
    let rhs = d.rhs(expr)?;
    let mut regex_exp = Some(rhs);
    let mut global = false;
    if rhs.op.op_type == OpType::Block {
        regex_exp = rhs.lhs();
        let params = d.get_matching_nodes_opt(context.clone_context(), rhs.rhs())?;
        let mut text = match params.first() {
            Some(first) => d.arena.get(first).value.clone(),
            None => String::new(),
        };
        if text.contains('g') {
            text = text.replace('g', "");
            global = true;
        }
        if text.contains('i') {
            return Err(Error(
                "'i' is not a valid option for match. To ignore case, use an expression like match(\"(?i)cat\")".to_string(),
            ));
        }
        if !text.is_empty() {
            return Err(Error(format!(
                "unrecognised match params '{text}', please see docs at https://mikefarah.gitbook.io/yq/operators/string-operators"
            )));
        }
    }
    let regex_nodes = d.get_matching_nodes_opt(context.read_only_clone(), regex_exp)?;
    let pattern = match regex_nodes.first() {
        Some(first) => d.arena.get(first).value.clone(),
        None => String::new(),
    };
    Ok(MatchArgs {
        regex: compile(&pattern)?,
        global,
    })
}

fn add_match(
    d: &mut Evaluator,
    content: &mut Vec<NodeId>,
    text: Option<&str>,
    offset: i64,
    name: &str,
) {
    content.push(d.arena.push(Node::scalar("!!str", "string")));
    match text {
        Some(text) => content.push(d.arena.push(Node::scalar("!!str", text))),
        None => content.push(d.arena.push(Node::scalar("!!null", "null"))),
    }
    content.push(d.arena.push(Node::scalar("!!str", "offset")));
    content.push(d.arena.push(Node::scalar("!!int", &offset.to_string())));
    content.push(d.arena.push(Node::scalar("!!str", "length")));
    let length = text.map(|t| t.len()).unwrap_or(0);
    content.push(d.arena.push(Node::scalar("!!int", &length.to_string())));
    if !name.is_empty() {
        content.push(d.arena.push(Node::scalar("!!str", "name")));
        content.push(d.arena.push(Node::scalar("!!str", name)));
    }
}

fn all_captures<'t>(args: &MatchArgs, value: &'t str) -> Vec<regex_lite::Captures<'t>> {
    if args.global {
        args.regex.captures_iter(value).collect()
    } else {
        args.regex.captures(value).into_iter().collect()
    }
}

pub fn match_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let args = extract_match_arguments(d, &context, expr)?;
    let names: Vec<String> = args
        .regex
        .capture_names()
        .map(|n| n.unwrap_or("").to_string())
        .collect();
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.guess_tag_from_custom_type(candidate) != "!!str" {
            return Err(Error(format!(
                "cannot match with {}, can only match strings. Hint: Most often you'll want to use '|=' over '=' for this operation",
                d.arena.get(candidate).tag
            )));
        }
        let value = d.arena.get(candidate).value.clone();
        for caps in all_captures(&args, &value) {
            let captures_list = d.arena.push(Node {
                kind: Kind::Sequence,
                ..Node::default()
            });
            for (group, name) in names.iter().enumerate().skip(1) {
                let capture_node = d.arena.push(Node {
                    kind: Kind::Mapping,
                    ..Node::default()
                });
                let mut content = Vec::new();
                let (text, offset) = match caps.get(group) {
                    Some(m) => (Some(m.as_str()), m.start() as i64),
                    None => (None, -1),
                };
                add_match(d, &mut content, text, offset, name);
                d.arena.add_children(capture_node, &content);
                d.arena.add_child(captures_list, capture_node);
            }
            let node = d
                .arena
                .create_replacement(candidate, Kind::Mapping, "!!map", "");
            let mut content = Vec::new();
            let whole = caps.get(0);
            add_match(
                d,
                &mut content,
                whole.map(|m| m.as_str()),
                whole.map(|m| m.start() as i64).unwrap_or(-1),
                "",
            );
            d.arena.add_children(node, &content);
            let key = d.arena.push(Node::scalar("!!str", "captures"));
            d.arena.add_key_value_child(node, key, captures_list);
            results.push(node);
        }
    }
    Ok(context.child_context(results))
}

pub fn capture_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let args = extract_match_arguments(d, &context, expr)?;
    let names: Vec<String> = args
        .regex
        .capture_names()
        .map(|n| n.unwrap_or("").to_string())
        .collect();
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.guess_tag_from_custom_type(candidate) != "!!str" {
            return Err(Error(format!(
                "cannot match with {}, can only match strings. Hint: Most often you'll want to use '|=' over '=' for this operation",
                d.arena.get(candidate).tag
            )));
        }
        let value = d.arena.get(candidate).value.clone();
        for caps in all_captures(&args, &value) {
            let captures_node = d
                .arena
                .create_replacement(candidate, Kind::Mapping, "!!map", "");
            for (group, name) in names.iter().enumerate().skip(1) {
                let key = d.arena.push(Node::scalar("!!str", name));
                let value_node = match caps.get(group) {
                    Some(m) => d.arena.push(Node::scalar("!!str", m.as_str())),
                    None => d.arena.push(Node::scalar("!!null", "null")),
                };
                d.arena.add_key_value_child(captures_node, key, value_node);
            }
            results.push(captures_node);
        }
    }
    Ok(context.child_context(results))
}

pub fn test_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let args = extract_match_arguments(d, &context, expr)?;
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        if d.arena.guess_tag_from_custom_type(candidate) != "!!str" {
            return Err(Error(format!(
                "cannot match with {}, can only match strings. Hint: Most often you'll want to use '|=' over '=' for this operation",
                d.arena.get(candidate).tag
            )));
        }
        let matched = args.regex.is_match(&d.arena.get(candidate).value);
        results.push(create_boolean_candidate(
            &mut d.arena,
            Some(candidate),
            matched,
        ));
    }
    Ok(context.child_context(results))
}
