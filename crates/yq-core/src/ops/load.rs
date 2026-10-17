//! `operator_load.go`: `load`, `load_str`, `load_props`, `load_base64`.
//! The engine has no filesystem; file contents come from the host through
//! `EvalOptions::files`, keyed by the exact path the expression uses.

use crate::context::Context;
use crate::eval::Evaluator;
use crate::node::{Kind, Node, NodeId};
use crate::operation::{EncodeFormat, ExprNode, Prefs};
use crate::Error;

fn read_file(d: &Evaluator, filename: &str) -> Result<String, Error> {
    d.files
        .iter()
        .find(|(path, _)| path == filename)
        .map(|(_, contents)| contents.clone())
        .ok_or_else(|| {
            Error(format!(
                "failed to load {filename}: open {filename}: no such file or directory"
            ))
        })
}

fn filename_for(
    d: &mut Evaluator,
    context: &Context,
    candidate: NodeId,
    expr: &ExprNode,
) -> Result<String, Error> {
    let rhs =
        d.get_matching_nodes_opt(context.single_readonly_child_context(candidate), expr.rhs())?;
    match rhs.first() {
        Some(first) => Ok(d.arena.get(first).value.clone()),
        None => Err(Error("filename expression returned nil".to_string())),
    }
}

/// `loadWithDecoder`: no documents gives null, one gives that document,
/// more give a sequence of them.
fn load_with_format(
    d: &mut Evaluator,
    filename: &str,
    format: EncodeFormat,
) -> Result<NodeId, Error> {
    let contents = read_file(d, filename)?;
    let documents = match format {
        EncodeFormat::Yaml => {
            crate::yaml::decode::decode_documents_with(&mut d.arena, &contents, filename, 0, false)?
        }
        EncodeFormat::Props => vec![crate::ops::formats::props_decode(d, &contents)?],
        EncodeFormat::Base64 => {
            let bytes = crate::ops::formats::base64_decode(&contents, false)?;
            let text = String::from_utf8(bytes).map_err(|e| Error(e.to_string()))?;
            vec![d.arena.push(Node::string_scalar(&text))]
        }
        _ => return Err(Error(format!("could not load {filename}"))),
    };
    match documents.len() {
        0 => Ok(d.arena.push(Node::scalar("!!null", ""))),
        1 => Ok(documents[0]),
        _ => {
            let sequence = d.arena.push(Node {
                kind: Kind::Sequence,
                ..Node::default()
            });
            for document in documents {
                d.arena.add_child(sequence, document);
            }
            Ok(sequence)
        }
    }
}

pub fn load_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    if d.disable_file_ops {
        return Err(Error("file operations have been disabled".to_string()));
    }
    let format = match expr.op.prefs {
        Prefs::Load { format } => format,
        _ => EncodeFormat::Yaml,
    };
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let filename = filename_for(d, &context, candidate, expr)?;
        results.push(load_with_format(d, &filename, format)?);
    }
    Ok(context.child_context(results))
}

pub fn load_string_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    if d.disable_file_ops {
        return Err(Error("file operations have been disabled".to_string()));
    }
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let filename = filename_for(d, &context, candidate, expr)?;
        let contents = read_file(d, &filename)?;
        results.push(d.arena.push(Node::scalar("!!str", &contents)));
    }
    Ok(context.child_context(results))
}
