//! JSON text to arena nodes (`UnmarshalJSON` in `candidate_node_json.go`).

use crate::node::{Arena, Kind, Node, NodeId};
use crate::util::format_float;
use crate::Error;

/// Reads every JSON value in `input` as its own document.
pub fn decode_documents(arena: &mut Arena, input: &str) -> Result<Vec<NodeId>, Error> {
    let mut parser = JsonParser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    let mut roots = Vec::new();
    loop {
        parser.skip_whitespace();
        if parser.pos >= parser.bytes.len() {
            break;
        }
        let root = parser.value(arena, None)?;
        let node = arena.get_mut(root);
        node.document = roots.len();
        node.evaluate_together = true;
        roots.push(root);
    }
    Ok(roots)
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> JsonParser<'a> {
    fn error(&self, message: &str) -> Error {
        Error(format!("{message} at offset {}", self.pos))
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.bytes.len()
            && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\n' | b'\r')
        {
            self.pos += 1;
        }
    }

    fn value(&mut self, arena: &mut Arena, parent: Option<NodeId>) -> Result<NodeId, Error> {
        self.skip_whitespace();
        let first = match self.bytes.get(self.pos) {
            Some(b) => *b,
            None => return Err(Error("unexpected end of JSON input".to_string())),
        };
        match first {
            b'{' => self.object(arena, parent),
            b'[' => self.array(arena, parent),
            b'"' => {
                let text = self.string()?;
                Ok(arena.push(Node {
                    parent,
                    ..Node::scalar("!!str", &text)
                }))
            }
            b't' | b'f' => {
                let (literal, tag): (&[u8], &str) = if first == b't' {
                    (b"true", "!!bool")
                } else {
                    (b"false", "!!bool")
                };
                self.literal(literal)?;
                let value = std::str::from_utf8(literal).unwrap_or("");
                Ok(arena.push(Node {
                    parent,
                    ..Node::scalar(tag, value)
                }))
            }
            b'n' => {
                self.literal(b"null")?;
                Ok(arena.push(Node {
                    parent,
                    ..Node::scalar("!!null", "null")
                }))
            }
            b'-' | b'0'..=b'9' => {
                let (tag, value) = self.number()?;
                Ok(arena.push(Node {
                    parent,
                    ..Node::scalar(tag, &value)
                }))
            }
            _ => Err(self.error("invalid character looking for beginning of value")),
        }
    }

    fn literal(&mut self, literal: &[u8]) -> Result<(), Error> {
        if self.bytes[self.pos..].starts_with(literal) {
            self.pos += literal.len();
            Ok(())
        } else {
            Err(self.error("invalid literal"))
        }
    }

    fn number(&mut self) -> Result<(&'static str, String), Error> {
        let start = self.pos;
        if self.bytes[self.pos] == b'-' {
            self.pos += 1;
        }
        let mut is_float = false;
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b'0'..=b'9' => {}
                b'.' | b'e' | b'E' => is_float = true,
                b'+' | b'-' => {}
                _ => break,
            }
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| self.error("invalid number"))?;
        let parsed: f64 = text
            .parse()
            .map_err(|_| Error(format!("invalid number literal {text}")))?;
        // The Go decoder reads every number as float64 and calls it an int
        // when it has no fractional part.
        if parsed == (parsed as i64) as f64 && parsed.is_finite() {
            return Ok(("!!int", (parsed as i64).to_string()));
        }
        let _ = is_float;
        Ok(("!!float", format_float(parsed)))
    }

    fn string(&mut self) -> Result<String, Error> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let b = match self.bytes.get(self.pos) {
                Some(b) => *b,
                None => return Err(Error("unexpected end of JSON input".to_string())),
            };
            match b {
                b'"' => {
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.pos += 1;
                    let escaped = match self.bytes.get(self.pos) {
                        Some(b) => *b,
                        None => return Err(Error("unexpected end of JSON input".to_string())),
                    };
                    self.pos += 1;
                    match escaped {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut code = self.hex4()?;
                            if (0xD800..0xDC00).contains(&code)
                                && self.bytes[self.pos..].starts_with(b"\\u")
                            {
                                self.pos += 2;
                                let low = self.hex4()?;
                                if (0xDC00..0xE000).contains(&low) {
                                    code = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                } else {
                                    out.push('\u{FFFD}');
                                    code = low;
                                }
                            }
                            out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                        }
                        _ => return Err(self.error("invalid escape")),
                    }
                }
                _ => {
                    let width = utf8_width(b);
                    let end = (self.pos + width).min(self.bytes.len());
                    let text = std::str::from_utf8(&self.bytes[self.pos..end])
                        .map_err(|_| self.error("invalid UTF-8"))?;
                    out.push_str(text);
                    self.pos = end;
                }
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let end = self.pos + 4;
        if end > self.bytes.len() {
            return Err(self.error("invalid \\u escape"));
        }
        let text = std::str::from_utf8(&self.bytes[self.pos..end])
            .map_err(|_| self.error("invalid \\u escape"))?;
        let code = u32::from_str_radix(text, 16).map_err(|_| self.error("invalid \\u escape"))?;
        self.pos = end;
        Ok(code)
    }

    fn object(&mut self, arena: &mut Arena, parent: Option<NodeId>) -> Result<NodeId, Error> {
        let id = arena.push(Node {
            kind: Kind::Mapping,
            tag: "!!map".to_string(),
            parent,
            ..Node::default()
        });
        self.pos += 1;
        loop {
            self.skip_whitespace();
            match self.bytes.get(self.pos) {
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(id);
                }
                Some(b'"') => {}
                Some(_) => return Err(self.error("invalid character looking for object key")),
                None => return Err(Error("unexpected end of JSON input".to_string())),
            }
            let key_text = self.string()?;
            let key = arena.push(Node {
                parent: Some(id),
                is_map_key: true,
                ..Node::scalar("!!str", &key_text)
            });
            self.skip_whitespace();
            if self.bytes.get(self.pos) != Some(&b':') {
                return Err(self.error("invalid character after object key"));
            }
            self.pos += 1;
            let value = self.value(arena, Some(id))?;
            arena.get_mut(value).key = Some(key);
            let node = arena.get_mut(id);
            node.content.push(key);
            node.content.push(value);
            self.skip_whitespace();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b'}') => {}
                _ => return Err(self.error("invalid character after object value")),
            }
        }
    }

    fn array(&mut self, arena: &mut Arena, parent: Option<NodeId>) -> Result<NodeId, Error> {
        let id = arena.push(Node {
            kind: Kind::Sequence,
            tag: "!!seq".to_string(),
            parent,
            ..Node::default()
        });
        self.pos += 1;
        let mut index = 0;
        loop {
            self.skip_whitespace();
            if self.bytes.get(self.pos) == Some(&b']') {
                self.pos += 1;
                return Ok(id);
            }
            let value = self.value(arena, Some(id))?;
            let key = arena.push(Node {
                parent: Some(id),
                is_map_key: true,
                ..Node::scalar("!!int", &index.to_string())
            });
            arena.get_mut(value).key = Some(key);
            arena.get_mut(id).content.push(value);
            index += 1;
            self.skip_whitespace();
            match self.bytes.get(self.pos) {
                Some(b',') => self.pos += 1,
                Some(b']') => {}
                _ => return Err(self.error("invalid character after array element")),
            }
        }
    }
}

fn utf8_width(b: u8) -> usize {
    if b & 0x80 == 0 {
        1
    } else if b & 0xE0 == 0xC0 {
        2
    } else if b & 0xF0 == 0xE0 {
        3
    } else {
        4
    }
}
