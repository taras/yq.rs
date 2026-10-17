//! Nodes to YAML text: a port of go-yaml v4's `serializer.go`, `desolver.go`
//! and `emitter.go` with the options yq uses (`WithV3Defaults`, line width
//! unlimited, compact sequence indent off).

use crate::node::{Arena, Kind, NodeId};
use crate::node::{
    DOUBLE_QUOTED_STYLE, FLOW_STYLE, FOLDED_STYLE, LITERAL_STYLE, SINGLE_QUOTED_STYLE,
    TAGGED_STYLE,
};
use crate::yaml::resolve::resolve_plain;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventType {
    StreamStart,
    StreamEnd,
    DocumentStart,
    DocumentEnd,
    Alias,
    Scalar,
    SequenceStart,
    SequenceEnd,
    MappingStart,
    MappingEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScalarStyle {
    Plain,
    SingleQuoted,
    DoubleQuoted,
    Literal,
    Folded,
}

#[derive(Debug, Clone)]
struct Event {
    typ: EventType,
    anchor: Vec<u8>,
    tag: Vec<u8>,
    value: Vec<u8>,
    implicit: bool,
    quoted_implicit: bool,
    scalar_style: ScalarStyle,
    flow: bool,
    head_comment: Vec<u8>,
    line_comment: Vec<u8>,
    foot_comment: Vec<u8>,
    tail_comment: Vec<u8>,
}

impl Event {
    fn new(typ: EventType) -> Event {
        Event {
            typ,
            anchor: Vec::new(),
            tag: Vec::new(),
            value: Vec::new(),
            implicit: false,
            quoted_implicit: false,
            scalar_style: ScalarStyle::Plain,
            flow: false,
            head_comment: Vec::new(),
            line_comment: Vec::new(),
            foot_comment: Vec::new(),
            tail_comment: Vec::new(),
        }
    }
}

fn long_tag(tag: &str) -> String {
    match tag.strip_prefix("!!") {
        Some(suffix) => format!("tag:yaml.org,2002:{suffix}"),
        None => tag.to_string(),
    }
}

/// `desolveScalar`: decides which tag to emit for a scalar and whether the
/// string type needs quoting to survive a round trip.
fn desolve_scalar(tag: &str, value: &str, style: u32) -> (String, u32) {
    if style & TAGGED_STYLE != 0 || tag.is_empty() {
        return (tag.to_string(), style);
    }
    match tag {
        "!!binary" => return (tag.to_string(), style),
        "!!merge" => {
            if value == "<<" {
                return (String::new(), style);
            }
            return (tag.to_string(), style);
        }
        "!!null" | "!!bool" | "!!str" | "!!int" | "!!float" | "!!timestamp" => {}
        _ => return (tag.to_string(), style),
    }
    let resolved = resolve_plain(value);
    if resolved == tag {
        return (String::new(), style);
    }
    if tag == "!!str" {
        let mut new_style = style;
        if style & (SINGLE_QUOTED_STYLE | DOUBLE_QUOTED_STYLE | LITERAL_STYLE | FOLDED_STYLE) == 0
        {
            new_style |= DOUBLE_QUOTED_STYLE;
        }
        return (String::new(), new_style);
    }
    if tag == "!!float" || tag == "!!int" {
        return (String::new(), style);
    }
    (tag.to_string(), style)
}

fn desolve_collection(kind: Kind, tag: &str, style: u32) -> String {
    if style & TAGGED_STYLE != 0 {
        return tag.to_string();
    }
    match (kind, tag) {
        (Kind::Mapping, "!!map") | (Kind::Sequence, "!!seq") => String::new(),
        _ => tag.to_string(),
    }
}

struct Serializer<'a> {
    arena: &'a Arena,
    events: Vec<Event>,
}

impl<'a> Serializer<'a> {
    fn serialize_document(&mut self, root: NodeId) {
        let mut start = Event::new(EventType::DocumentStart);
        start.implicit = true;
        self.events.push(start);
        self.node(root, Vec::new());
        let mut end = Event::new(EventType::DocumentEnd);
        end.implicit = true;
        self.events.push(end);
    }

    fn node(&mut self, id: NodeId, tail: Vec<u8>) {
        let node = self.arena.get(id);
        match node.kind {
            Kind::Sequence => {
                let tag = desolve_collection(Kind::Sequence, &node.tag, node.style);
                let mut event = Event::new(EventType::SequenceStart);
                event.anchor = node.anchor.as_bytes().to_vec();
                event.tag = long_tag(&tag).into_bytes();
                event.implicit = tag.is_empty();
                event.flow = node.style & FLOW_STYLE != 0;
                event.head_comment = node.head_comment.as_bytes().to_vec();
                self.events.push(event);
                for child in &node.content {
                    self.node(*child, Vec::new());
                }
                let mut end = Event::new(EventType::SequenceEnd);
                end.line_comment = node.line_comment.as_bytes().to_vec();
                end.foot_comment = node.foot_comment.as_bytes().to_vec();
                self.events.push(end);
            }
            Kind::Mapping => {
                let tag = desolve_collection(Kind::Mapping, &node.tag, node.style);
                let mut event = Event::new(EventType::MappingStart);
                event.anchor = node.anchor.as_bytes().to_vec();
                event.tag = long_tag(&tag).into_bytes();
                event.implicit = tag.is_empty();
                event.flow = node.style & FLOW_STYLE != 0;
                event.tail_comment = tail;
                event.head_comment = node.head_comment.as_bytes().to_vec();
                self.events.push(event);
                let mut tail: Vec<u8> = Vec::new();
                let mut i = 0;
                while i + 1 < node.content.len() {
                    let key = node.content[i];
                    let foot = self.arena.get(key).foot_comment.as_bytes().to_vec();
                    self.node_with_foot(key, tail, true);
                    tail = foot;
                    self.node(node.content[i + 1], Vec::new());
                    i += 2;
                }
                let mut end = Event::new(EventType::MappingEnd);
                end.tail_comment = tail;
                end.line_comment = node.line_comment.as_bytes().to_vec();
                end.foot_comment = node.foot_comment.as_bytes().to_vec();
                self.events.push(end);
            }
            Kind::Alias => {
                let mut event = Event::new(EventType::Alias);
                event.anchor = node.value.as_bytes().to_vec();
                event.head_comment = node.head_comment.as_bytes().to_vec();
                event.line_comment = node.line_comment.as_bytes().to_vec();
                event.foot_comment = node.foot_comment.as_bytes().to_vec();
                self.events.push(event);
            }
            Kind::Scalar => self.scalar(id, tail, false),
        }
    }

