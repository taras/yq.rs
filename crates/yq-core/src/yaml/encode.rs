//! Arena nodes to YAML text: yq's `encoder_yaml.go` on top of go-yaml's
//! desolver and serializer (`desolver.go`, `serializer.go`).

use crate::node::{
    Arena, Kind, NodeId, DOUBLE_QUOTED_STYLE, FLOW_STYLE, FOLDED_STYLE, LITERAL_STYLE,
    SINGLE_QUOTED_STYLE, TAGGED_STYLE,
};
use crate::yaml::decode::{long_tag, DOC_SEPARATOR_MARKER};
use crate::yaml::emitter::{Emitter, Event, EventType, ScalarStyle};
use crate::yaml::resolve::resolve_tag;
use crate::Error;

#[derive(Debug, Clone, Copy)]
pub struct YamlPrefs {
    pub indent: usize,
    pub unwrap_scalar: bool,
    pub print_doc_separators: bool,
    pub compact_sequence_indent: bool,
}

/// `PrintYAMLDocumentSeparator`.
pub fn document_separator(prefs: &YamlPrefs) -> &'static str {
    if prefs.print_doc_separators {
        "---\n"
    } else {
        ""
    }
}

/// `PrintYAMLLeadingContent`: comments and separator markers that preceded
/// the document, re-emitted as comments.
pub fn leading_content(content: &str, print_doc_separators: bool) -> String {
    let mut out = String::new();
    if content.is_empty() {
        return out;
    }
    let mut remainder = content;
    loop {
        let (line, next) = match remainder.find('\n') {
            Some(index) => (&remainder[..index + 1], &remainder[index + 1..]),
            None => (remainder, ""),
        };
        if line.contains(DOC_SEPARATOR_MARKER) {
            if print_doc_separators {
                let ending = if line.ends_with("\r\n") {
                    "---\r\n"
                } else {
                    "---\n"
                };
                out.push_str(ending);
            }
        } else {
            let mut text = line.to_string();
            if !text.is_empty()
                && text != "\n"
                && !text.starts_with('%')
                && !text.trim_start().starts_with('#')
            {
                text = format!("# {text}");
            }
            out.push_str(&text);
        }
        if next.is_empty() {
            if !line.is_empty() && !line.ends_with('\n') {
                out.push('\n');
            }
            break;
        }
        remainder = next;
    }
    out
}

/// `yamlEncoder.Encode`.
pub fn encode(arena: &Arena, id: NodeId, prefs: &YamlPrefs) -> Result<String, Error> {
    let node = arena.get(id);
    let line_ending = if node.leading_content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    if node.kind == Kind::Scalar && prefs.unwrap_scalar {
        let mut value = node.value.clone();
        if node.leading_content.is_empty() || !value.is_empty() {
            value.push_str(line_ending);
        }
        return Ok(value);
    }
    let indent = prefs.indent.clamp(2, 9);
    let mut serializer = Serializer {
        emitter: Emitter::new(indent, prefs.compact_sequence_indent),
        arena,
    };
    serializer
        .emitter
        .emit(Event::new(EventType::StreamStart))?;
    let mut start = Event::new(EventType::DocumentStart);
    start.implicit = true;
    serializer.emitter.emit(start)?;
    serializer.node(id, true, &[])?;
    let mut end = Event::new(EventType::DocumentEnd);
    end.implicit = true;
    serializer.emitter.emit(end)?;
    serializer.emitter.set_open_ended(false);
    serializer.emitter.emit(Event::new(EventType::StreamEnd))?;
    let bytes = serializer.emitter.into_output();
    let mut text = String::from_utf8(bytes).map_err(|e| Error(e.to_string()))?;
    text.push_str(&leading_content(
        &node.foot_comment,
        prefs.print_doc_separators,
    ));
    Ok(text)
}

struct Serializer<'a> {
    emitter: Emitter,
    arena: &'a Arena,
}

struct Desolved {
    tag: String,
    style: u32,
}

