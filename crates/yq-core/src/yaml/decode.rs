//! YAML text to arena nodes: the port of `decoder_yaml.go` and the
//! `UnmarshalYAML` half of `candidate_node_yaml.go`, with go-yaml's composer
//! and resolver folded in.

use std::collections::HashMap;

use crate::node::{
    Arena, Kind, Node, NodeId, DOUBLE_QUOTED_STYLE, FLOW_STYLE, FOLDED_STYLE, LITERAL_STYLE,
    SINGLE_QUOTED_STYLE, TAGGED_STYLE,
};
use crate::yaml::resolve::resolve_tag;
use crate::yaml::saphyr::{Event, EventComments, Parser, ScalarStyle, Span, StrInput, Tag};
use crate::Error;

pub const DOC_SEPARATOR_MARKER: &str = "$yqDocSeparator$";

/// Splits the input into yq's "leading content" (comments, directives and
/// blank lines before the first real line, with `---` lines replaced by the
/// separator marker) and the YAML text that follows.
pub fn process_read_stream(input: &str) -> (String, String) {
    let mut leading = String::new();
    let mut rest = String::new();
    let mut remainder = input;
    while !remainder.is_empty() {
        let (line_with_newline, next) = match remainder.find('\n') {
            Some(index) => (&remainder[..index + 1], &remainder[index + 1..]),
            None => (remainder, ""),
        };
        let (line, newline) = if let Some(stripped) = line_with_newline.strip_suffix("\r\n") {
            (stripped, "\r\n")
        } else if let Some(stripped) = line_with_newline.strip_suffix('\n') {
            (stripped, "\n")
        } else {
            (line_with_newline, "")
        };
        let trimmed = line.trim();
        if trimmed == "---" {
            leading.push_str(DOC_SEPARATOR_MARKER);
            leading.push_str(newline);
            remainder = next;
            continue;
        }
        if let Some(after) = separator_prefix(line) {
            let sep_newline = if newline.is_empty() { "\n" } else { newline };
            leading.push_str(DOC_SEPARATOR_MARKER);
            leading.push_str(sep_newline);
            // The Go decoder pushes the remainder back without its newline, so
            // it runs into the following line.
            let joined = format!("{after}{next}");
            if joined.is_empty() {
                return (leading, rest);
            }
            rest = joined;
            let (new_leading, new_rest) = process_read_stream(&rest);
            leading.push_str(&new_leading);
            return (leading, new_rest);
        }
        if trimmed.is_empty() || trimmed.starts_with('#') || line.trim_start().starts_with("%YAML")
        {
            leading.push_str(line);
            leading.push_str(newline);
            remainder = next;
            continue;
        }
        rest.push_str(line_with_newline);
        rest.push_str(next);
        return (leading, rest);
    }
    (leading, rest)
}

fn separator_prefix(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let after = trimmed.strip_prefix("---")?;
    if after.is_empty() || !after.starts_with([' ', '\t']) {
        return None;
    }
    Some(after.trim_start_matches([' ', '\t']))
}

/// Reads every document in `input`, stamping document index, filename and
/// file index on each root, as `readDocuments` does.
pub fn decode_documents(
    arena: &mut Arena,
    input: &str,
    filename: &str,
    file_index: usize,
) -> Result<Vec<NodeId>, Error> {
    decode_documents_with(arena, input, filename, file_index, true)
}

/// `decode_documents` with control over yq's leading-content preprocessing.
/// `load` turns it off: comments then reach the parser, and a file with
/// nothing but comments becomes a null node carrying the whole text.
pub fn decode_documents_with(
    arena: &mut Arena,
    input: &str,
    filename: &str,
    file_index: usize,
    preprocess_leading_content: bool,
) -> Result<Vec<NodeId>, Error> {
    let (leading_content, text) = if preprocess_leading_content {
        process_read_stream(input)
    } else {
        (String::new(), input.to_string())
    };
    let mut decoder = Decoder::new(arena, &text);
    let mut roots = decoder
        .decode_all()
        .map_err(|e| Error(format!("bad file '{filename}': {e}")))?;
    let leading_content = if roots.is_empty() && !preprocess_leading_content {
        input.to_string()
    } else {
        leading_content
    };
    if roots.is_empty() && !leading_content.is_empty() {
        let blank = arena.push(Node {
            kind: Kind::Scalar,
            tag: "!!null".to_string(),
            ..Node::default()
        });
        roots.push(blank);
    }
    for (index, root) in roots.iter().enumerate() {
        let node = arena.get_mut(*root);
        node.document = index;
        node.filename = filename.to_string();
        node.file_index = file_index;
        node.evaluate_together = true;
        if index == 0 && !leading_content.is_empty() {
            node.leading_content = leading_content.clone();
        }
    }
    Ok(roots)
}