    /// Mapping keys are serialised with their foot comment moved to the tail
    /// of the following key, so the key itself is emitted without one.
    fn node_with_foot(&mut self, id: NodeId, tail: Vec<u8>, strip_foot: bool) {
        if self.arena.get(id).kind == Kind::Scalar {
            self.scalar(id, tail, strip_foot);
        } else {
            self.node(id, tail);
        }
    }

    fn scalar(&mut self, id: NodeId, tail: Vec<u8>, strip_foot: bool) {
        let node = self.arena.get(id);
        let (tag, style) = desolve_scalar(&node.tag, &node.value, node.style);
        let force_quoting = tag.is_empty()
            && style & (SINGLE_QUOTED_STYLE | DOUBLE_QUOTED_STYLE | LITERAL_STYLE | FOLDED_STYLE)
                != 0;
        let value = node.value.as_str();
        let scalar_style = if style & DOUBLE_QUOTED_STYLE != 0 {
            ScalarStyle::DoubleQuoted
        } else if style & SINGLE_QUOTED_STYLE != 0 {
            ScalarStyle::SingleQuoted
        } else if style & LITERAL_STYLE != 0 {
            ScalarStyle::Literal
        } else if style & FOLDED_STYLE != 0 {
            ScalarStyle::Folded
        } else if value.contains('\n') {
            ScalarStyle::Literal
        } else if force_quoting {
            ScalarStyle::DoubleQuoted
        } else {
            ScalarStyle::Plain
        };
        let implicit = tag.is_empty();
        let mut event = Event::new(EventType::Scalar);
        event.anchor = node.anchor.as_bytes().to_vec();
        event.tag = if implicit {
            Vec::new()
        } else {
            long_tag(&tag).into_bytes()
        };
        event.value = value.as_bytes().to_vec();
        event.implicit = implicit;
        event.quoted_implicit = implicit;
        event.scalar_style = scalar_style;
        event.head_comment = node.head_comment.as_bytes().to_vec();
        event.line_comment = node.line_comment.as_bytes().to_vec();
        event.foot_comment = if strip_foot {
            Vec::new()
        } else {
            node.foot_comment.as_bytes().to_vec()
        };
        event.tail_comment = tail;
        self.events.push(event);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    StreamStart,
    FirstDocumentStart,
    DocumentStart,
    DocumentContent,
    DocumentEnd,
    FlowSequenceFirstItem,
    FlowSequenceTrailItem,
    FlowSequenceItem,
    FlowMappingFirstKey,
    FlowMappingTrailKey,
    FlowMappingKey,
    FlowMappingSimpleValue,
    FlowMappingValue,
    BlockSequenceFirstItem,
    BlockSequenceItem,
    BlockMappingFirstKey,
    BlockMappingKey,
    BlockMappingSimpleValue,
    BlockMappingValue,
    End,
}

#[derive(Default)]
struct ScalarData {
    value: Vec<u8>,
    multiline: bool,
    flow_plain_allowed: bool,
    block_plain_allowed: bool,
    single_quoted_allowed: bool,
    block_allowed: bool,
    style: Option<ScalarStyle>,
}

struct Emitter {
    out: Vec<u8>,
    best_indent: i64,
    best_width: i64,
    state: State,
    states: Vec<State>,
    events: Vec<Event>,
    head: usize,
    indents: Vec<i64>,
    indent: i64,
    flow_level: i64,
    root_context: bool,
    sequence_context: bool,
    mapping_context: bool,
    simple_key_context: bool,
    line: i64,
    column: i64,
    whitespace: bool,
    indention: bool,
    open_ended: bool,
    space_above: bool,
    foot_indent: i64,
    anchor: Vec<u8>,
    anchor_is_alias: bool,
    tag_handle: Vec<u8>,
    tag_suffix: Vec<u8>,
    scalar: ScalarData,
    head_comment: Vec<u8>,
    line_comment: Vec<u8>,
    foot_comment: Vec<u8>,
    tail_comment: Vec<u8>,
    key_line_comment: Vec<u8>,
}

type EmitResult = Result<(), String>;

fn width(b: u8) -> usize {
    if b & 0x80 == 0x00 {
        1
    } else if b & 0xE0 == 0xC0 {
        2
    } else if b & 0xF0 == 0xE0 {
        3
    } else if b & 0xF8 == 0xF0 {
        4
    } else {
        1
    }
}

fn at(b: &[u8], i: usize) -> u8 {
    b.get(i).copied().unwrap_or(0)
}

fn is_alpha(b: &[u8], i: usize) -> bool {
    let c = at(b, i);
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

fn is_printable(b: &[u8], i: usize) -> bool {
    let c = at(b, i);
    (c == 0x0A)
        || (0x20..=0x7E).contains(&c)
        || (c == 0xC2 && at(b, i + 1) >= 0xA0)
        || (c > 0xC2 && c < 0xED)
        || (c == 0xED && at(b, i + 1) < 0xA0)
        || (c == 0xEE)
        || (c == 0xEF
            && !(at(b, i + 1) == 0xBB && at(b, i + 2) == 0xBF)
            && !(at(b, i + 1) == 0xBF && (at(b, i + 2) == 0xBE || at(b, i + 2) == 0xBF)))
}

fn is_ascii(b: &[u8], i: usize) -> bool {
    at(b, i) <= 0x7F
}

fn is_bom(b: &[u8]) -> bool {
    b.len() >= 3 && b[0] == 0xEF && b[1] == 0xBB && b[2] == 0xBF
}

fn is_space(b: &[u8], i: usize) -> bool {
    at(b, i) == b' '
}

fn is_blank(b: &[u8], i: usize) -> bool {
    let c = at(b, i);
    c == b' ' || c == b'\t'
}

fn is_line_break(b: &[u8], i: usize) -> bool {
    let c = at(b, i);
    c == b'\r'
        || c == b'\n'
        || (c == 0xC2 && at(b, i + 1) == 0x85)
        || (c == 0xE2 && at(b, i + 1) == 0x80 && at(b, i + 2) == 0xA8)
        || (c == 0xE2 && at(b, i + 1) == 0x80 && at(b, i + 2) == 0xA9)
}

fn is_blank_or_zero(b: &[u8], i: usize) -> bool {
    is_blank(b, i) || is_line_break(b, i) || at(b, i) == 0
}

impl Emitter {
    fn new(events: Vec<Event>, best_indent: i64) -> Emitter {
        Emitter {
            out: Vec::new(),
            best_indent,
            best_width: -1,
            state: State::StreamStart,
            states: Vec::new(),
            events,
            head: 0,
            indents: Vec::new(),
            indent: -1,
            flow_level: 0,
            root_context: false,
            sequence_context: false,
            mapping_context: false,
            simple_key_context: false,
            line: 0,
            column: 0,
            whitespace: true,
            indention: true,
            open_ended: false,
            space_above: true,
            foot_indent: -1,
            anchor: Vec::new(),
            anchor_is_alias: false,
            tag_handle: Vec::new(),
            tag_suffix: Vec::new(),
            scalar: ScalarData::default(),
            head_comment: Vec::new(),
            line_comment: Vec::new(),
            foot_comment: Vec::new(),
            tail_comment: Vec::new(),
            key_line_comment: Vec::new(),
        }
    }

    fn run(&mut self) -> EmitResult {
        while self.head < self.events.len() {
            let event = self.events[self.head].clone();
            self.analyze_event(&event)?;
            self.state_machine(&event)?;
            self.head += 1;
        }
        Ok(())
    }

    fn current_type(&self, offset: usize) -> Option<EventType> {
        self.events.get(self.head + offset).map(|e| e.typ)
    }

    fn state_machine(&mut self, event: &Event) -> EmitResult {
        match self.state {
            State::StreamStart => self.emit_stream_start(event),
            State::FirstDocumentStart => self.emit_document_start(event, true),
            State::DocumentStart => self.emit_document_start(event, false),
            State::DocumentContent => self.emit_document_content(event),
            State::DocumentEnd => self.emit_document_end(event),
            State::FlowSequenceFirstItem => self.emit_flow_sequence_item(event, true, false),
            State::FlowSequenceTrailItem => self.emit_flow_sequence_item(event, false, true),
            State::FlowSequenceItem => self.emit_flow_sequence_item(event, false, false),
            State::FlowMappingFirstKey => self.emit_flow_mapping_key(event, true, false),
            State::FlowMappingTrailKey => self.emit_flow_mapping_key(event, false, true),
            State::FlowMappingKey => self.emit_flow_mapping_key(event, false, false),
            State::FlowMappingSimpleValue => self.emit_flow_mapping_value(event, true),
            State::FlowMappingValue => self.emit_flow_mapping_value(event, false),
            State::BlockSequenceFirstItem => self.emit_block_sequence_item(event, true),
            State::BlockSequenceItem => self.emit_block_sequence_item(event, false),
            State::BlockMappingFirstKey => self.emit_block_mapping_key(event, true),
            State::BlockMappingKey => self.emit_block_mapping_key(event, false),
            State::BlockMappingSimpleValue => self.emit_block_mapping_value(event, true),
            State::BlockMappingValue => self.emit_block_mapping_value(event, false),
            State::End => Err("expected nothing after STREAM-END".to_string()),
        }
    }

    fn pop_state(&mut self) {
        self.state = self.states.pop().unwrap_or(State::End);
    }

    fn pop_indent(&mut self) {
        self.indent = self.indents.pop().unwrap_or(-1);
    }

    fn emit_stream_start(&mut self, event: &Event) -> EmitResult {
        if event.typ != EventType::StreamStart {
            return Err("expected STREAM-START".to_string());
        }
        if self.best_indent < 2 || self.best_indent > 9 {
            self.best_indent = 2;
        }
        if self.best_width >= 0 && self.best_width <= self.best_indent * 2 {
            self.best_width = 80;
        }
        if self.best_width < 0 {
            self.best_width = i64::MAX;
        }
        self.indent = -1;
        self.line = 0;
        self.column = 0;
        self.whitespace = true;
        self.indention = true;
        self.space_above = true;
        self.foot_indent = -1;
        self.state = State::FirstDocumentStart;
        Ok(())
    }

    fn emit_document_start(&mut self, event: &Event, first: bool) -> EmitResult {
        if event.typ == EventType::DocumentStart {
            let mut implicit = event.implicit;
            if !first {
                implicit = false;
            }
            if !implicit {
                self.write_indent()?;
                self.write_indicator(b"---", true, false, false)?;
                self.write_indent()?;
            }
            if !self.head_comment.is_empty() {
                self.process_head_comment()?;
                self.put_line_break();
            }
            self.state = State::DocumentContent;
            return Ok(());
        }
        if event.typ == EventType::StreamEnd {
            if self.open_ended {
                self.write_indicator(b"...", true, false, false)?;
                self.write_indent()?;
            }
            self.state = State::End;
            return Ok(());
        }
        Err("expected DOCUMENT-START or STREAM-END".to_string())
    }

    fn emit_document_content(&mut self, event: &Event) -> EmitResult {
        self.states.push(State::DocumentEnd);
        self.process_head_comment()?;
        self.emit_node(event, true, false, false, false)?;
        self.process_line_comment()?;
        self.process_foot_comment()
    }

    fn emit_document_end(&mut self, event: &Event) -> EmitResult {
        if event.typ != EventType::DocumentEnd {
            return Err("expected DOCUMENT-END".to_string());
        }
        self.foot_indent = 0;
        self.process_foot_comment()?;
        self.foot_indent = -1;
        self.write_indent()?;
        if !event.implicit {
            self.write_indicator(b"...", true, false, false)?;
            self.write_indent()?;
        }
        self.state = State::DocumentStart;
        Ok(())
    }

    fn has_trailing_comments(&self) -> bool {
        !self.line_comment.is_empty()
            || !self.foot_comment.is_empty()
            || !self.tail_comment.is_empty()
    }

    fn emit_flow_sequence_item(&mut self, event: &Event, first: bool, trail: bool) -> EmitResult {
        if first {
            self.write_indicator(b"[", true, true, false)?;
            self.increase_indent(true, false);
            self.flow_level += 1;
        }
        if event.typ == EventType::SequenceEnd {
            self.flow_level -= 1;
            self.pop_indent();
            if self.column == 0 {
                self.write_indent()?;
            }
            self.write_indicator(b"]", false, false, false)?;
            self.process_line_comment()?;
            self.process_foot_comment()?;
            self.pop_state();
            return Ok(());
        }
        if !first && !trail {
            self.write_indicator(b",", false, false, false)?;
        }
        self.process_head_comment()?;
        if self.column == 0 {
            self.write_indent()?;
        }
        if self.column > self.best_width {
            self.write_indent()?;
        }
        if self.has_trailing_comments() {
            self.states.push(State::FlowSequenceTrailItem);
        } else {
            self.states.push(State::FlowSequenceItem);
        }
        self.emit_node(event, false, true, false, false)?;
        if self.has_trailing_comments() {
            self.write_indicator(b",", false, false, false)?;
        }
        self.process_line_comment()?;
        self.process_foot_comment()
    }

    fn emit_flow_mapping_key(&mut self, event: &Event, first: bool, trail: bool) -> EmitResult {
        if first {
            self.write_indicator(b"{", true, true, false)?;
            self.increase_indent(true, false);
            self.flow_level += 1;
        }
        if event.typ == EventType::MappingEnd {
            let has_comments = !self.head_comment.is_empty()
                || !self.foot_comment.is_empty()
                || !self.tail_comment.is_empty();
            if has_comments && !first && !trail {
                self.write_indicator(b",", false, false, false)?;
            }
            self.process_head_comment()?;
            self.flow_level -= 1;
            self.pop_indent();
            self.write_indicator(b"}", false, false, false)?;
            self.process_line_comment()?;
            self.process_foot_comment()?;
            self.pop_state();
            return Ok(());
        }
        if !first && !trail {
            self.write_indicator(b",", false, false, false)?;
        }
        self.process_head_comment()?;
        if self.column == 0 {
            self.write_indent()?;
        }
        if self.column > self.best_width {
            self.write_indent()?;
        }
        if self.check_simple_key() {
            self.states.push(State::FlowMappingSimpleValue);
            return self.emit_node(event, false, false, true, true);
        }
        self.write_indicator(b"?", true, false, false)?;
        self.states.push(State::FlowMappingValue);
        self.emit_node(event, false, false, true, false)
    }

    fn emit_flow_mapping_value(&mut self, event: &Event, simple: bool) -> EmitResult {
        if simple {
            self.write_indicator(b":", false, false, false)?;
        } else {
            if self.column > self.best_width {
                self.write_indent()?;
            }
            self.write_indicator(b":", true, false, false)?;
        }
        if self.has_trailing_comments() {
            self.states.push(State::FlowMappingTrailKey);
        } else {
            self.states.push(State::FlowMappingKey);
        }
        self.emit_node(event, false, false, true, false)?;
        if self.has_trailing_comments() {
            self.write_indicator(b",", false, false, false)?;
        }
        self.process_line_comment()?;
        self.process_foot_comment()
    }

    fn emit_block_sequence_item(&mut self, event: &Event, first: bool) -> EmitResult {
        if first {
            self.increase_indent(false, false);
        }
        if event.typ == EventType::SequenceEnd {
            self.pop_indent();
            self.pop_state();
            return Ok(());
        }
        self.process_head_comment()?;
        self.write_indent()?;
        self.write_indicator(b"-", true, false, true)?;
        self.states.push(State::BlockSequenceItem);
        self.emit_node(event, false, true, false, false)?;
        self.process_line_comment()?;
        self.process_foot_comment()
    }

    fn emit_block_mapping_key(&mut self, event: &Event, first: bool) -> EmitResult {
        if first {
            self.increase_indent(false, false);
        }
        self.process_head_comment()?;
        if event.typ == EventType::MappingEnd {
            self.pop_indent();
            self.pop_state();
            return Ok(());
        }
        self.write_indent()?;
        if !self.line_comment.is_empty() {
            self.key_line_comment = std::mem::take(&mut self.line_comment);
        }
        if self.check_simple_key() {
            self.states.push(State::BlockMappingSimpleValue);
            self.emit_node(event, false, false, true, true)?;
            if event.typ == EventType::Alias {
                self.put(b' ');
            }
            return Ok(());
        }
        self.write_indicator(b"?", true, false, true)?;
        self.states.push(State::BlockMappingValue);
        self.emit_node(event, false, false, true, false)
    }

    fn emit_block_mapping_value(&mut self, event: &Event, simple: bool) -> EmitResult {
        if simple {
            self.write_indicator(b":", false, false, false)?;
        } else {
            self.write_indent()?;
            self.write_indicator(b":", true, false, true)?;
        }
        if !self.key_line_comment.is_empty() {
            if event.typ == EventType::Scalar {
                if self.line_comment.is_empty() {
                    self.line_comment = std::mem::take(&mut self.key_line_comment);
                }
            } else if !event.flow
                && (event.typ == EventType::MappingStart || event.typ == EventType::SequenceStart)
            {
                std::mem::swap(&mut self.line_comment, &mut self.key_line_comment);
                self.process_line_comment()?;
                std::mem::swap(&mut self.line_comment, &mut self.key_line_comment);
            }
        }
        self.states.push(State::BlockMappingKey);
        self.emit_node(event, false, false, true, false)?;
        self.process_line_comment()?;
        self.process_foot_comment()
    }

    fn emit_node(
        &mut self,
        event: &Event,
        root: bool,
        sequence: bool,
        mapping: bool,
        simple_key: bool,
    ) -> EmitResult {
        self.root_context = root;
        self.sequence_context = sequence;
        self.mapping_context = mapping;
        self.simple_key_context = simple_key;
        match event.typ {
            EventType::Alias => self.emit_alias(),
            EventType::Scalar => self.emit_scalar(event),
            EventType::SequenceStart => self.emit_sequence_start(event),
            EventType::MappingStart => self.emit_mapping_start(event),
            other => Err(format!(
                "expected SCALAR, SEQUENCE-START, MAPPING-START, or ALIAS, but got {other:?}"
            )),
        }
    }

    fn emit_alias(&mut self) -> EmitResult {
        self.process_anchor()?;
        self.pop_state();
        Ok(())
    }

    fn emit_scalar(&mut self, event: &Event) -> EmitResult {
        self.select_scalar_style(event)?;
        self.process_anchor()?;
        self.process_tag()?;
        self.increase_indent(true, false);
        self.process_scalar()?;
        self.pop_indent();
        self.pop_state();
        Ok(())
    }

    fn emit_sequence_start(&mut self, event: &Event) -> EmitResult {
        self.process_anchor()?;
        self.process_tag()?;
        if self.flow_level > 0 || event.flow || self.check_empty_sequence() {
            self.state = State::FlowSequenceFirstItem;
        } else {
            self.state = State::BlockSequenceFirstItem;
        }
        Ok(())
    }

    fn emit_mapping_start(&mut self, event: &Event) -> EmitResult {
        self.process_anchor()?;
        self.process_tag()?;
        if self.flow_level > 0 || event.flow || self.check_empty_mapping() {
            self.state = State::FlowMappingFirstKey;
        } else {
            self.state = State::BlockMappingFirstKey;
        }
        Ok(())
    }

    fn check_empty_sequence(&self) -> bool {
        self.current_type(0) == Some(EventType::SequenceStart)
            && self.current_type(1) == Some(EventType::SequenceEnd)
    }

    fn check_empty_mapping(&self) -> bool {
        self.current_type(0) == Some(EventType::MappingStart)
            && self.current_type(1) == Some(EventType::MappingEnd)
    }

    fn check_simple_key(&self) -> bool {
        let mut length = 0;
        match self.current_type(0) {
            Some(EventType::Alias) => length += self.anchor.len(),
            Some(EventType::Scalar) => {
                if self.scalar.multiline {
                    return false;
                }
                length += self.anchor.len()
                    + self.tag_handle.len()
                    + self.tag_suffix.len()
                    + self.scalar.value.len();
            }
            Some(EventType::SequenceStart) => {
                if !self.check_empty_sequence() {
                    return false;
                }
                length += self.anchor.len() + self.tag_handle.len() + self.tag_suffix.len();
            }
            Some(EventType::MappingStart) => {
                if !self.check_empty_mapping() {
                    return false;
                }
                length += self.anchor.len() + self.tag_handle.len() + self.tag_suffix.len();
            }
            _ => return false,
        }
        length <= 128
    }

    fn process_anchor(&mut self) -> EmitResult {
        if self.anchor.is_empty() {
            return Ok(());
        }
        let indicator: &[u8] = if self.anchor_is_alias { b"*" } else { b"&" };
        self.write_indicator(indicator, true, false, false)?;
        let anchor = self.anchor.clone();
        self.write_all(&anchor);
        self.whitespace = false;
        self.indention = false;
        Ok(())
    }

    fn process_tag(&mut self) -> EmitResult {
        if self.tag_handle.is_empty() && self.tag_suffix.is_empty() {
            return Ok(());
        }
        if !self.tag_handle.is_empty() {
            let handle = self.tag_handle.clone();
            self.write_tag_handle(&handle);
            if !self.tag_suffix.is_empty() {
                let suffix = self.tag_suffix.clone();
                self.write_tag_content(&suffix, false);
            }
        } else {
            self.write_indicator(b"!<", true, false, false)?;
            let suffix = self.tag_suffix.clone();
            self.write_tag_content(&suffix, false);
            self.write_indicator(b">", false, false, false)?;
        }
        Ok(())
    }

    fn process_scalar(&mut self) -> EmitResult {
        let value = std::mem::take(&mut self.scalar.value);
        let allow_breaks = !self.simple_key_context;
        let result = match self.scalar.style.unwrap_or(ScalarStyle::Plain) {
            ScalarStyle::Plain => self.write_plain_scalar(&value, allow_breaks),
            ScalarStyle::SingleQuoted => self.write_single_quoted_scalar(&value, allow_breaks),
            ScalarStyle::DoubleQuoted => self.write_double_quoted_scalar(&value, allow_breaks),
            ScalarStyle::Literal => self.write_literal_scalar(&value),
            ScalarStyle::Folded => self.write_folded_scalar(&value),
        };
        self.scalar.value = value;
        result
    }

    fn process_head_comment(&mut self) -> EmitResult {
        if !self.tail_comment.is_empty() {
            self.write_indent()?;
            let tail = std::mem::take(&mut self.tail_comment);
            self.write_comment(&tail)?;
            self.foot_indent = self.indent.max(0);
        }
        if self.head_comment.is_empty() {
            return Ok(());
        }
        self.write_indent()?;
        let head = std::mem::take(&mut self.head_comment);
        self.write_comment(&head)
    }

    fn process_line_comment(&mut self) -> EmitResult {
        self.process_line_comment_linebreak(false)
    }

    fn process_line_comment_linebreak(&mut self, linebreak: bool) -> EmitResult {
        if self.line_comment.is_empty() {
            if linebreak {
                self.put_line_break();
            }
            return Ok(());
        }
        if !self.whitespace {
            self.put(b' ');
        }
        let comment = std::mem::take(&mut self.line_comment);
        self.write_comment(&comment)
    }

    fn process_foot_comment(&mut self) -> EmitResult {
        if self.foot_comment.is_empty() {
            return Ok(());
        }
        self.write_indent()?;
        let foot = std::mem::take(&mut self.foot_comment);
        self.write_comment(&foot)?;
        self.foot_indent = self.indent.max(0);
        Ok(())
    }

    fn analyze_tag(&mut self, tag: &[u8]) -> EmitResult {
        if tag.is_empty() {
            return Err("tag value must not be empty".to_string());
        }
        let directives: [(&[u8], &[u8]); 2] = [(b"!", b"!"), (b"!!", b"tag:yaml.org,2002:")];
        for (handle, prefix) in directives {
            if tag.starts_with(prefix) {
                self.tag_handle = handle.to_vec();
                self.tag_suffix = tag[prefix.len()..].to_vec();
                return Ok(());
            }
        }
        self.tag_suffix = tag.to_vec();
        Ok(())
    }

    fn analyze_scalar(&mut self, value: &[u8]) {
        self.scalar.value = value.to_vec();
        if value.is_empty() {
            self.scalar.multiline = false;
            self.scalar.flow_plain_allowed = false;
            self.scalar.block_plain_allowed = true;
            self.scalar.single_quoted_allowed = true;
            self.scalar.block_allowed = false;
            return;
        }
        let mut block_indicators = false;
        let mut flow_indicators = false;
        let mut line_breaks = false;
        let mut special_characters = false;
        let mut tab_characters = false;
        let mut leading_space = false;
        let mut leading_break = false;
        let mut trailing_space = false;
        let mut trailing_break = false;
        let mut break_space = false;
        let mut space_break = false;
        let mut previous_space = false;
        let mut previous_break = false;

        if value.len() >= 3
            && ((value[0] == b'-' && value[1] == b'-' && value[2] == b'-')
                || (value[0] == b'.' && value[1] == b'.' && value[2] == b'.'))
        {
            block_indicators = true;
            flow_indicators = true;
        }
        let mut preceded_by_whitespace = true;
        let mut i = 0;
        while i < value.len() {
            let w = width(value[i]);
            let followed_by_whitespace = i + w >= value.len() || is_blank(value, i + w);
            if i == 0 {
                match value[i] {
                    b'#' | b',' | b'[' | b']' | b'{' | b'}' | b'&' | b'*' | b'!' | b'|' | b'>'
                    | b'\'' | b'"' | b'%' | b'@' | b'`' => {
                        flow_indicators = true;
                        block_indicators = true;
                    }
                    b'?' | b':' => {
                        flow_indicators = true;
                        if followed_by_whitespace {
                            block_indicators = true;
                        }
                    }
                    b'-' => {
                        if followed_by_whitespace {
                            flow_indicators = true;
                            block_indicators = true;
                        }
                    }
                    _ => {}
                }
            } else {
                match value[i] {
                    b',' | b'?' | b'[' | b']' | b'{' | b'}' => flow_indicators = true,
                    b':' => {
                        flow_indicators = true;
                        if followed_by_whitespace {
                            block_indicators = true;
                        }
                    }
                    b'#' => {
                        if preceded_by_whitespace {
                            flow_indicators = true;
                            block_indicators = true;
                        }
                    }
                    _ => {}
                }
            }
            if value[i] == b'\t' {
                tab_characters = true;
            } else if !is_printable(value, i) {
                special_characters = true;
            }
            if is_space(value, i) {
                if i == 0 {
                    leading_space = true;
                }
                if i + w == value.len() {
                    trailing_space = true;
                }
                if previous_break {
                    break_space = true;
                }
                previous_space = true;
                previous_break = false;
            } else if is_line_break(value, i) {
                line_breaks = true;
                if i == 0 {
                    leading_break = true;
                }
                if i + w == value.len() {
                    trailing_break = true;
                }
                if previous_space {
                    space_break = true;
                }
                previous_space = false;
                previous_break = true;
            } else {
                previous_space = false;
                previous_break = false;
            }
            preceded_by_whitespace = is_blank_or_zero(value, i);
            i += w;
        }
        self.scalar.multiline = line_breaks;
        self.scalar.flow_plain_allowed = true;
        self.scalar.block_plain_allowed = true;
        self.scalar.single_quoted_allowed = true;
        self.scalar.block_allowed = true;
        if leading_space || leading_break || trailing_space || trailing_break {
            self.scalar.flow_plain_allowed = false;
            self.scalar.block_plain_allowed = false;
        }
        if trailing_space {
            self.scalar.block_allowed = false;
        }
        if break_space {
            self.scalar.flow_plain_allowed = false;
            self.scalar.block_plain_allowed = false;
            self.scalar.single_quoted_allowed = false;
        }
        if space_break || tab_characters || special_characters {
            self.scalar.flow_plain_allowed = false;
            self.scalar.block_plain_allowed = false;
            self.scalar.single_quoted_allowed = false;
        }
        if space_break || special_characters {
            self.scalar.block_allowed = false;
        }
        if line_breaks {
            self.scalar.flow_plain_allowed = false;
            self.scalar.block_plain_allowed = false;
        }
        if flow_indicators {
            self.scalar.flow_plain_allowed = false;
        }
        if block_indicators {
            self.scalar.block_plain_allowed = false;
        }
    }

    fn analyze_event(&mut self, event: &Event) -> EmitResult {
        self.anchor = Vec::new();
        self.anchor_is_alias = false;
        self.tag_handle = Vec::new();
        self.tag_suffix = Vec::new();
        self.scalar.value = Vec::new();
        if !event.head_comment.is_empty() {
            self.head_comment = event.head_comment.clone();
        }
        if !event.line_comment.is_empty() {
            self.line_comment = event.line_comment.clone();
        }
        if !event.foot_comment.is_empty() {
            self.foot_comment = event.foot_comment.clone();
        }
        if !event.tail_comment.is_empty() {
            self.tail_comment = event.tail_comment.clone();
        }
        match event.typ {
            EventType::Alias => {
                self.anchor = event.anchor.clone();
                self.anchor_is_alias = true;
            }
            EventType::Scalar => {
                if !event.anchor.is_empty() {
                    self.anchor = event.anchor.clone();
                }
                if !event.tag.is_empty() && !event.implicit && !event.quoted_implicit {
                    self.analyze_tag(&event.tag)?;
                }
                self.analyze_scalar(&event.value);
            }
            EventType::SequenceStart | EventType::MappingStart => {
                if !event.anchor.is_empty() {
                    self.anchor = event.anchor.clone();
                }
                if !event.tag.is_empty() && !event.implicit {
                    self.analyze_tag(&event.tag)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn select_scalar_style(&mut self, event: &Event) -> EmitResult {
        let no_tag = self.tag_handle.is_empty() && self.tag_suffix.is_empty();
        if no_tag && !event.implicit && !event.quoted_implicit {
            return Err("neither tag nor implicit flags are specified".to_string());
        }
        let mut style = event.scalar_style;
        if self.simple_key_context && self.scalar.multiline {
            style = ScalarStyle::DoubleQuoted;
        }
        if style == ScalarStyle::Plain {
            if (self.flow_level > 0 && !self.scalar.flow_plain_allowed)
                || (self.flow_level == 0 && !self.scalar.block_plain_allowed)
            {
                style = ScalarStyle::SingleQuoted;
            }
            if self.scalar.value.is_empty() && (self.flow_level > 0 || self.simple_key_context) {
                style = ScalarStyle::SingleQuoted;
            }
            if no_tag && !event.implicit {
                style = ScalarStyle::SingleQuoted;
            }
        }
        if style == ScalarStyle::SingleQuoted && !self.scalar.single_quoted_allowed {
            style = ScalarStyle::DoubleQuoted;
        }
        if (style == ScalarStyle::Literal || style == ScalarStyle::Folded)
            && (!self.scalar.block_allowed || self.flow_level > 0 || self.simple_key_context)
        {
            style = ScalarStyle::DoubleQuoted;
        }
        if no_tag && !event.quoted_implicit && style != ScalarStyle::Plain {
            self.tag_handle = b"!".to_vec();
        }
        self.scalar.style = Some(style);
        Ok(())
    }

    fn write_indent(&mut self) -> EmitResult {
        let indent = self.indent.max(0);
        if !self.indention || self.column > indent || (self.column == indent && !self.whitespace) {
            self.put_line_break();
        }
        if self.foot_indent == indent {
            self.put_line_break();
        }
        while self.column < indent {
            self.put(b' ');
        }
        self.whitespace = true;
        self.space_above = false;
        self.foot_indent = -1;
        Ok(())
    }

    fn write_indicator(
        &mut self,
        indicator: &[u8],
        need_whitespace: bool,
        is_whitespace: bool,
        is_indention: bool,
    ) -> EmitResult {
        if need_whitespace && !self.whitespace {
            self.put(b' ');
        }
        self.write_all(indicator);
        self.whitespace = is_whitespace;
        self.indention = self.indention && is_indention;
        self.open_ended = false;
        Ok(())
    }

    fn write_tag_handle(&mut self, value: &[u8]) {
        if !self.whitespace {
            self.put(b' ');
        }
        self.write_all(value);
        self.whitespace = false;
        self.indention = false;
    }

    fn write_tag_content(&mut self, value: &[u8], need_whitespace: bool) {
        if need_whitespace && !self.whitespace {
            self.put(b' ');
        }
        let mut i = 0;
        while i < value.len() {
            let must_write = match value[i] {
                b';' | b'/' | b'?' | b':' | b'@' | b'&' | b'=' | b'+' | b'$' | b',' | b'_'
                | b'.' | b'~' | b'*' | b'\'' | b'(' | b')' | b'[' | b']' => true,
                _ => is_alpha(value, i),
            };
            if must_write {
                self.write(value, &mut i);
            } else {
                let w = width(value[i]);
                for _ in 0..w {
                    let octet = at(value, i);
                    i += 1;
                    self.put(b'%');
                    let hi = octet >> 4;
                    self.put(if hi < 10 { hi + b'0' } else { hi + b'A' - 10 });
                    let lo = octet & 0x0f;
                    self.put(if lo < 10 { lo + b'0' } else { lo + b'A' - 10 });
                }
            }
        }
        self.whitespace = false;
        self.indention = false;
    }

    fn write_plain_scalar(&mut self, value: &[u8], allow_breaks: bool) -> EmitResult {
        if !value.is_empty() && !self.whitespace {
            self.put(b' ');
        }
        let mut spaces = false;
        let mut breaks = false;
        let mut i = 0;
        while i < value.len() {
            if is_space(value, i) {
                if allow_breaks
                    && !spaces
                    && self.column > self.best_width
                    && !is_space(value, i + 1)
                {
                    self.write_indent()?;
                    i += width(value[i]);
                } else {
                    self.write(value, &mut i);
                }
                spaces = true;
            } else if is_line_break(value, i) {
                if !breaks && value[i] == b'\n' {
                    self.put_line_break();
                }
                self.write_line_break(value, &mut i);
                breaks = true;
            } else {
                if breaks {
                    self.write_indent()?;
                }
                self.write(value, &mut i);
                self.indention = false;
                spaces = false;
                breaks = false;
            }
        }
        if !value.is_empty() {
            self.whitespace = false;
        }
        self.indention = false;
        if self.root_context {
            self.open_ended = true;
        }
        Ok(())
    }

    fn write_single_quoted_scalar(&mut self, value: &[u8], allow_breaks: bool) -> EmitResult {
        self.write_indicator(b"'", true, false, false)?;
        let mut spaces = false;
        let mut breaks = false;
        let mut i = 0;
        while i < value.len() {
            if is_space(value, i) {
                if allow_breaks
                    && !spaces
                    && self.column > self.best_width
                    && i > 0
                    && i < value.len() - 1
                    && !is_space(value, i + 1)
                {
                    self.write_indent()?;
                    i += width(value[i]);
                } else {
                    self.write(value, &mut i);
                }
                spaces = true;
            } else if is_line_break(value, i) {
                if !breaks && value[i] == b'\n' {
                    self.put_line_break();
                }
                self.write_line_break(value, &mut i);
                breaks = true;
            } else {
                if breaks {
                    self.write_indent()?;
                }
                if value[i] == b'\'' {
                    self.put(b'\'');
                }
                self.write(value, &mut i);
                self.indention = false;
                spaces = false;
                breaks = false;
            }
        }
        self.write_indicator(b"'", false, false, false)?;
        self.whitespace = false;
        self.indention = false;
        Ok(())
    }

    fn write_double_quoted_scalar(&mut self, value: &[u8], allow_breaks: bool) -> EmitResult {
        let mut spaces = false;
        self.write_indicator(b"\"", true, false, false)?;
        let bom = is_bom(value);
        let mut i = 0;
        while i < value.len() {
            if !is_printable(value, i)
                || bom
                || is_line_break(value, i)
                || value[i] == b'"'
                || value[i] == b'\\'
            {
                let octet = value[i];
                let (w, mut v): (usize, u32) = if octet & 0x80 == 0x00 {
                    (1, (octet & 0x7F) as u32)
                } else if octet & 0xE0 == 0xC0 {
                    (2, (octet & 0x1F) as u32)
                } else if octet & 0xF0 == 0xE0 {
                    (3, (octet & 0x0F) as u32)
                } else {
                    (4, (octet & 0x07) as u32)
                };
                for k in 1..w {
                    v = (v << 6) + (at(value, i + k) as u32 & 0x3F);
                }
                i += w;
                self.put(b'\\');
                match v {
                    0x00 => self.put(b'0'),
                    0x07 => self.put(b'a'),
                    0x08 => self.put(b'b'),
                    0x09 => self.put(b't'),
                    0x0A => self.put(b'n'),
                    0x0b => self.put(b'v'),
                    0x0c => self.put(b'f'),
                    0x0d => self.put(b'r'),
                    0x1b => self.put(b'e'),
                    0x22 => self.put(b'"'),
                    0x5c => self.put(b'\\'),
                    0x85 => self.put(b'N'),
                    0xA0 => self.put(b'_'),
                    0x2028 => self.put(b'L'),
                    0x2029 => self.put(b'P'),
                    _ => {
                        let digits = if v <= 0xFF {
                            self.put(b'x');
                            2
                        } else if v <= 0xFFFF {
                            self.put(b'u');
                            4
                        } else {
                            self.put(b'U');
                            8
                        };
                        let mut k = (digits - 1) * 4;
                        loop {
                            let digit = ((v >> k) & 0x0F) as u8;
                            self.put(if digit < 10 {
                                digit + b'0'
                            } else {
                                digit + b'A' - 10
                            });
                            if k == 0 {
                                break;
                            }
                            k -= 4;
                        }
                    }
                }
                spaces = false;
            } else if is_space(value, i) {
                if allow_breaks
                    && !spaces
                    && self.column > self.best_width
                    && i > 0
                    && i < value.len() - 1
                {
                    self.write_indent()?;
                    if is_space(value, i + 1) {
                        self.put(b'\\');
                    }
                    i += width(value[i]);
                } else {
                    self.write(value, &mut i);
                }
                spaces = true;
            } else {
                self.write(value, &mut i);
                spaces = false;
            }
        }
        self.write_indicator(b"\"", false, false, false)?;
        self.whitespace = false;
        self.indention = false;
        Ok(())
    }

    fn write_block_scalar_hints(&mut self, value: &[u8]) -> EmitResult {
        if is_space(value, 0) {
            let hint = [b'0' + self.best_indent as u8];
            self.write_indicator(&hint, false, false, false)?;
        }
        self.open_ended = false;
        let mut chomp_hint: Option<u8> = None;
        if value.is_empty() {
            chomp_hint = Some(b'-');
        } else {
            let mut i = value.len() - 1;
            while i > 0 && value[i] & 0xC0 == 0x80 {
                i -= 1;
            }
            if !is_line_break(value, i) {
                chomp_hint = Some(b'-');
            } else if i == 0 {
                chomp_hint = Some(b'+');
                self.open_ended = true;
            } else {
                i -= 1;
                while i > 0 && value[i] & 0xC0 == 0x80 {
                    i -= 1;
                }
                if is_line_break(value, i) {
                    chomp_hint = Some(b'+');
                    self.open_ended = true;
                }
            }
        }
        if let Some(hint) = chomp_hint {
            self.write_indicator(&[hint], false, false, false)?;
        }
        Ok(())
    }

    fn write_literal_scalar(&mut self, value: &[u8]) -> EmitResult {
        self.write_indicator(b"|", true, false, false)?;
        self.write_block_scalar_hints(value)?;
        self.process_line_comment_linebreak(true)?;
        self.whitespace = true;
        let mut breaks = true;
        let mut i = 0;
        while i < value.len() {
            if is_line_break(value, i) {
                self.write_line_break(value, &mut i);
                breaks = true;
            } else {
                if breaks {
                    self.write_indent()?;
                }
                self.write(value, &mut i);
                self.indention = false;
                breaks = false;
            }
        }
        Ok(())
    }

    fn write_folded_scalar(&mut self, value: &[u8]) -> EmitResult {
        self.write_indicator(b">", true, false, false)?;
        self.write_block_scalar_hints(value)?;
        self.process_line_comment_linebreak(true)?;
        self.whitespace = true;
        let mut breaks = true;
        let mut leading_spaces = true;
        let mut i = 0;
        while i < value.len() {
            if is_line_break(value, i) {
                if !breaks && !leading_spaces && value[i] == b'\n' {
                    let mut k = 0;
                    while is_line_break(value, k) {
                        k += width(value[k]);
                    }
                    if !is_blank_or_zero(value, k) {
                        self.put_line_break();
                    }
                }
                self.write_line_break(value, &mut i);
                breaks = true;
            } else {
                if breaks {
                    self.write_indent()?;
                    leading_spaces = is_blank(value, i);
                }
                if !breaks
                    && is_space(value, i)
                    && !is_space(value, i + 1)
                    && self.column > self.best_width
                {
                    self.write_indent()?;
                    i += width(value[i]);
                } else {
                    self.write(value, &mut i);
                }
                self.indention = false;
                breaks = false;
            }
        }
        Ok(())
    }

    fn write_comment(&mut self, comment: &[u8]) -> EmitResult {
        let mut breaks = false;
        let mut pound = false;
        let mut i = 0;
        while i < comment.len() {
            if is_line_break(comment, i) {
                self.write_line_break(comment, &mut i);
                breaks = true;
                pound = false;
            } else {
                if breaks {
                    self.write_indent()?;
                }
                if !pound {
                    if comment[i] != b'#' {
                        self.put(b'#');
                        self.put(b' ');
                    }
                    pound = true;
                }
                self.write(comment, &mut i);
                self.indention = false;
                breaks = false;
            }
        }
        if !breaks {
            self.put_line_break();
        }
        self.whitespace = true;
        Ok(())
    }

    fn put(&mut self, value: u8) {
        self.out.push(value);
        self.column += 1;
    }

    fn put_line_break(&mut self) {
        self.out.push(b'\n');
        if self.column == 0 {
            self.space_above = true;
        }
        self.column = 0;
        self.line += 1;
        self.indention = true;
    }

    fn write(&mut self, s: &[u8], i: &mut usize) {
        let w = width(s[*i]);
        let end = (*i + w).min(s.len());
        self.out.extend_from_slice(&s[*i..end]);
        self.column += 1;
        *i = end;
    }

    fn write_all(&mut self, s: &[u8]) {
        let mut i = 0;
        while i < s.len() {
            self.write(s, &mut i);
        }
    }

    fn write_line_break(&mut self, s: &[u8], i: &mut usize) {
        if s[*i] == b'\n' {
            self.put_line_break();
            *i += 1;
        } else {
            self.write(s, i);
            if self.column == 0 {
                self.space_above = true;
            }
            self.column = 0;
            self.line += 1;
            self.indention = true;
        }
    }

    fn increase_indent(&mut self, flow: bool, indentless: bool) {
        self.indents.push(self.indent);
        if self.indent < 0 {
            self.indent = if flow { self.best_indent } else { 0 };
        } else if !indentless {
            if self.states.last() == Some(&State::BlockSequenceItem) {
                self.indent += 2;
            } else {
                self.indent = self.best_indent * ((self.indent + self.best_indent) / self.best_indent);
            }
        }
    }
}

/// Encodes a node tree as a YAML document the way yq's `Encode` does with
/// `UnwrapScalar` off: block style at `indent` spaces.
pub fn encode_node(arena: &Arena, root: NodeId, indent: usize) -> Result<String, String> {
    let mut serializer = Serializer {
        arena,
        events: vec![Event::new(EventType::StreamStart)],
    };
    serializer.serialize_document(root);
    serializer.events.push(Event::new(EventType::StreamEnd));
    let best_indent = indent.clamp(2, 9) as i64;
    let mut emitter = Emitter::new(serializer.events, best_indent);
    emitter.run()?;
    String::from_utf8(emitter.out).map_err(|e| e.to_string())
}
