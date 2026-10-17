//! Arena nodes to JSON (`MarshalJSON` in `candidate_node_json.go` and
//! `encoder_json.go`). Go's encoder with `SetIndent("", indent)` produces
//! pretty output when the indent is non-empty and compact output otherwise.

use crate::node::{Arena, Kind, NodeId, ValueRep};
use crate::util::{format_float, parse_float};
use crate::yaml::resolve::go_parse_float;
use crate::Error;

#[derive(Debug, Clone, Copy)]
pub struct JsonPrefs {
    pub indent: usize,
    pub unwrap_scalar: bool,
}

pub fn encode(arena: &mut Arena, id: NodeId, prefs: &JsonPrefs) -> Result<String, Error> {
    let node = arena.get(id);
    if node.kind == Kind::Scalar && prefs.unwrap_scalar {
        return Ok(format!("{}\n", node.value));
    }
    let indent = " ".repeat(prefs.indent);
    let mut out = String::new();
    write_node(arena, id, &indent, 0, &mut out)?;
    out.push('\n');
    Ok(out)
}

fn newline(out: &mut String, indent: &str, depth: usize) {
    if indent.is_empty() {
        return;
    }
    out.push('\n');
    for _ in 0..depth {
        out.push_str(indent);
    }
}

fn write_node(
    arena: &mut Arena,
    id: NodeId,
    indent: &str,
    depth: usize,
    out: &mut String,
) -> Result<(), Error> {
    match arena.get(id).kind {
        Kind::Alias => {
            let target = arena.get(id).alias;
            match target {
                Some(target) => write_node(arena, target, indent, depth, out),
                None => {
                    out.push_str("null");
                    Ok(())
                }
            }
        }
        Kind::Scalar => {
            let real_tag = arena.guess_tag_from_custom_type(id);
            if real_tag == "!!float" {
                if let Some(raw) = json_float_literal(&arena.get(id).value) {
                    out.push_str(&raw);
                    return Ok(());
                }
            }
            match arena.get_value_rep(id)? {
                ValueRep::Int(value) => out.push_str(&value.to_string()),
                ValueRep::Float(value) => {
                    if value.is_nan() || value.is_infinite() {
                        return Err(Error(format!(
                            "json: unsupported value: {}",
                            format_float(value)
                        )));
                    }
                    out.push_str(&go_json_float(value));
                }
                ValueRep::Bool(value) => out.push_str(if value { "true" } else { "false" }),
                ValueRep::Null => out.push_str("null"),
                ValueRep::Str(value) => write_string(&value, out),
            }
            Ok(())
        }
        Kind::Mapping => {
            let content = arena.get(id).content.clone();
            if content.is_empty() {
                out.push_str("{}");
                return Ok(());
            }
            out.push('{');
            let mut index = 0;
            while index + 1 < content.len() {
                newline(out, indent, depth + 1);
                write_string(&arena.get(content[index]).value, out);
                out.push(':');
                if !indent.is_empty() {
                    out.push(' ');
                }
                write_node(arena, content[index + 1], indent, depth + 1, out)?;
                if index + 2 < content.len() {
                    out.push(',');
                }
                index += 2;
            }
            newline(out, indent, depth);
            out.push('}');
            Ok(())
        }
        Kind::Sequence => {
            let content = arena.get(id).content.clone();
            if content.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            for (index, child) in content.iter().enumerate() {
                newline(out, indent, depth + 1);
                write_node(arena, *child, indent, depth + 1, out)?;
                if index + 1 < content.len() {
                    out.push(',');
                }
            }
            newline(out, indent, depth);
            out.push(']');
            Ok(())
        }
    }
}

/// Go's `encoding/json` string escaping with `SetEscapeHTML(false)`.
pub fn write_string(value: &str, out: &mut String) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `jsonFloatLiteral`: keeps the source text of a float when it is already a
/// JSON number with a fraction or exponent.
fn json_float_literal(raw: &str) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let value = go_parse_float(raw)?;
    if value.is_infinite() || value.is_nan() {
        return None;
    }
    if is_json_number_literal(raw) {
        return Some(raw.to_string());
    }
    let mut formatted = go_format_float_f(value);
    if !formatted.contains(['.', 'e', 'E']) {
        formatted.push_str(".0");
    }
    Some(formatted)
}

/// Go's `strconv.FormatFloat(f, 'f', -1, 64)`.
fn go_format_float_f(value: f64) -> String {
    if value == value.trunc() && value.abs() < 1e21 {
        return format!("{}", value as i64);
    }
    let text = format!("{value}");
    if text.contains('e') {
        // Rust's shortest repr switched to exponent form; expand it.
        return format!("{value:.0}");
    }
    text
}

/// Go's `encoding/json` float formatting: `%g`-like with exponent form for
/// very large or small magnitudes.
pub fn go_json_float(value: f64) -> String {
    let abs = value.abs();
    if abs != 0.0 && (abs < 1e-6 || abs >= 1e21) {
        let formatted = format!("{value:e}");
        let (mantissa, exp) = formatted.split_once('e').unwrap_or((&formatted, "0"));
        let exp_value: i32 = exp.parse().unwrap_or(0);
        let sign = if exp_value < 0 { '-' } else { '+' };
        return format!("{mantissa}e{sign}{:02}", exp_value.abs());
    }
    let text = format!("{value}");
    text
}

fn is_json_number_literal(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    if bytes[i] == b'-' {
        i += 1;
        if i == bytes.len() {
            return false;
        }
    }
    if bytes[i] == b'0' {
        i += 1;
    } else if (b'1'..=b'9').contains(&bytes[i]) {
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    } else {
        return false;
    }
    let mut has_fraction = false;
    if i < bytes.len() && bytes[i] == b'.' {
        has_fraction = true;
        i += 1;
        if i == bytes.len() || !bytes[i].is_ascii_digit() {
            return false;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    let mut has_exponent = false;
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        has_exponent = true;
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        if i == bytes.len() || !bytes[i].is_ascii_digit() {
            return false;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    i == bytes.len() && (has_fraction || has_exponent)
}

#[allow(dead_code)]
fn parse_float_for_tests(text: &str) -> Result<f64, Error> {
    parse_float(text)
}