/// `parseSnippet`: parses a scalar's text as YAML to learn its real type.
pub fn parse_snippet(arena: &mut Arena, value: &str) -> Result<NodeId, Error> {
    if value.is_empty() {
        return Ok(arena.push(Node::scalar("!!null", "")));
    }
    let (leading_content, text) = process_read_stream(value);
    let mut decoder = Decoder::new(arena, &text);
    let mut roots = decoder.decode_all()?;
    if roots.is_empty() && !leading_content.is_empty() {
        roots.push(arena.push(Node::scalar("!!null", "")));
    }
    let result = match roots.first() {
        Some(root) => *root,
        None => return Err(Error("EOF".to_string())),
    };
    if arena.get(result).kind == Kind::Scalar {
        arena.get_mut(result).line_comment = leading_content;
    } else {
        arena.get_mut(result).head_comment = leading_content;
    }
    if arena.get(result).tag == "!!str" {
        // The original text is kept because decoding drops newlines.
        let line_comment = arena.get(result).line_comment.clone();
        let fresh = arena.push(Node::scalar("!!str", value));
        arena.get_mut(fresh).line_comment = line_comment;
        return Ok(fresh);
    }
    let node = arena.get_mut(result);
    node.line = 0;
    node.column = 0;
    Ok(result)
}

struct Decoder<'a, 'input> {
    arena: &'a mut Arena,
    parser: Parser<'input, StrInput<'input>>,
    anchors: HashMap<String, NodeId>,
    pending: Option<(Event<'input>, Span, EventComments)>,
}

impl<'a, 'input> Decoder<'a, 'input> {
    fn new(arena: &'a mut Arena, text: &'input str) -> Self {
        Decoder {
            arena,
            parser: Parser::new_from_str(text),
            anchors: HashMap::new(),
            pending: None,
        }
    }

    fn next(&mut self) -> Result<(Event<'input>, Span, EventComments), Error> {
        if let Some(event) = self.pending.take() {
            return Ok(event);
        }
        match self.parser.next_event() {
            Some(Ok((event, span))) => Ok((event, span, self.parser.event_comments().clone())),
            Some(Err(error)) => Err(Error(format!("yaml: {error}"))),
            None => Ok((Event::StreamEnd, Span::default(), EventComments::default())),
        }
    }

    fn decode_all(&mut self) -> Result<Vec<NodeId>, Error> {
        let mut roots = Vec::new();
        loop {
            let (event, _, comments) = self.next()?;
            match event {
                Event::StreamStart | Event::DocumentEnd => {}
                Event::StreamEnd => return Ok(roots),
                Event::DocumentStart(_) => {
                    let (first, span, first_comments) = self.next()?;
                    let root = self.compose(first, span, first_comments, None)?;
                    // The document node's comments wrap the root's, as yq's
                    // decoder concatenates them.
                    let mut foot = String::new();
                    let (next_event, next_span, next_comments) = self.next()?;
                    if matches!(next_event, Event::DocumentEnd) {
                        foot = next_comments.foot.clone();
                    }
                    self.pending = Some((next_event, next_span, next_comments));
                    let node = self.arena.get_mut(root);
                    if !comments.head.is_empty() {
                        node.head_comment = format!("{}{}", comments.head, node.head_comment);
                    }
                    if !foot.is_empty() {
                        node.foot_comment = format!("{}{}", foot, node.foot_comment);
                    }
                    roots.push(root);
                }
                other => {
                    return Err(Error(format!("yaml: unexpected event {other:?}")));
                }
            }
        }
    }

