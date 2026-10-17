//! `operator_encoder_decoder.go`: `to_yaml`, `to_json`, `from_yaml`,
//! `from_json` and friends. Only YAML and JSON are supported.

use crate::context::Context;
use crate::eval::Evaluator;
use crate::node::{Kind, Node, NodeId};
use crate::operation::{EncodeFormat, ExprNode, Prefs};
use crate::printer::{Printer, PrinterPrefs};
use crate::Error;
use crate::Format;

/// `encodeToString`.
pub fn encode_to_string(
    d: &mut Evaluator,
    candidate: NodeId,
    format: EncodeFormat,
    indent: usize,
) -> Result<String, Error> {
    let prefs = match format {
        EncodeFormat::Yaml => PrinterPrefs {
            format: Format::Yaml,
            indent,
            unwrap_scalar: true,
            print_doc_separators: true,
        },
        EncodeFormat::Json => PrinterPrefs {
            format: Format::Json,
            indent,
            unwrap_scalar: false,
            print_doc_separators: true,
        },
        EncodeFormat::Base64 | EncodeFormat::Base64Url => {
            if d.arena.guess_tag_from_custom_type(candidate) != "!!str" {
                return Err(Error(format!(
                    "cannot encode {} as base64, can only operate on strings",
                    d.arena.get(candidate).tag
                )));
            }
            let value = d.arena.get(candidate).value.clone();
            return Ok(crate::ops::formats::base64_encode(
                value.as_bytes(),
                format == EncodeFormat::Base64Url,
            ));
        }
        EncodeFormat::Uri | EncodeFormat::Sh => {
            if d.arena.guess_tag_from_custom_type(candidate) != "!!str" {
                return Err(Error(format!(
                    "cannot encode {} as URI, can only operate on strings. Please first pipe through another encoding operator to convert the value to a string",
                    d.arena.get(candidate).tag
                )));
            }
            let value = d.arena.get(candidate).value.clone();
            return Ok(if format == EncodeFormat::Uri {
                crate::ops::formats::query_escape(&value)
            } else {
                crate::ops::formats::sh_encode(&value)
            });
        }
        EncodeFormat::Csv => {
            crate::ops::meta::explode_node(&mut d.arena, candidate, false)?;
            return crate::ops::formats::csv_encode(d, candidate, ',');
        }
        EncodeFormat::Tsv => {
            crate::ops::meta::explode_node(&mut d.arena, candidate, false)?;
            return crate::ops::formats::csv_encode(d, candidate, '\t');
        }
        EncodeFormat::Props => {
            crate::ops::meta::explode_node(&mut d.arena, candidate, false)?;
            return crate::ops::formats::props_encode(d, candidate);
        }
        EncodeFormat::Xml => return Err(Error("no support for output format".to_string())),
    };
    let mut printer = Printer::new(prefs);
    printer.print_node(&mut d.arena, candidate)?;
    Ok(printer.into_output())
}

fn chomp(text: &str) -> String {
    text.trim_end_matches('\n').to_string()
}

pub fn encode_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let (format, indent) = match expr.op.prefs {
        Prefs::Encode { format, indent } => (format, indent),
        _ => (EncodeFormat::Yaml, 2),
    };
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let mut text = encode_to_string(d, candidate, format, indent)?;
        let key = format!("decoded: {}", d.arena.get_key(candidate));
        if let Some(original_list) = context.get_variable(&key) {
            // Go's check is `[^\n].*\n$`: the last line is non-empty.
            let has_only_one_newline = text.ends_with('\n')
                && !text[..text.len() - 1].ends_with('\n')
                && !text[..text.len() - 1].is_empty();
            if let (Some(original), true) = (original_list.first(), has_only_one_newline) {
                if !d.arena.get(*original).value.ends_with('\n') {
                    text = chomp(&text);
                }
            }
        }
        if (format == EncodeFormat::Json && indent == 0)
            || format == EncodeFormat::Csv
            || format == EncodeFormat::Tsv
        {
            text = chomp(&text);
        }
        results.push(
            d.arena
                .create_replacement(candidate, Kind::Scalar, "!!str", &text),
        );
    }
    Ok(context.child_context(results))
}

pub fn decode_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let format = match expr.op.prefs {
        Prefs::Decode { format } => format,
        _ => EncodeFormat::Yaml,
    };
    let mut context = context;
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let key = format!("decoded: {}", d.arena.get_key(candidate));
        context.set_variable(&key, vec![candidate]);
        let text = d.arena.get(candidate).value.clone();
        let decoded = match format {
            EncodeFormat::Yaml => {
                let docs = crate::yaml::decode::decode_documents(&mut d.arena, &text, "", 0)?;
                match docs.first() {
                    Some(first) => *first,
                    None => return Err(Error("EOF".to_string())),
                }
            }
            EncodeFormat::Json => {
                let docs = crate::json::decode::decode_documents(&mut d.arena, &text)?;
                match docs.first() {
                    Some(first) => *first,
                    None => return Err(Error("EOF".to_string())),
                }
            }
            EncodeFormat::Base64 | EncodeFormat::Base64Url => {
                let bytes =
                    crate::ops::formats::base64_decode(&text, format == EncodeFormat::Base64Url)?;
                let decoded = String::from_utf8(bytes).map_err(|e| Error(e.to_string()))?;
                d.arena.push(Node::string_scalar(&decoded))
            }
            EncodeFormat::Uri => {
                let decoded = crate::ops::formats::query_unescape(&text)?;
                d.arena.push(Node::string_scalar(&decoded))
            }
            EncodeFormat::Csv => crate::ops::formats::csv_decode(d, &text, ',')?,
            EncodeFormat::Tsv => crate::ops::formats::csv_decode(d, &text, '\t')?,
            EncodeFormat::Props => crate::ops::formats::props_decode(d, &text)?,
            _ => return Err(Error("no support for input format".to_string())),
        };
        let (parent, key_node) = {
            let source = d.arena.get(candidate);
            (source.parent, source.key)
        };
        let node = d.arena.get_mut(decoded);
        node.key = key_node;
        node.parent = parent;
        results.push(decoded);
    }
    Ok(context.child_context(results))
}