/// The desolver: drops tags the reader would infer and quotes strings that
/// would otherwise change type.
fn desolve(kind: Kind, tag: &str, value: &str, style: u32) -> Desolved {
    let mut out = Desolved {
        tag: tag.to_string(),
        style,
    };
    if style & TAGGED_STYLE != 0 || tag.is_empty() {
        return out;
    }
    match kind {
        Kind::Mapping => {
            if tag == "!!map" {
                out.tag.clear();
            }
        }
        Kind::Sequence => {
            if tag == "!!seq" {
                out.tag.clear();
            }
        }
        Kind::Scalar => match tag {
            "!!binary" => {}
            "!!merge" => {
                if value == "<<" {
                    out.tag.clear();
                }
            }
            "!!null" | "!!bool" | "!!str" | "!!int" | "!!float" | "!!timestamp" => {
                let resolved = resolve_tag(value);
                if resolved == tag {
                    out.tag.clear();
                } else if tag == "!!str" {
                    out.tag.clear();
                    let quoted =
                        SINGLE_QUOTED_STYLE | DOUBLE_QUOTED_STYLE | LITERAL_STYLE | FOLDED_STYLE;
                    if style & quoted == 0 {
                        out.style |= DOUBLE_QUOTED_STYLE;
                    }
                } else if tag == "!!float" || tag == "!!int" {
                    out.tag.clear();
                }
            }
            _ => {}
        },
        Kind::Alias => {}
    }
    out
}

impl<'a> Serializer<'a> {
    fn node(&mut self, id: NodeId, root: bool, tail: &[u8]) -> Result<(), Error> {
        let node = self.arena.get(id);
        let desolved = desolve(node.kind, &node.tag, &node.value, node.style);
        let tag = desolved.tag;
        let style = desolved.style;
        let force_quoting = tag.is_empty()
            && node.kind == Kind::Scalar
            && style & (SINGLE_QUOTED_STYLE | DOUBLE_QUOTED_STYLE | LITERAL_STYLE | FOLDED_STYLE)
                != 0;
        let head = if root {
            Vec::new()
        } else {
            node.head_comment.as_bytes().to_vec()
        };
        match node.kind {
            Kind::Sequence => {
                let mut event = Event::new(EventType::SequenceStart);
                event.anchor = node.anchor.as_bytes().to_vec();
                event.tag = long_tag(&tag).into_bytes();
                event.implicit = tag.is_empty();
                event.flow = style & FLOW_STYLE != 0;
                event.head_comment = head;
                self.emitter.emit(event)?;
                for child in &node.content {
                    self.node(*child, false, &[])?;
                }
                let mut event = Event::new(EventType::SequenceEnd);
                event.line_comment = node.line_comment.as_bytes().to_vec();
                event.foot_comment = node.foot_comment.as_bytes().to_vec();
                self.emitter.emit(event)?;
            }
            Kind::Mapping => {
                let mut event = Event::new(EventType::MappingStart);
                event.anchor = node.anchor.as_bytes().to_vec();
                event.tag = long_tag(&tag).into_bytes();
                event.implicit = tag.is_empty();
                event.flow = style & FLOW_STYLE != 0;
                event.tail_comment = tail.to_vec();
                event.head_comment = head;
                self.emitter.emit(event)?;
                let mut pending_tail: Vec<u8> = Vec::new();
                let mut index = 0;
                while index + 1 < node.content.len() {
                    let key = node.content[index];
                    let foot = self.arena.get(key).foot_comment.as_bytes().to_vec();
                    self.key_node(key, &pending_tail)?;
                    pending_tail = foot;
                    self.node(node.content[index + 1], false, &[])?;
                    index += 2;
                }
                let mut event = Event::new(EventType::MappingEnd);
                event.tail_comment = pending_tail;
                event.line_comment = node.line_comment.as_bytes().to_vec();
                event.foot_comment = node.foot_comment.as_bytes().to_vec();
                self.emitter.emit(event)?;
            }
            Kind::Alias => {
                let mut event = Event::new(EventType::Alias);
                event.anchor = node.value.as_bytes().to_vec();
                event.head_comment = head;
                event.line_comment = node.line_comment.as_bytes().to_vec();
                event.foot_comment = node.foot_comment.as_bytes().to_vec();
                self.emitter.emit(event)?;
            }
            Kind::Scalar => {
                let scalar_style = if style & DOUBLE_QUOTED_STYLE != 0 {
                    ScalarStyle::DoubleQuoted
                } else if style & SINGLE_QUOTED_STYLE != 0 {
                    ScalarStyle::SingleQuoted
                } else if style & LITERAL_STYLE != 0 {
                    ScalarStyle::Literal
                } else if style & FOLDED_STYLE != 0 {
                    ScalarStyle::Folded
                } else if node.value.contains('\n') {
                    ScalarStyle::Literal
                } else if force_quoting {
                    ScalarStyle::DoubleQuoted
                } else {
                    ScalarStyle::Plain
                };
                self.scalar(
                    &node.value,
                    &node.anchor,
                    &tag,
                    scalar_style,
                    head,
                    node.line_comment.as_bytes().to_vec(),
                    node.foot_comment.as_bytes().to_vec(),
                    tail.to_vec(),
                )?;
            }
        }
        Ok(())
    }