    fn compose(
        &mut self,
        event: Event<'input>,
        span: Span,
        comments: EventComments,
        parent: Option<NodeId>,
    ) -> Result<NodeId, Error> {
        let id = self.arena.push(Node {
            parent,
            line: span.start.line(),
            column: span.start.col() + 1,
            head_comment: comments.head,
            line_comment: comments.line,
            foot_comment: comments.foot,
            ..Node::default()
        });
        match event {
            Event::Alias(anchor_id) => {
                let name = self.parser.anchor_name(anchor_id).to_string();
                let target = match self.anchors.get(&name) {
                    Some(target) => *target,
                    None => return Err(Error(format!("yaml: unknown anchor '{name}' referenced"))),
                };
                let node = self.arena.get_mut(id);
                node.kind = Kind::Alias;
                node.value = name;
                node.alias = Some(target);
            }
            Event::Scalar(value, style, anchor_id, tag) => {
                let (tag_text, mut style_bits) = explicit_tag(tag.as_deref());
                style_bits |= match style {
                    ScalarStyle::Plain => 0,
                    ScalarStyle::SingleQuoted => SINGLE_QUOTED_STYLE,
                    ScalarStyle::DoubleQuoted => DOUBLE_QUOTED_STYLE,
                    ScalarStyle::Literal => LITERAL_STYLE,
                    ScalarStyle::Folded => FOLDED_STYLE,
                };
                let resolved = match tag_text {
                    Some(tag) => tag,
                    None => {
                        if style == ScalarStyle::Plain {
                            resolve_tag(&value).to_string()
                        } else {
                            "!!str".to_string()
                        }
                    }
                };
                self.register_anchor(id, anchor_id);
                let node = self.arena.get_mut(id);
                node.kind = Kind::Scalar;
                node.style = style_bits;
                node.tag = resolved;
                node.value = value.into_owned();
            }
            Event::SequenceStart(anchor_id, tag) => {
                let (tag_text, mut style_bits) = explicit_tag(tag.as_deref());
                if self.parser.last_start_was_flow() {
                    style_bits |= FLOW_STYLE;
                }
                self.register_anchor(id, anchor_id);
                {
                    let node = self.arena.get_mut(id);
                    node.kind = Kind::Sequence;
                    node.style = style_bits;
                    node.tag = tag_text.unwrap_or_else(|| "!!seq".to_string());
                }
                let mut index = 0;
                loop {
                    let (child_event, child_span, child_comments) = self.next()?;
                    if matches!(child_event, Event::SequenceEnd) {
                        let node = self.arena.get_mut(id);
                        node.line_comment = child_comments.line;
                        node.foot_comment = child_comments.foot;
                        break;
                    }
                    let key = self.arena.push(Node {
                        kind: Kind::Scalar,
                        tag: "!!int".to_string(),
                        value: index.to_string(),
                        parent: Some(id),
                        is_map_key: true,
                        ..Node::default()
                    });
                    let child = self.compose(child_event, child_span, child_comments, Some(id))?;
                    self.arena.get_mut(child).key = Some(key);
                    self.arena.get_mut(id).content.push(child);
                    index += 1;
                }
            }
            Event::MappingStart(anchor_id, tag) => {
                let (tag_text, mut style_bits) = explicit_tag(tag.as_deref());
                if self.parser.last_start_was_flow() {
                    style_bits |= FLOW_STYLE;
                }
                self.register_anchor(id, anchor_id);
                {
                    let node = self.arena.get_mut(id);
                    node.kind = Kind::Mapping;
                    node.style = style_bits;
                    node.tag = tag_text.unwrap_or_else(|| "!!map".to_string());
                }
                let block = style_bits & FLOW_STYLE == 0;
                loop {
                    let (key_event, key_span, key_comments) = self.next()?;
                    if matches!(key_event, Event::MappingEnd) {
                        let content = self.arena.get(id).content.clone();
                        let node = self.arena.get_mut(id);
                        node.line_comment = key_comments.line;
                        node.foot_comment = key_comments.foot;
                        // A block mapping's foot comment belongs to its last
                        // value, as in go-yaml's composer.
                        if block && !node.foot_comment.is_empty() && content.len() > 1 {
                            let foot = std::mem::take(&mut node.foot_comment);
                            self.arena.get_mut(content[content.len() - 1]).foot_comment = foot;
                        }
                        break;
                    }
                    let key = self.compose(key_event, key_span, key_comments, Some(id))?;
                    self.arena.get_mut(key).is_map_key = true;
                    if block && !self.arena.get(key).foot_comment.is_empty() {
                        // A foot comment on a key is a foot of the prior value.
                        let content = self.arena.get(id).content.clone();
                        if content.len() > 2 {
                            let foot = std::mem::take(&mut self.arena.get_mut(key).foot_comment);
                            self.arena.get_mut(content[content.len() - 1]).foot_comment = foot;
                        }
                    }
                    let (value_event, value_span, value_comments) = self.next()?;
                    let value = self.compose(value_event, value_span, value_comments, Some(id))?;
                    self.arena.get_mut(value).key = Some(key);
                    if self.arena.get(key).foot_comment.is_empty()
                        && !self.arena.get(value).foot_comment.is_empty()
                    {
                        let foot = std::mem::take(&mut self.arena.get_mut(value).foot_comment);
                        self.arena.get_mut(key).foot_comment = foot;
                    }
                    let node = self.arena.get_mut(id);
                    node.content.push(key);
                    node.content.push(value);
                }
            }
            other => {
                return Err(Error(format!(
                    "yaml: unexpected event {other:?} while composing a node"
                )))
            }
        }
        Ok(id)
    }

