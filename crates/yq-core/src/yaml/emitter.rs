//! A port of go-yaml's `emitter.go` (libyaml's emitter), driven by the same
//! event stream its serializer produces. yq's output is this emitter's output
//! byte for byte, so the state machine is kept as it is in Go rather than
//! rewritten; only the buffering is simplified.

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventType {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScalarStyle {
    #[default]
    Any,
    Plain,
    SingleQuoted,
    DoubleQuoted,
    Literal,
    Folded,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub kind: EventType,
    pub anchor: Vec<u8>,
    pub tag: Vec<u8>,
    pub value: Vec<u8>,
    pub implicit: bool,
    pub quoted_implicit: bool,
    pub scalar_style: ScalarStyle,
    pub flow: bool,
    pub head_comment: Vec<u8>,
    pub line_comment: Vec<u8>,
    pub foot_comment: Vec<u8>,
    pub tail_comment: Vec<u8>,
}

impl Event {
    pub fn new(kind: EventType) -> Event {
        Event {
            kind,
            anchor: Vec::new(),
            tag: Vec::new(),
            value: Vec::new(),
            implicit: false,
            quoted_implicit: false,
            scalar_style: ScalarStyle::Any,
            flow: false,
            head_comment: Vec::new(),
            line_comment: Vec::new(),
            foot_comment: Vec::new(),
            tail_comment: Vec::new(),
        }
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
    style: ScalarStyle,
}

const DEFAULT_TAG_DIRECTIVES: [(&[u8], &[u8]); 2] = [(b"!", b"!"), (b"!!", b"tag:yaml.org,2002:")];

pub struct Emitter {
    out: Vec<u8>,
    best_indent: i32,
    best_width: i32,
    unicode: bool,
    compact_sequence_indent: bool,
    quote_double: bool,
    state: State,
    states: Vec<State>,
    events: Vec<Event>,
    events_head: usize,
    indents: Vec<i32>,
    indent: i32,
    flow_level: i32,
    root_context: bool,
    sequence_context: bool,
    mapping_context: bool,
    simple_key_context: bool,
    line: i32,
    column: i32,
    whitespace: bool,
    indention: bool,
    open_ended: bool,
    space_above: bool,
    foot_indent: i32,
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

impl Emitter {
    pub fn new(indent: usize, compact_sequence_indent: bool) -> Emitter {
        Emitter {
            out: Vec::new(),
            best_indent: indent as i32,
            best_width: -1,
            unicode: true,
            compact_sequence_indent,
            quote_double: false,
            state: State::StreamStart,
            states: Vec::new(),
            events: Vec::new(),
            events_head: 0,
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

    pub fn set_open_ended(&mut self, value: bool) {
        self.open_ended = value;
    }

    pub fn into_output(self) -> Vec<u8> {
        self.out
    }

    pub fn emit(&mut self, event: Event) -> Result<(), Error> {
        self.events.push(event);
        while !self.need_more_events() {
            let event = self.events[self.events_head].clone();
            self.analyze_event(&event)?;
            self.state_machine(&event)?;
            self.events_head += 1;
        }
        if self.events_head == self.events.len() {
            self.events.clear();
            self.events_head = 0;
        }
        Ok(())
    }

    fn need_more_events(&self) -> bool {
        if self.events_head == self.events.len() {
            return true;
        }
        let accumulate = match self.events[self.events_head].kind {
            EventType::DocumentStart => 1,
            EventType::SequenceStart => 2,
            EventType::MappingStart => 3,
            _ => return false,
        };
        if self.events.len() - self.events_head > accumulate {
            return false;
        }
        let mut level = 0;
        for event in &self.events[self.events_head..] {
            match event.kind {
                EventType::StreamStart
                | EventType::DocumentStart
                | EventType::SequenceStart
                | EventType::MappingStart => level += 1,
                EventType::StreamEnd
                | EventType::DocumentEnd
                | EventType::SequenceEnd
                | EventType::MappingEnd => level -= 1,
                _ => {}
            }
            if level == 0 {
                return false;
            }
        }
        true
    }

    fn state_machine(&mut self, event: &Event) -> Result<(), Error> {
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
            State::End => Err(Error("expected nothing after STREAM-END".to_string())),
        }
    }

    fn emit_stream_start(&mut self, event: &Event) -> Result<(), Error> {
        if event.kind != EventType::StreamStart {
            return Err(Error("expected STREAM-START".to_string()));
        }
        if self.best_indent < 2 || self.best_indent > 9 {
            self.best_indent = 2;
        }
        if self.best_width >= 0 && self.best_width <= self.best_indent * 2 {
            self.best_width = 80;
        }
        if self.best_width < 0 {
            self.best_width = i32::MAX;
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

    fn emit_document_start(&mut self, event: &Event, first: bool) -> Result<(), Error> {
        if event.kind == EventType::DocumentStart {
            let mut implicit = event.implicit;
            if !first {
                implicit = false;
            }
            if !implicit {
                self.write_indent();
                self.write_indicator(b"---", true, false, false);
                self.write_indent();
            }
            if !self.head_comment.is_empty() {
                self.process_head_comment();
                self.put_line_break();
            }
            self.state = State::DocumentContent;
            return Ok(());
        }
        if event.kind == EventType::StreamEnd {
            if self.open_ended {
                self.write_indicator(b"...", true, false, false);
                self.write_indent();
            }
            self.state = State::End;
            return Ok(());
        }
        Err(Error("expected DOCUMENT-START or STREAM-END".to_string()))
    }

    fn emit_document_content(&mut self, event: &Event) -> Result<(), Error> {
        self.states.push(State::DocumentEnd);
        self.process_head_comment();
        self.emit_node(event, true, false, false, false)?;
        self.process_line_comment();
        self.process_foot_comment();
        Ok(())
    }

    fn emit_document_end(&mut self, event: &Event) -> Result<(), Error> {
        if event.kind != EventType::DocumentEnd {
            return Err(Error("expected DOCUMENT-END".to_string()));
        }
        self.foot_indent = 0;
        self.process_foot_comment();
        self.foot_indent = -1;
        self.write_indent();
        if !event.implicit {
            self.write_indicator(b"...", true, false, false);
            self.write_indent();
        }
        self.state = State::DocumentStart;
        Ok(())
    }

    fn pop_indent_and_state(&mut self) {
        self.indent = self.indents.pop().unwrap_or(-1);
        self.state = self.states.pop().unwrap_or(State::End);
    }

    fn has_trailing_comments(&self) -> bool {
        !self.line_comment.is_empty()
            || !self.foot_comment.is_empty()
            || !self.tail_comment.is_empty()
    }

    fn emit_flow_sequence_item(
        &mut self,
        event: &Event,
        first: bool,
        trail: bool,
    ) -> Result<(), Error> {
        if first {
            self.write_indicator(b"[", true, true, false);
            self.increase_indent(true, false);
            self.flow_level += 1;
        }
        if event.kind == EventType::SequenceEnd {
            self.flow_level -= 1;
            self.indent = self.indents.pop().unwrap_or(-1);
            if self.column == 0 {
                self.write_indent();
            }
            self.write_indicator(b"]", false, false, false);
            self.process_line_comment();
            self.process_foot_comment();
            self.state = self.states.pop().unwrap_or(State::End);
            return Ok(());
        }
        if !first && !trail {
            self.write_indicator(b",", false, false, false);
        }
        self.process_head_comment();
        if self.column == 0 {
            self.write_indent();
        }
        if self.column > self.best_width {
            self.write_indent();
        }
        if self.has_trailing_comments() {
            self.states.push(State::FlowSequenceTrailItem);
        } else {
            self.states.push(State::FlowSequenceItem);
        }
        self.emit_node(event, false, true, false, false)?;
        if self.has_trailing_comments() {
            self.write_indicator(b",", false, false, false);
        }
        self.process_line_comment();
        self.process_foot_comment();
        Ok(())
    }

    fn emit_flow_mapping_key(
        &mut self,
        event: &Event,
        first: bool,
        trail: bool,
    ) -> Result<(), Error> {
        if first {
            self.write_indicator(b"{", true, true, false);
            self.increase_indent(true, false);
            self.flow_level += 1;
        }
        if event.kind == EventType::MappingEnd {
            let has_comments = !self.head_comment.is_empty()
                || !self.foot_comment.is_empty()
                || !self.tail_comment.is_empty();
            if has_comments && !first && !trail {
                self.write_indicator(b",", false, false, false);
            }
            self.process_head_comment();
            self.flow_level -= 1;
            self.indent = self.indents.pop().unwrap_or(-1);
            self.write_indicator(b"}", false, false, false);
            self.process_line_comment();
            self.process_foot_comment();
            self.state = self.states.pop().unwrap_or(State::End);
            return Ok(());
        }
        if !first && !trail {
            self.write_indicator(b",", false, false, false);
        }
        self.process_head_comment();
        if self.column == 0 {
            self.write_indent();
        }
        if self.column > self.best_width {
            self.write_indent();
        }
        if self.check_simple_key() {
            self.states.push(State::FlowMappingSimpleValue);
            return self.emit_node(event, false, false, true, true);
        }
        self.write_indicator(b"?", true, false, false);
        self.states.push(State::FlowMappingValue);
        self.emit_node(event, false, false, true, false)
    }

    fn emit_flow_mapping_value(&mut self, event: &Event, simple: bool) -> Result<(), Error> {
        if simple {
            self.write_indicator(b":", false, false, false);
        } else {
            if self.column > self.best_width {
                self.write_indent();
            }
            self.write_indicator(b":", true, false, false);
        }
        if self.has_trailing_comments() {
            self.states.push(State::FlowMappingTrailKey);
        } else {
            self.states.push(State::FlowMappingKey);
        }
        self.emit_node(event, false, false, true, false)?;
        if self.has_trailing_comments() {
            self.write_indicator(b",", false, false, false);
        }
        self.process_line_comment();
        self.process_foot_comment();
        Ok(())
    }

    fn emit_block_sequence_item(&mut self, event: &Event, first: bool) -> Result<(), Error> {
        if first {
            let seq = self.mapping_context
                && (self.column == 0 || !self.indention)
                && self.compact_sequence_indent;
            self.increase_indent_compact(false, false, seq);
        }
        if event.kind == EventType::SequenceEnd {
            self.pop_indent_and_state();
            return Ok(());
        }
        self.process_head_comment();
        self.write_indent();
        self.write_indicator(b"-", true, false, true);
        self.states.push(State::BlockSequenceItem);
        self.emit_node(event, false, true, false, false)?;
        self.process_line_comment();
        self.process_foot_comment();
        Ok(())
    }

    fn emit_block_mapping_key(&mut self, event: &Event, first: bool) -> Result<(), Error> {
        if first {
            self.increase_indent(false, false);
        }
        self.process_head_comment();
        if event.kind == EventType::MappingEnd {
            self.pop_indent_and_state();
            return Ok(());
        }
        self.write_indent();
        if !self.line_comment.is_empty() {
            self.key_line_comment = std::mem::take(&mut self.line_comment);
        }
        if self.check_simple_key() {
            self.states.push(State::BlockMappingSimpleValue);
            self.emit_node(event, false, false, true, true)?;
            if event.kind == EventType::Alias {
                self.put(b' ');
            }
            return Ok(());
        }
        self.write_indicator(b"?", true, false, true);
        self.states.push(State::BlockMappingValue);
        self.emit_node(event, false, false, true, false)
    }

    fn emit_block_mapping_value(&mut self, event: &Event, simple: bool) -> Result<(), Error> {
        if simple {
            self.write_indicator(b":", false, false, false);
        } else {
            self.write_indent();
            self.write_indicator(b":", true, false, true);
        }
        if !self.key_line_comment.is_empty() {
            if event.kind == EventType::Scalar {
                if self.line_comment.is_empty() {
                    self.line_comment = std::mem::take(&mut self.key_line_comment);
                }
            } else if !event.flow
                && (event.kind == EventType::MappingStart || event.kind == EventType::SequenceStart)
            {
                std::mem::swap(&mut self.line_comment, &mut self.key_line_comment);
                self.process_line_comment();
                std::mem::swap(&mut self.line_comment, &mut self.key_line_comment);
            }
        }
        self.states.push(State::BlockMappingKey);
        self.emit_node(event, false, false, true, false)?;
        self.process_line_comment();
        self.process_foot_comment();
        Ok(())
    }

    fn emit_node(
        &mut self,
        event: &Event,
        root: bool,
        sequence: bool,
        mapping: bool,
        simple_key: bool,
    ) -> Result<(), Error> {
        self.root_context = root;
        self.sequence_context = sequence;
        self.mapping_context = mapping;
        self.simple_key_context = simple_key;
        match event.kind {
            EventType::Alias => self.emit_alias(),
            EventType::Scalar => self.emit_scalar(event),
            EventType::SequenceStart => self.emit_sequence_start(event),
            EventType::MappingStart => self.emit_mapping_start(event),
            other => Err(Error(format!(
                "expected SCALAR, SEQUENCE-START, MAPPING-START, or ALIAS, but got {other:?}"
            ))),
        }
    }

    fn emit_alias(&mut self) -> Result<(), Error> {
        self.process_anchor();
        self.state = self.states.pop().unwrap_or(State::End);
        Ok(())
    }

    fn emit_scalar(&mut self, event: &Event) -> Result<(), Error> {
        self.select_scalar_style(event)?;
        self.process_anchor();
        self.process_tag();
        self.increase_indent(true, false);
        self.process_scalar();
        self.pop_indent_and_state();
        Ok(())
    }

    fn emit_sequence_start(&mut self, event: &Event) -> Result<(), Error> {
        self.process_anchor();
        self.process_tag();
        if self.flow_level > 0 || event.flow || self.check_empty_sequence() {
            self.state = State::FlowSequenceFirstItem;
        } else {
            self.state = State::BlockSequenceFirstItem;
        }
        Ok(())
    }

    fn emit_mapping_start(&mut self, event: &Event) -> Result<(), Error> {
        self.process_anchor();
        self.process_tag();
        if self.flow_level > 0 || event.flow || self.check_empty_mapping() {
            self.state = State::FlowMappingFirstKey;
        } else {
            self.state = State::BlockMappingFirstKey;
        }
        Ok(())
    }

    fn check_empty_sequence(&self) -> bool {
        if self.events.len() - self.events_head < 2 {
            return false;
        }
        self.events[self.events_head].kind == EventType::SequenceStart
            && self.events[self.events_head + 1].kind == EventType::SequenceEnd
    }

    fn check_empty_mapping(&self) -> bool {
        if self.events.len() - self.events_head < 2 {
            return false;
        }
        self.events[self.events_head].kind == EventType::MappingStart
            && self.events[self.events_head + 1].kind == EventType::MappingEnd
    }

    fn check_simple_key(&self) -> bool {
        let mut length = 0;
        match self.events[self.events_head].kind {
            EventType::Alias => length += self.anchor.len(),
            EventType::Scalar => {
                if self.scalar.multiline {
                    return false;
                }
                length += self.anchor.len()
                    + self.tag_handle.len()
                    + self.tag_suffix.len()
                    + self.scalar.value.len();
            }
            EventType::SequenceStart => {
                if !self.check_empty_sequence() {
                    return false;
                }
                length += self.anchor.len() + self.tag_handle.len() + self.tag_suffix.len();
            }
            EventType::MappingStart => {
                if !self.check_empty_mapping() {
                    return false;
                }
                length += self.anchor.len() + self.tag_handle.len() + self.tag_suffix.len();
            }
            _ => return false,
        }
        length <= 128
    }

    fn process_anchor(&mut self) {
        if self.anchor.is_empty() {
            return;
        }
        let indicator: &[u8] = if self.anchor_is_alias { b"*" } else { b"&" };
        self.write_indicator(indicator, true, false, false);
        let anchor = self.anchor.clone();
        self.write_all(&anchor);
        self.whitespace = false;
        self.indention = false;
    }

    fn process_tag(&mut self) {
        if self.tag_handle.is_empty() && self.tag_suffix.is_empty() {
            return;
        }
        if !self.tag_handle.is_empty() {
            let handle = self.tag_handle.clone();
            self.write_tag_handle(&handle);
            if !self.tag_suffix.is_empty() {
                let suffix = self.tag_suffix.clone();
                self.write_tag_content(&suffix, false);
            }
        } else {
            self.write_indicator(b"!<", true, false, false);
            let suffix = self.tag_suffix.clone();
            self.write_tag_content(&suffix, false);
            self.write_indicator(b">", false, false, false);
        }
    }

    fn process_scalar(&mut self) {
        let value = std::mem::take(&mut self.scalar.value);
        let allow_breaks = !self.simple_key_context;
        match self.scalar.style {
            ScalarStyle::Plain | ScalarStyle::Any => self.write_plain_scalar(&value, allow_breaks),
            ScalarStyle::SingleQuoted => self.write_single_quoted_scalar(&value, allow_breaks),
            ScalarStyle::DoubleQuoted => self.write_double_quoted_scalar(&value, allow_breaks),
            ScalarStyle::Literal => self.write_literal_scalar(&value),
            ScalarStyle::Folded => self.write_folded_scalar(&value),
        }
        self.scalar.value = value;
    }

    fn process_head_comment(&mut self) {
        if !self.tail_comment.is_empty() {
            self.write_indent();
            let comment = std::mem::take(&mut self.tail_comment);
            self.write_comment(&comment);
            self.foot_indent = self.indent;
            if self.foot_indent < 0 {
                self.foot_indent = 0;
            }
        }
        if self.head_comment.is_empty() {
            return;
        }
        self.write_indent();
        let comment = std::mem::take(&mut self.head_comment);
        self.write_comment(&comment);
    }

    fn process_line_comment(&mut self) {
        self.process_line_comment_linebreak(false);
    }

    fn process_line_comment_linebreak(&mut self, linebreak: bool) {
        if self.line_comment.is_empty() {
            if linebreak {
                self.put_line_break();
            }
            return;
        }
        if !self.whitespace {
            self.put(b' ');
        }
        let comment = std::mem::take(&mut self.line_comment);
        self.write_comment(&comment);
    }

    fn process_foot_comment(&mut self) {
        if self.foot_comment.is_empty() {
            return;
        }
        self.write_indent();
        let comment = std::mem::take(&mut self.foot_comment);
        self.write_comment(&comment);
        self.foot_indent = self.indent;
        if self.foot_indent < 0 {
            self.foot_indent = 0;
        }
    }

    fn analyze_anchor(&mut self, anchor: &[u8], alias: bool) -> Result<(), Error> {
        if anchor.is_empty() {
            let problem = if alias {
                "alias value must not be empty"
            } else {
                "anchor value must not be empty"
            };
            return Err(Error(problem.to_string()));
        }
        let mut i = 0;
        while i < anchor.len() {
            if !is_anchor_char(anchor, i) {
                let problem = if alias {
                    "alias value must contain valid characters only"
                } else {
                    "anchor value must contain valid characters only"
                };
                return Err(Error(problem.to_string()));
            }
            i += width(anchor[i]);
        }
        self.anchor = anchor.to_vec();
        self.anchor_is_alias = alias;
        Ok(())
    }

    fn analyze_tag(&mut self, tag: &[u8]) -> Result<(), Error> {
        if tag.is_empty() {
            return Err(Error("tag value must not be empty".to_string()));
        }
        for (handle, prefix) in DEFAULT_TAG_DIRECTIVES {
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

        if value.len() >= 3 && (value.starts_with(b"---") || value.starts_with(b"...")) {
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
                    b'-' if followed_by_whitespace => {
                        flow_indicators = true;
                        block_indicators = true;
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
                    b'#' if preceded_by_whitespace => {
                        flow_indicators = true;
                        block_indicators = true;
                    }
                    _ => {}
                }
            }
            if value[i] == b'\t' {
                tab_characters = true;
            } else if !is_printable(value, i) || (!is_ascii(value, i) && !self.unicode) {
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

    fn analyze_event(&mut self, event: &Event) -> Result<(), Error> {
        self.anchor.clear();
        self.tag_handle.clear();
        self.tag_suffix.clear();
        self.scalar.value.clear();
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
        match event.kind {
            EventType::Alias => self.analyze_anchor(&event.anchor, true)?,
            EventType::Scalar => {
                if !event.anchor.is_empty() {
                    self.analyze_anchor(&event.anchor, false)?;
                }
                if !event.tag.is_empty() && !event.implicit && !event.quoted_implicit {
                    self.analyze_tag(&event.tag)?;
                }
                self.analyze_scalar(&event.value);
            }
            EventType::SequenceStart | EventType::MappingStart => {
                if !event.anchor.is_empty() {
                    self.analyze_anchor(&event.anchor, false)?;
                }
                if !event.tag.is_empty() && !event.implicit {
                    self.analyze_tag(&event.tag)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn required_quote_style(&self) -> ScalarStyle {
        if self.quote_double {
            ScalarStyle::DoubleQuoted
        } else {
            ScalarStyle::SingleQuoted
        }
    }

    fn select_scalar_style(&mut self, event: &Event) -> Result<(), Error> {
        let no_tag = self.tag_handle.is_empty() && self.tag_suffix.is_empty();
        if no_tag && !event.implicit && !event.quoted_implicit {
            return Err(Error(
                "neither tag nor implicit flags are specified".to_string(),
            ));
        }
        let mut style = event.scalar_style;
        if style == ScalarStyle::Any {
            style = ScalarStyle::Plain;
        }
        if self.simple_key_context && self.scalar.multiline {
            style = ScalarStyle::DoubleQuoted;
        }
        if style == ScalarStyle::Plain {
            if (self.flow_level > 0 && !self.scalar.flow_plain_allowed)
                || (self.flow_level == 0 && !self.scalar.block_plain_allowed)
            {
                style = self.required_quote_style();
            }
            if self.scalar.value.is_empty() && (self.flow_level > 0 || self.simple_key_context) {
                style = self.required_quote_style();
            }
            if no_tag && !event.implicit {
                style = self.required_quote_style();
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
        self.scalar.style = style;
        Ok(())
    }

    fn write_indent(&mut self) {
        let indent = if self.indent < 0 { 0 } else { self.indent };
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
    }

    fn write_indicator(
        &mut self,
        indicator: &[u8],
        need_whitespace: bool,
        is_whitespace: bool,
        is_indention: bool,
    ) {
        if need_whitespace && !self.whitespace {
            self.put(b' ');
        }
        self.write_all(indicator);
        self.whitespace = is_whitespace;
        self.indention = self.indention && is_indention;
        self.open_ended = false;
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
            let must_write = matches!(
                value[i],
                b';' | b'/'
                    | b'?'
                    | b':'
                    | b'@'
                    | b'&'
                    | b'='
                    | b'+'
                    | b'$'
                    | b','
                    | b'_'
                    | b'.'
                    | b'~'
                    | b'*'
                    | b'\''
                    | b'('
                    | b')'
                    | b'['
                    | b']'
            ) || is_alpha(value, i);
            if must_write {
                self.write(value, &mut i);
            } else {
                let w = width(value[i]);
                for _ in 0..w {
                    let octet = value[i];
                    i += 1;
                    self.put(b'%');
                    self.put(hex_digit(octet >> 4));
                    self.put(hex_digit(octet & 0x0f));
                }
            }
        }
        self.whitespace = false;
        self.indention = false;
    }

    fn write_plain_scalar(&mut self, value: &[u8], allow_breaks: bool) {
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
                    && !is_space_at(value, i + 1)
                {
                    self.write_indent();
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
                    self.write_indent();
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
    }

    fn write_single_quoted_scalar(&mut self, value: &[u8], allow_breaks: bool) {
        self.write_indicator(b"'", true, false, false);
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
                    && !is_space_at(value, i + 1)
                {
                    self.write_indent();
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
                    self.write_indent();
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
        self.write_indicator(b"'", false, false, false);
        self.whitespace = false;
        self.indention = false;
    }

    fn write_double_quoted_scalar(&mut self, value: &[u8], allow_breaks: bool) {
        let mut spaces = false;
        self.write_indicator(b"\"", true, false, false);
        let mut i = 0;
        while i < value.len() {
            if !is_printable(value, i)
                || (!self.unicode && !is_ascii(value, i))
                || is_bom(value, i)
                || is_line_break(value, i)
                || value[i] == b'"'
                || value[i] == b'\\'
            {
                let octet = value[i];
                let (mut w, mut v): (usize, u32) = if octet & 0x80 == 0x00 {
                    (1, (octet & 0x7F) as u32)
                } else if octet & 0xE0 == 0xC0 {
                    (2, (octet & 0x1F) as u32)
                } else if octet & 0xF0 == 0xE0 {
                    (3, (octet & 0x0F) as u32)
                } else if octet & 0xF8 == 0xF0 {
                    (4, (octet & 0x07) as u32)
                } else {
                    (1, octet as u32)
                };
                for k in 1..w {
                    let octet = value[i + k];
                    v = (v << 6) + (octet as u32 & 0x3F);
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
                        if v <= 0xFF {
                            self.put(b'x');
                            w = 2;
                        } else if v <= 0xFFFF {
                            self.put(b'u');
                            w = 4;
                        } else {
                            self.put(b'U');
                            w = 8;
                        }
                        let mut k = (w as i32 - 1) * 4;
                        while k >= 0 {
                            let digit = ((v >> k) & 0x0F) as u8;
                            self.put(hex_digit(digit));
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
                    self.write_indent();
                    if is_space_at(value, i + 1) {
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
        self.write_indicator(b"\"", false, false, false);
        self.whitespace = false;
        self.indention = false;
    }

    fn write_block_scalar_hints(&mut self, value: &[u8]) {
        if !value.is_empty() && is_space(value, 0) {
            let hint = [b'0' + self.best_indent as u8];
            self.write_indicator(&hint, false, false, false);
        }
        self.open_ended = false;
        let mut chomp_hint = 0u8;
        if value.is_empty() {
            chomp_hint = b'-';
        } else {
            let mut i = value.len() - 1;
            while value[i] & 0xC0 == 0x80 {
                i -= 1;
            }
            if !is_line_break(value, i) {
                chomp_hint = b'-';
            } else if i == 0 {
                chomp_hint = b'+';
                self.open_ended = true;
            } else {
                i -= 1;
                while value[i] & 0xC0 == 0x80 {
                    i -= 1;
                }
                if is_line_break(value, i) {
                    chomp_hint = b'+';
                    self.open_ended = true;
                }
            }
        }
        if chomp_hint != 0 {
            self.write_indicator(&[chomp_hint], false, false, false);
        }
    }

    fn write_literal_scalar(&mut self, value: &[u8]) {
        self.write_indicator(b"|", true, false, false);
        self.write_block_scalar_hints(value);
        self.process_line_comment_linebreak(true);
        self.whitespace = true;
        let mut breaks = true;
        let mut i = 0;
        while i < value.len() {
            if is_line_break(value, i) {
                self.write_line_break(value, &mut i);
                breaks = true;
            } else {
                if breaks {
                    self.write_indent();
                }
                self.write(value, &mut i);
                self.indention = false;
                breaks = false;
            }
        }
    }

    fn write_folded_scalar(&mut self, value: &[u8]) {
        self.write_indicator(b">", true, false, false);
        self.write_block_scalar_hints(value);
        self.process_line_comment_linebreak(true);
        self.whitespace = true;
        let mut breaks = true;
        let mut leading_spaces = true;
        let mut i = 0;
        while i < value.len() {
            if is_line_break(value, i) {
                if !breaks && !leading_spaces && value[i] == b'\n' {
                    let mut k = 0;
                    while k < value.len() && is_line_break(value, k) {
                        k += width(value[k]);
                    }
                    if k >= value.len() || !is_blank_or_zero(value, k) {
                        self.put_line_break();
                    }
                }
                self.write_line_break(value, &mut i);
                breaks = true;
            } else {
                if breaks {
                    self.write_indent();
                    leading_spaces = is_blank(value, i);
                }
                if !breaks
                    && is_space(value, i)
                    && !is_space_at(value, i + 1)
                    && self.column > self.best_width
                {
                    self.write_indent();
                    i += width(value[i]);
                } else {
                    self.write(value, &mut i);
                }
                self.indention = false;
                breaks = false;
            }
        }
    }

    fn write_comment(&mut self, comment: &[u8]) {
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
                    self.write_indent();
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
        let w = width(s[*i]).max(1);
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

    fn increase_indent_compact(&mut self, flow: bool, indentless: bool, compact_seq: bool) {
        self.indents.push(self.indent);
        if self.indent < 0 {
            self.indent = if flow { self.best_indent } else { 0 };
        } else if !indentless {
            if self.states.last() == Some(&State::BlockSequenceItem) {
                self.indent += 2;
            } else {
                self.indent =
                    self.best_indent * ((self.indent + self.best_indent) / self.best_indent);
                if compact_seq {
                    self.indent -= 2;
                }
            }
        }
    }

    fn increase_indent(&mut self, flow: bool, indentless: bool) {
        self.increase_indent_compact(flow, indentless, false);
    }
}

fn hex_digit(value: u8) -> u8 {
    if value < 10 {
        value + b'0'
    } else {
        value + b'A' - 10
    }
}

pub fn width(b: u8) -> usize {
    if b & 0x80 == 0x00 {
        1
    } else if b & 0xE0 == 0xC0 {
        2
    } else if b & 0xF0 == 0xE0 {
        3
    } else if b & 0xF8 == 0xF0 {
        4
    } else {
        0
    }
}

fn at(b: &[u8], i: usize) -> u8 {
    b.get(i).copied().unwrap_or(0)
}

fn is_alpha(b: &[u8], i: usize) -> bool {
    let c = at(b, i);
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

fn is_ascii(b: &[u8], i: usize) -> bool {
    at(b, i) <= 0x7F
}

fn is_printable(b: &[u8], i: usize) -> bool {
    let c = at(b, i);
    let c1 = at(b, i + 1);
    let c2 = at(b, i + 2);
    c == 0x0A
        || (0x20..=0x7E).contains(&c)
        || (c == 0xC2 && c1 >= 0xA0)
        || (c > 0xC2 && c < 0xED)
        || (c == 0xED && c1 < 0xA0)
        || c == 0xEE
        || (c == 0xEF && !(c1 == 0xBB && c2 == 0xBF) && !(c1 == 0xBF && (c2 == 0xBE || c2 == 0xBF)))
}

fn is_bom(b: &[u8], _i: usize) -> bool {
    at(b, 0) == 0xEF && at(b, 1) == 0xBB && at(b, 2) == 0xBF
}

fn is_space(b: &[u8], i: usize) -> bool {
    at(b, i) == b' '
}

fn is_space_at(b: &[u8], i: usize) -> bool {
    i < b.len() && b[i] == b' '
}

fn is_blank(b: &[u8], i: usize) -> bool {
    let c = at(b, i);
    c == b' ' || c == b'\t'
}

fn is_line_break(b: &[u8], i: usize) -> bool {
    let c = at(b, i);
    let c1 = at(b, i + 1);
    let c2 = at(b, i + 2);
    c == b'\r'
        || c == b'\n'
        || (c == 0xC2 && c1 == 0x85)
        || (c == 0xE2 && c1 == 0x80 && c2 == 0xA8)
        || (c == 0xE2 && c1 == 0x80 && c2 == 0xA9)
}

fn is_blank_or_zero(b: &[u8], i: usize) -> bool {
    is_blank(b, i) || is_line_break(b, i) || at(b, i) == 0
}

fn is_flow_indicator(b: &[u8], i: usize) -> bool {
    matches!(at(b, i), b'[' | b']' | b'{' | b'}' | b',')
}

fn is_anchor_char(b: &[u8], i: usize) -> bool {
    if at(b, i) == b':' {
        return false;
    }
    is_printable(b, i)
        && !is_line_break(b, i)
        && !is_blank(b, i)
        && !is_bom(b, i)
        && !is_flow_indicator(b, i)
        && is_ascii(b, i)
}
