//! `operator_env.go`: `env`, `strenv` and `envsubst`, backed by the host's
//! environment table in `EvalOptions`.

use crate::context::Context;
use crate::eval::Evaluator;
use crate::node::{Kind, Node};
use crate::operation::{ExprNode, Prefs};
use crate::Error;

fn lookup(d: &Evaluator, name: &str) -> Option<String> {
    d.env
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

pub fn env_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    if d.disable_env_ops {
        return Err(Error("env operations have been disabled".to_string()));
    }
    let name = expr
        .op
        .node
        .as_ref()
        .map(|node| node.value.clone())
        .unwrap_or_default();
    let raw = lookup(d, &name).unwrap_or_default();
    let string_value = matches!(
        expr.op.prefs,
        Prefs::Env {
            string_value: true,
            ..
        }
    );
    let node = if string_value {
        d.arena.push(Node::scalar("!!str", &raw))
    } else if raw.is_empty() {
        return Err(Error(format!(
            "value for env variable '{name}' not provided in env()"
        )));
    } else {
        let docs = crate::yaml::decode::decode_documents(&mut d.arena, &raw, "", 0)?;
        match docs.first() {
            Some(first) => *first,
            None => return Err(Error("EOF".to_string())),
        }
    };
    Ok(context.single_child_context(node))
}

struct Restrictions {
    no_unset: bool,
    no_empty: bool,
    fail_fast: bool,
}

/// A port of the subset of `a8m/envsubst` yq relies on: `$VAR`, `${VAR}`
/// and the `-`, `:-`, `+`, `:+`, `=`, `:=`, `?`, `:?` modifiers.
fn envsubst(d: &Evaluator, text: &str, restrictions: &Restrictions) -> Result<String, Error> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut errors: Vec<String> = Vec::new();
    let mut i = 0;
    let is_name_char = |c: char| c.is_ascii_alphanumeric() || c == '_';
    while i < chars.len() {
        let c = chars[i];
        if c != '$' || i + 1 >= chars.len() {
            out.push(c);
            i += 1;
            continue;
        }
        let next = chars[i + 1];
        if next == '$' {
            out.push('$');
            i += 2;
            continue;
        }
        if next == '{' {
            let close = match chars[i + 2..].iter().position(|c| *c == '}') {
                Some(offset) => i + 2 + offset,
                None => {
                    out.push(c);
                    i += 1;
                    continue;
                }
            };
            let inner: String = chars[i + 2..close].iter().collect();
            let (name, modifier, argument) = split_modifier(&inner);
            let value = lookup(d, name);
            match substitute(name, value, modifier, argument, restrictions) {
                Ok(text) => out.push_str(&text),
                Err(message) => {
                    if restrictions.fail_fast {
                        return Err(Error(message));
                    }
                    errors.push(message);
                }
            }
            i = close + 1;
            continue;
        }
        if is_name_char(next) && !next.is_ascii_digit() {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && is_name_char(chars[end]) {
                end += 1;
            }
            let name: String = chars[start..end].iter().collect();
            let value = lookup(d, &name);
            match substitute(&name, value, "", "", restrictions) {
                Ok(text) => out.push_str(&text),
                Err(message) => {
                    if restrictions.fail_fast {
                        return Err(Error(message));
                    }
                    errors.push(message);
                }
            }
            i = end;
            continue;
        }
        out.push(c);
        i += 1;
    }
    if !errors.is_empty() {
        return Err(Error(errors.join("\n")));
    }
    Ok(out)
}

fn split_modifier(inner: &str) -> (&str, &str, &str) {
    let name_end = inner
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(inner.len());
    let name = &inner[..name_end];
    let rest = &inner[name_end..];
    for modifier in [":-", ":+", ":=", ":?", "-", "+", "=", "?"] {
        if let Some(argument) = rest.strip_prefix(modifier) {
            return (name, modifier, argument);
        }
    }
    (name, "", rest)
}

fn substitute(
    name: &str,
    value: Option<String>,
    modifier: &str,
    argument: &str,
    restrictions: &Restrictions,
) -> Result<String, String> {
    let set = value.is_some();
    let value = value.unwrap_or_default();
    let empty = value.is_empty();
    match modifier {
        "-" | "=" => {
            if !set {
                return Ok(argument.to_string());
            }
        }
        ":-" | ":=" => {
            if !set || empty {
                return Ok(argument.to_string());
            }
        }
        "+" => {
            return Ok(if set {
                argument.to_string()
            } else {
                String::new()
            });
        }
        ":+" => {
            return Ok(if set && !empty {
                argument.to_string()
            } else {
                String::new()
            });
        }
        "?" if !set => {
            return Err(format!("variable ${{{name}}} {argument}"));
        }
        ":?" if !set || empty => {
            return Err(format!("variable ${{{name}}} {argument}"));
        }
        _ => {}
    }
    if !set && restrictions.no_unset {
        return Err(format!("variable ${{{name}}} not set"));
    }
    if set && empty && restrictions.no_empty {
        return Err(format!("variable ${{{name}}} set but empty"));
    }
    Ok(value)
}

pub fn envsubst_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    if d.disable_env_ops {
        return Err(Error("env operations have been disabled".to_string()));
    }
    let restrictions = match expr.op.prefs {
        Prefs::Env {
            no_unset,
            no_empty,
            fail_fast,
            ..
        } => Restrictions {
            no_unset,
            no_empty,
            fail_fast,
        },
        _ => Restrictions {
            no_unset: false,
            no_empty: false,
            fail_fast: false,
        },
    };
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let node = d.arena.get(candidate).clone();
        if node.tag != "!!str" {
            return Err(Error(format!(
                "cannot substitute with {}, can only substitute strings. Hint: Most often you'll want to use '|=' over '=' for this operation",
                node.tag
            )));
        }
        let value = envsubst(d, &node.value, &restrictions)?;
        results.push(
            d.arena
                .create_replacement(candidate, Kind::Scalar, "!!str", &value),
        );
    }
    Ok(context.child_context(results))
}