    fn register_anchor(&mut self, id: NodeId, anchor_id: usize) {
        if anchor_id == 0 {
            return;
        }
        let name = self.parser.anchor_name(anchor_id).to_string();
        if name.is_empty() {
            return;
        }
        self.arena.get_mut(id).anchor = name.clone();
        self.anchors.insert(name, id);
    }
}

/// Converts an explicit tag to yaml.v3's short form and reports whether the
/// node must carry `TaggedStyle`.
fn explicit_tag(tag: Option<&Tag>) -> (Option<String>, u32) {
    let tag = match tag {
        Some(tag) => tag,
        None => return (None, 0),
    };
    let text = short_tag(&format!("{}{}", tag.handle, tag.suffix));
    if text.is_empty() || text == "!" {
        return (None, 0);
    }
    (Some(text), TAGGED_STYLE)
}

/// `shortTag`: `tag:yaml.org,2002:str` becomes `!!str`.
pub fn short_tag(tag: &str) -> String {
    match tag.strip_prefix("tag:yaml.org,2002:") {
        Some(rest) => format!("!!{rest}"),
        None => tag.to_string(),
    }
}

/// `longTag`: `!!str` becomes `tag:yaml.org,2002:str`.
pub fn long_tag(tag: &str) -> String {
    match tag.strip_prefix("!!") {
        Some(rest) => format!("tag:yaml.org,2002:{rest}"),
        None => tag.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_content_is_split_off() {
        let (leading, rest) = process_read_stream("# hi\n---\na: 1\n");
        assert_eq!(leading, "# hi\n$yqDocSeparator$\n");
        assert_eq!(rest, "a: 1\n");
        let (leading, rest) = process_read_stream(" # hi");
        assert_eq!(leading, " # hi");
        assert_eq!(rest, "");
    }

    #[test]
    fn decodes_a_mapping() {
        let mut arena = Arena::new();
        let roots = decode_documents(&mut arena, "a: cat\nb: [1, 2]\n", "sample.yml", 0).unwrap();
        assert_eq!(roots.len(), 1);
        let root = arena.get(roots[0]);
        assert_eq!(root.kind, Kind::Mapping);
        assert_eq!(root.tag, "!!map");
        assert_eq!(root.content.len(), 4);
        let value = arena.get(root.content[1]);
        assert_eq!(value.tag, "!!str");
        assert_eq!(value.value, "cat");
        assert_eq!(value.line, 1);
        assert_eq!(value.column, 4);
        let seq = arena.get(root.content[3]);
        assert_eq!(seq.kind, Kind::Sequence);
        assert_eq!(seq.style, FLOW_STYLE);
        assert_eq!(arena.get(seq.content[1]).tag, "!!int");
        assert_eq!(arena.get_path(seq.content[1]).len(), 2);
    }

    #[test]
    fn decodes_multiple_documents_and_anchors() {
        let mut arena = Arena::new();
        let roots =
            decode_documents(&mut arena, "a: &x 1\nb: *x\n---\nc: 2\n", "sample.yml", 0).unwrap();
        assert_eq!(roots.len(), 2);
        assert_eq!(arena.get(roots[1]).document, 1);
        let root = arena.get(roots[0]);
        let anchored = arena.get(root.content[1]);
        assert_eq!(anchored.anchor, "x");
        let alias = arena.get(root.content[3]);
        assert_eq!(alias.kind, Kind::Alias);
        assert_eq!(alias.alias, Some(root.content[1]));
    }
}
