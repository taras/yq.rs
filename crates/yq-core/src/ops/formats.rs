//! The small string formats yq's `@` operators use: base64, base64url, uri,
//! sh, csv, tsv and properties.

use crate::eval::Evaluator;
use crate::node::{find_key_in_map, Kind, Node, NodeId, PathElem};
use crate::util::head_and_line_comment;
use crate::Error;

const STD_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn base64_encode(input: &[u8], url: bool) -> String {
    let alphabet = if url { URL_ALPHABET } else { STD_ALPHABET };
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(alphabet[(triple >> 18) as usize & 63] as char);
        out.push(alphabet[(triple >> 12) as usize & 63] as char);
        if chunk.len() > 1 {
            out.push(alphabet[(triple >> 6) as usize & 63] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(alphabet[triple as usize & 63] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn base64_decode(input: &str, url: bool) -> Result<Vec<u8>, Error> {
    let alphabet = if url { URL_ALPHABET } else { STD_ALPHABET };
    let mut text = input.trim().to_string();
    let pad = text.len() % 4;
    if pad > 0 {
        text.push_str(&"=".repeat(4 - pad));
    }
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        let mut values = [0u32; 4];
        let mut padding = 0;
        for (j, value) in values.iter_mut().enumerate() {
            let c = bytes[i + j];
            if c == b'=' {
                padding += 1;
                continue;
            }
            *value = alphabet
                .iter()
                .position(|a| *a == c)
                .ok_or_else(|| Error(format!("illegal base64 data at input byte {}", i + j)))?
                as u32;
        }
        let triple = (values[0] << 18) | (values[1] << 12) | (values[2] << 6) | values[3];
        out.push((triple >> 16) as u8);
        if padding < 2 {
            out.push((triple >> 8) as u8);
        }
        if padding < 1 {
            out.push(triple as u8);
        }
        i += 4;
    }
    Ok(out)
}

/// Go's `url.QueryEscape`.
pub fn query_escape(input: &str) -> String {
    let mut out = String::new();
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Go's `url.QueryUnescape`.
pub fn query_unescape(input: &str) -> Result<String, Error> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err(Error(format!("invalid URL escape \"{}\"", &input[i..])));
                }
                let hex = &input[i + 1..i + 3];
                let value = u8::from_str_radix(hex, 16)
                    .map_err(|_| Error(format!("invalid URL escape \"{}\"", &input[i..i + 3])))?;
                out.push(value);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|e| Error(e.to_string()))
}

/// The `@sh` encoder: unsafe runs go into single quotes.
pub fn sh_encode(input: &str) -> String {
    let mut out = String::new();
    let mut in_quote = false;
    for c in input.chars() {
        if c == '\'' {
            if in_quote {
                out.push('\'');
                in_quote = false;
            }
            out.push('\\');
        } else if !is_sh_safe(c) && !in_quote {
            out.push('\'');
            in_quote = true;
        }
        out.push(c);
    }
    if in_quote {
        out.push('\'');
    }
    out
}

fn is_sh_safe(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c, '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-')
}