    /// A mapping key is serialised without its foot comment, which travels
    /// as the tail comment of the following key instead.
    fn key_node(&mut self, id: NodeId, tail: &[u8]) -> Result<(), Error> {
        let node = self.arena.get(id);
        if node.foot_comment.is_empty() {
            return self.node(id, false, tail);
        }
        let desolved = desolve(node.kind, &node.tag, &node.value, node.style);
        if node.kind != Kind::Scalar {
            return self.node(id, false, tail);
        }
        let style = desolved.style;
        let force_quoting = desolved.tag.is_empty()
            && style & (SINGLE_QUOTED_STYLE | DOUBLE_QUOTED_STYLE | LITERAL_STYLE | FOLDED_STYLE)
                != 0;
        let scalar_style = if style & DOUBLE_QUOTED_STYLE != 0 {
            ScalarStyle::DoubleQuoted
        } else if style & SINGLE_QUOTED_STYLE != 0 {
            ScalarStyle::SingleQuoted
        } else if style & LITERAL_STYLE != 0 {
            ScalarStyle::Literal
        } else if style & FOLDED_STYLE != 0 {
            ScalarStyle::Folded
        } else if node.value.contains('\n') {
            ScalarStyle::Literal
        } else if force_quoting {
            ScalarStyle::DoubleQuoted
        } else {
            ScalarStyle::Plain
        };
        self.scalar(
            &node.value,
            &node.anchor,
            &desolved.tag,
            scalar_style,
            node.head_comment.as_bytes().to_vec(),
            node.line_comment.as_bytes().to_vec(),
            Vec::new(),
            tail.to_vec(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn scalar(
        &mut self,
        value: &str,
        anchor: &str,
        tag: &str,
        style: ScalarStyle,
        head: Vec<u8>,
        line: Vec<u8>,
        foot: Vec<u8>,
        tail: Vec<u8>,
    ) -> Result<(), Error> {
        let implicit = tag.is_empty();
        let mut event = Event::new(EventType::Scalar);
        event.anchor = anchor.as_bytes().to_vec();
        if !implicit {
            event.tag = long_tag(tag).into_bytes();
        }
        event.value = value.as_bytes().to_vec();
        event.implicit = implicit;
        event.quoted_implicit = implicit;
        event.scalar_style = style;
        event.head_comment = head;
        event.line_comment = line;
        event.foot_comment = foot;
        event.tail_comment = tail;
        self.emitter.emit(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::decode::decode_documents;

    fn roundtrip(input: &str, indent: usize) -> String {
        let mut arena = Arena::new();
        let roots = decode_documents(&mut arena, input, "sample.yml", 0).unwrap();
        let prefs = YamlPrefs {
            indent,
            unwrap_scalar: true,
            print_doc_separators: true,
            compact_sequence_indent: false,
        };
        encode(&arena, roots[0], &prefs).unwrap()
    }

    #[test]
    fn block_mapping_with_nested_sequence() {
        assert_eq!(
            roundtrip("a:\n  - 1\n  - b: 2\n", 2),
            "a:\n  - 1\n  - b: 2\n"
        );
        assert_eq!(roundtrip("a:\n- 1\n", 4), "a:\n    - 1\n");
    }

    #[test]
    fn quoting_rules() {
        assert_eq!(roundtrip("a: \"1\"\n", 2), "a: \"1\"\n");
        assert_eq!(roundtrip("a: 'cat'\n", 2), "a: 'cat'\n");
        assert_eq!(roundtrip("a: !!str 1\n", 2), "a: !!str 1\n");
        assert_eq!(
            roundtrip("a: [1, 2]\nb: {c: d}\n", 2),
            "a: [1, 2]\nb: {c: d}\n"
        );
        assert_eq!(roundtrip("a: |\n  cat\n  dog\n", 2), "a: |\n  cat\n  dog\n");
        assert_eq!(roundtrip("a: &x 1\nb: *x\n", 2), "a: &x 1\nb: *x\n");
        assert_eq!(roundtrip("a: []\nb: {}\n", 2), "a: []\nb: {}\n");
        assert_eq!(roundtrip("a: null\nb: ~\n", 2), "a: null\nb: ~\n");
    }
}