/// Go's `encoding/csv` writer rules for one field.
fn csv_field(value: &str, separator: char) -> String {
    let needs_quotes = value.is_empty() && false
        || value.contains(separator)
        || value.contains('"')
        || value.contains('\r')
        || value.contains('\n')
        || value.starts_with(' ')
        || value.starts_with('\t');
    if !needs_quotes {
        return value.to_string();
    }
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn csv_row(d: &Evaluator, contents: &[NodeId], separator: char) -> Result<String, Error> {
    let mut fields = Vec::new();
    for (i, child) in contents.iter().enumerate() {
        let node = d.arena.get(*child);
        if node.kind != Kind::Scalar {
            return Err(Error(format!(
                "csv encoding only works for arrays of scalars (string/numbers/booleans), child[{i}] is a {}",
                node.tag
            )));
        }
        fields.push(csv_field(&node.value, separator));
    }
    Ok(format!("{}\n", fields.join(&separator.to_string())))
}

pub fn csv_encode(d: &mut Evaluator, node: NodeId, separator: char) -> Result<String, Error> {
    let current = d.arena.get(node).clone();
    if current.kind == Kind::Scalar {
        return Ok(format!("{}\n", current.value));
    }
    if current.kind != Kind::Sequence {
        return Err(Error(format!(
            "csv encoding only works for arrays, got: {}",
            current.tag
        )));
    }
    let first = match current.content.first() {
        Some(first) => *first,
        None => return Ok(String::new()),
    };
    let mut out = String::new();
    match d.arena.get(first).kind {
        Kind::Scalar => out.push_str(&csv_row(d, &current.content, separator)?),
        Kind::Mapping => {
            let headers: Vec<NodeId> = d
                .arena
                .get(first)
                .content
                .iter()
                .step_by(2)
                .copied()
                .collect();
            out.push_str(&csv_row(d, &headers, separator)?);
            for (i, child) in current.content.iter().enumerate() {
                if d.arena.get(*child).kind != Kind::Mapping {
                    return Err(Error(format!(
                        "csv object encoding only works for arrays of flat objects (string key => string/numbers/boolean value), child[{i}] is a {}",
                        d.arena.get(*child).tag
                    )));
                }
                let mut row = Vec::new();
                for header in &headers {
                    let value = match find_key_in_map(&mut d.arena, *child, *header) {
                        Some(index) => d.arena.get(*child).content[index + 1],
                        None => d.arena.push(Node::scalar("!!null", "")),
                    };
                    row.push(value);
                }
                out.push_str(&csv_row(d, &row, separator)?);
            }
        }
        _ => {
            for (i, child) in current.content.iter().enumerate() {
                let child_node = d.arena.get(*child).clone();
                if child_node.kind != Kind::Sequence {
                    return Err(Error(format!(
                        "csv encoding only works for arrays of scalars (string/numbers/booleans), child[{i}] is a {}",
                        child_node.tag
                    )));
                }
                out.push_str(&csv_row(d, &child_node.content, separator)?);
            }
        }
    }
    Ok(out)
}

/// A reader for Go's `encoding/csv` with default settings.
fn csv_read_rows(text: &str, separator: char) -> Result<Vec<Vec<String>>, Error> {
    let mut rows = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let mut row = Vec::new();
        loop {
            let mut field = String::new();
            if chars.get(i) == Some(&'"') {
                i += 1;
                loop {
                    match chars.get(i) {
                        None => {
                            return Err(Error(
                                "record on line 1: wrong number of fields".to_string(),
                            ))
                        }
                        Some('"') => {
                            if chars.get(i + 1) == Some(&'"') {
                                field.push('"');
                                i += 2;
                            } else {
                                i += 1;
                                break;
                            }
                        }
                        Some(c) => {
                            field.push(*c);
                            i += 1;
                        }
                    }
                }
            } else {
                while i < chars.len() && chars[i] != separator && chars[i] != '\n' {
                    if chars[i] != '\r' || chars.get(i + 1) != Some(&'\n') {
                        field.push(chars[i]);
                    }
                    i += 1;
                }
            }
            row.push(field);
            if chars.get(i) == Some(&separator) {
                i += 1;
                continue;
            }
            if chars.get(i) == Some(&'\n') {
                i += 1;
            }
            break;
        }
        let blank = row.len() == 1 && row[0].is_empty();
        if !blank {
            rows.push(row);
        }
    }
    Ok(rows)
}

pub fn csv_decode(d: &mut Evaluator, text: &str, separator: char) -> Result<NodeId, Error> {
    let rows = csv_read_rows(text, separator)?;
    let header = match rows.first() {
        Some(header) => header.clone(),
        None => return Err(Error("EOF".to_string())),
    };
    let root = d.arena.push(Node {
        kind: Kind::Sequence,
        tag: "!!seq".to_string(),
        ..Node::default()
    });
    for row in rows.iter().skip(1) {
        let object = d.arena.push(Node {
            kind: Kind::Mapping,
            tag: "!!map".to_string(),
            ..Node::default()
        });
        for (i, name) in header.iter().enumerate() {
            let content = row.get(i).cloned().unwrap_or_default();
            let key = d.arena.push(Node::scalar("!!str", name));
            let value = match crate::yaml::decode::parse_snippet(&mut d.arena, &content) {
                Ok(node)
                    if d.arena.get(node).kind == Kind::Scalar
                        && d.arena.get(node).value == content =>
                {
                    node
                }
                _ => d.arena.push(Node::scalar("!!str", &content)),
            };
            d.arena.add_key_value_child(object, key, value);
        }
        d.arena.add_child(root, object);
    }
    Ok(root)
}

fn props_append_path(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn props_escape(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{000C}' => out.push_str("\\f"),
            other => out.push(other),
        }
    }
    out
}

fn props_encode_node(
    d: &mut Evaluator,
    node: NodeId,
    path: &str,
    key: Option<NodeId>,
    out: &mut String,
) -> Result<(), Error> {
    let current = d.arena.get(node).clone();
    let mut comments = String::new();
    if let Some(key) = key {
        comments.push_str(&head_and_line_comment(d.arena.get(key)));
    }
    comments.push_str(&head_and_line_comment(&current));
    match current.kind {
        Kind::Scalar => {
            for line in comments.lines() {
                out.push_str(&format!("#{line}\n"));
            }
            out.push_str(&format!("{path} = {}\n", props_escape(&current.value)));
            Ok(())
        }
        Kind::Sequence => {
            for (index, child) in current.content.iter().enumerate() {
                props_encode_node(
                    d,
                    *child,
                    &props_append_path(path, &index.to_string()),
                    None,
                    out,
                )?;
            }
            Ok(())
        }
        Kind::Mapping => {
            let mut index = 0;
            while index + 1 < current.content.len() {
                let key = current.content[index];
                let key_text = d.arena.get(key).value.clone();
                props_encode_node(
                    d,
                    current.content[index + 1],
                    &props_append_path(path, &key_text),
                    Some(key),
                    out,
                )?;
                index += 2;
            }
            Ok(())
        }
        Kind::Alias => match current.alias {
            Some(target) => props_encode_node(d, target, path, None, out),
            None => Ok(()),
        },
    }
}

pub fn props_encode(d: &mut Evaluator, node: NodeId) -> Result<String, Error> {
    if d.arena.get(node).kind == Kind::Scalar {
        return Ok(format!("{}\n", d.arena.get(node).value));
    }
    let mut out = String::new();
    props_encode_node(d, node, "", None, &mut out)?;
    Ok(out)
}

fn props_key_path(key: &str) -> Vec<PathElem> {
    key.split('.')
        .map(|segment| match segment.parse::<i64>() {
            Ok(number) => PathElem::Index(number),
            Err(_) => PathElem::Key(segment.to_string()),
        })
        .collect()
}

pub fn props_decode(d: &mut Evaluator, text: &str) -> Result<NodeId, Error> {
    let root = d.arena.push(Node {
        kind: Kind::Mapping,
        tag: "!!map".to_string(),
        ..Node::default()
    });
    let context = crate::context::Context::new(vec![root]);
    let mut comments: Vec<String> = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_start();
        if line.is_empty() {
            continue;
        }
        if let Some(comment) = line.strip_prefix('#').or_else(|| line.strip_prefix('!')) {
            comments.push(comment.trim().to_string());
            continue;
        }
        let split = line
            .find(['=', ':'])
            .or_else(|| line.find([' ', '\t']))
            .unwrap_or(line.len());
        let key = line[..split].trim();
        let value = line[split..]
            .trim_start_matches([' ', '\t'])
            .trim_start_matches(['=', ':'])
            .trim_start_matches([' ', '\t']);
        let path = props_key_path(key);
        if !comments.is_empty() {
            let head = format!("# {}", comments.join("\n"));
            let last = path.last().map(|p| p.to_string()).unwrap_or_default();
            let key_node = d.arena.push(Node::scalar("!!str", &last));
            let tag = d.arena.guess_tag_from_custom_type(key_node);
            {
                let node = d.arena.get_mut(key_node);
                node.tag = tag;
                node.head_comment = head;
            }
            let tree = crate::operation::ExprNode::binary(
                crate::operation::Operation::with_prefs(
                    crate::operation::OpType::Assign,
                    crate::operation::Prefs::Assign(crate::node::AssignPrefs::default()),
                ),
                crate::eval::create_traversal_tree(
                    &path,
                    crate::operation::TraversePrefs::default(),
                    true,
                ),
                crate::operation::ExprNode::leaf(crate::operation::Operation::reference(key_node)),
            );
            d.get_matching_nodes(context.clone_context(), &tree)?;
            comments.clear();
        }
        let value_node = d.arena.push(Node::string_scalar(value));
        let tag = d.arena.guess_tag_from_custom_type(value_node);
        d.arena.get_mut(value_node).tag = tag;
        d.deeply_assign(context.clone_context(), &path, value_node)?;
    }
    Ok(root)
}
