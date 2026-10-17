//! Node arena: the port of `candidate_node.go`.
//!
//! Nodes live in an arena and refer to each other by index, which gives yq's
//! pointer semantics (parent links, identity comparison, in-place mutation)
//! without reference counting.

use crate::util::parse_int;
use crate::Error;

pub type NodeId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Sequence,
    Mapping,
    Scalar,
    Alias,
}

pub const TAGGED_STYLE: u32 = 1;
pub const DOUBLE_QUOTED_STYLE: u32 = 2;
pub const SINGLE_QUOTED_STYLE: u32 = 4;
pub const LITERAL_STYLE: u32 = 8;
pub const FOLDED_STYLE: u32 = 16;
pub const FLOW_STYLE: u32 = 32;

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    pub style: u32,
    pub tag: String,
    pub value: String,
    pub anchor: String,
    pub alias: Option<NodeId>,
    pub content: Vec<NodeId>,
    pub head_comment: String,
    pub line_comment: String,
    pub foot_comment: String,
    pub parent: Option<NodeId>,
    pub key: Option<NodeId>,
    pub leading_content: String,
    pub document: usize,
    pub filename: String,
    pub file_index: usize,
    pub line: usize,
    pub column: usize,
    pub evaluate_together: bool,
    pub is_map_key: bool,
}

impl Default for Node {
    fn default() -> Self {
        Node {
            kind: Kind::Scalar,
            style: 0,
            tag: String::new(),
            value: String::new(),
            anchor: String::new(),
            alias: None,
            content: Vec::new(),
            head_comment: String::new(),
            line_comment: String::new(),
            foot_comment: String::new(),
            parent: None,
            key: None,
            leading_content: String::new(),
            document: 0,
            filename: String::new(),
            file_index: 0,
            line: 0,
            column: 0,
            evaluate_together: false,
            is_map_key: false,
        }
    }
}

impl Node {
    pub fn scalar(tag: &str, value: &str) -> Node {
        Node {
            kind: Kind::Scalar,
            tag: tag.to_string(),
            value: value.to_string(),
            ..Node::default()
        }
    }

    /// `createStringScalarNode`: `<<` becomes a merge key.
    pub fn string_scalar(value: &str) -> Node {
        let tag = if value == "<<" { "!!merge" } else { "!!str" };
        Node::scalar(tag, value)
    }
}

/// One element of a node's path: a map key or a sequence index.
#[derive(Debug, Clone, PartialEq)]
pub enum PathElem {
    Index(i64),
    Key(String),
}

impl std::fmt::Display for PathElem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathElem::Index(i) => write!(f, "{i}"),
            PathElem::Key(k) => f.write_str(k),
        }
    }
}

/// Go's `%v` formatting of a path slice: `[a b 0]`.
pub fn format_path(path: &[PathElem]) -> String {
    let parts: Vec<String> = path.iter().map(|p| p.to_string()).collect();
    format!("[{}]", parts.join(" "))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AssignPrefs {
    pub dont_overwrite_anchor: bool,
    pub only_write_null: bool,
    pub clobber_custom_tags: bool,
}

#[derive(Debug, Default)]
pub struct Arena {
    nodes: Vec<Node>,
}

impl Arena {
    pub fn new() -> Arena {
        Arena::default()
    }

    pub fn push(&mut self, node: Node) -> NodeId {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    pub fn get(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    pub fn get_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id]
    }

    pub fn get_document(&self, id: NodeId) -> usize {
        let mut current = id;
        while let Some(parent) = self.nodes[current].parent {
            current = parent;
        }
        self.nodes[current].document
    }

    pub fn get_filename(&self, id: NodeId) -> &str {
        let mut current = id;
        while let Some(parent) = self.nodes[current].parent {
            current = parent;
        }
        &self.nodes[current].filename
    }

    pub fn get_file_index(&self, id: NodeId) -> usize {
        let mut current = id;
        while let Some(parent) = self.nodes[current].parent {
            current = parent;
        }
        self.nodes[current].file_index
    }

    /// `GetKey`: the identity string used to de-duplicate nodes.
    pub fn get_key(&self, id: NodeId) -> String {
        let node = &self.nodes[id];
        let key_prefix = if node.is_map_key {
            format!("key-{}-", node.value)
        } else {
            String::new()
        };
        let key = node.key.map(|k| self.nodes[k].value.as_str()).unwrap_or("");
        format!("{}{} - {}", key_prefix, self.get_document(id), key)
    }

    fn parsed_key(&self, id: NodeId) -> Option<PathElem> {
        let node = &self.nodes[id];
        if node.is_map_key {
            return Some(PathElem::Key(node.value.clone()));
        }
        let key = &self.nodes[node.key?];
        if key.tag == "!!str" {
            return Some(PathElem::Key(key.value.clone()));
        }
        match parse_int(&key.value) {
            Ok(index) => Some(PathElem::Index(index)),
            Err(_) => Some(PathElem::Key(key.value.clone())),
        }
    }

    pub fn get_path(&self, id: NodeId) -> Vec<PathElem> {
        let key = self.parsed_key(id);
        let node = &self.nodes[id];
        match (node.parent, key) {
            (Some(parent), Some(key)) => {
                let mut path = self.get_path(parent);
                path.push(key);
                path
            }
            (None, Some(key)) => vec![key],
            (_, None) => Vec::new(),
        }
    }

    /// `GetNicePath`: `a.b[0]` style, used in messages.
    pub fn get_nice_path(&self, id: NodeId) -> String {
        let mut out = String::new();
        for (i, element) in self.get_path(id).iter().enumerate() {
            match element {
                PathElem::Index(index) => out.push_str(&format!("[{index}]")),
                PathElem::Key(key) => {
                    if i == 0 {
                        out.push_str(key);
                    } else if key.contains('.') {
                        out.push_str(&format!("[{key}]"));
                    } else {
                        out.push('.');
                        out.push_str(key);
                    }
                }
            }
        }
        out
    }

    pub fn can_visit_values(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].kind, Kind::Mapping | Kind::Sequence)
    }

    /// The value children of a mapping or sequence (keys skipped).
    pub fn value_children(&self, id: NodeId) -> Vec<NodeId> {
        let node = &self.nodes[id];
        match node.kind {
            Kind::Mapping => node.content.iter().skip(1).step_by(2).copied().collect(),
            Kind::Sequence => node.content.clone(),
            _ => Vec::new(),
        }
    }

    pub fn add_key_value_child(
        &mut self,
        parent: NodeId,
        raw_key: NodeId,
        raw_value: NodeId,
    ) -> (NodeId, NodeId) {
        let key = self.copy(raw_key);
        {
            let key_node = self.get_mut(key);
            key_node.parent = Some(parent);
            key_node.is_map_key = true;
        }
        let value = self.copy(raw_value);
        {
            let value_node = self.get_mut(value);
            value_node.parent = Some(parent);
            value_node.is_map_key = false;
            value_node.key = Some(key);
        }
        let parent_node = self.get_mut(parent);
        parent_node.content.push(key);
        parent_node.content.push(value);
        (key, value)
    }

    pub fn add_child(&mut self, parent: NodeId, raw_child: NodeId) -> NodeId {
        let value = self.copy(raw_child);
        let index = self.nodes[parent].content.len();
        let key = self.push(Node {
            kind: Kind::Scalar,
            tag: "!!int".to_string(),
            value: index.to_string(),
            parent: Some(parent),
            ..Node::default()
        });
        {
            let value_node = self.get_mut(value);
            value_node.parent = Some(parent);
            value_node.is_map_key = false;
            value_node.key = Some(key);
        }
        self.get_mut(parent).content.push(value);
        value
    }

    pub fn add_children(&mut self, parent: NodeId, children: &[NodeId]) {
        if self.nodes[parent].kind == Kind::Mapping {
            let mut i = 0;
            while i + 1 < children.len() {
                self.add_key_value_child(parent, children[i], children[i + 1]);
                i += 2;
            }
        } else {
            for child in children {
                self.add_child(parent, *child);
            }
        }
    }

    pub fn copy(&mut self, id: NodeId) -> NodeId {
        self.do_copy(id, true)
    }

    pub fn copy_without_content(&mut self, id: NodeId) -> NodeId {
        self.do_copy(id, false)
    }

    fn do_copy(&mut self, id: NodeId, clone_content: bool) -> NodeId {
        let copy_key = self.nodes[id].key.map(|key| self.copy(key));
        let original = self.nodes[id].clone();
        let clone = self.push(Node {
            content: Vec::new(),
            key: copy_key,
            ..original
        });
        if clone_content {
            let content = self.nodes[id].content.clone();
            self.add_children(clone, &content);
        }
        clone
    }

    pub fn create_replacement(&mut self, id: NodeId, kind: Kind, tag: &str, value: &str) -> NodeId {
        let node = self.push(Node {
            kind,
            tag: tag.to_string(),
            value: value.to_string(),
            ..Node::default()
        });
        self.copy_as_replacement(id, node)
    }

    pub fn copy_as_replacement(&mut self, id: NodeId, replacement: NodeId) -> NodeId {
        let new_copy = self.copy(replacement);
        let (parent, key) = {
            let original = &self.nodes[id];
            let key = if original.is_map_key {
                Some(id)
            } else {
                original.key
            };
            (original.parent, key)
        };
        let copy_node = self.get_mut(new_copy);
        copy_node.parent = parent;
        copy_node.key = key;
        new_copy
    }

    pub fn create_replacement_with_comments(
        &mut self,
        id: NodeId,
        kind: Kind,
        tag: &str,
        style: u32,
    ) -> NodeId {
        let replacement = self.create_replacement(id, kind, tag, "");
        let original = self.nodes[id].clone();
        let node = self.get_mut(replacement);
        node.leading_content = original.leading_content;
        node.head_comment = original.head_comment;
        node.line_comment = original.line_comment;
        node.foot_comment = original.foot_comment;
        node.style = style;
        replacement
    }

    /// `UpdateFrom`: replaces this node's kind, value and content with `other`'s.
    pub fn update_from(&mut self, id: NodeId, other: NodeId, prefs: AssignPrefs) {
        if id == other {
            return;
        }
        let empty_collection =
            self.nodes[id].kind != Kind::Scalar && self.nodes[id].content.is_empty();
        if empty_collection
            || self.guess_tag_from_custom_type(id) != self.guess_tag_from_custom_type(other)
        {
            self.nodes[id].style = self.nodes[other].style;
        }
        self.nodes[id].content = Vec::new();
        self.nodes[id].kind = self.nodes[other].kind;
        let content = self.nodes[other].content.clone();
        self.add_children(id, &content);
        self.nodes[id].value = self.nodes[other].value.clone();
        self.update_attributes_from(id, other, prefs);
    }

    pub fn update_attributes_from(&mut self, id: NodeId, other: NodeId, prefs: AssignPrefs) {
        let other_node = self.nodes[other].clone();
        let node = &mut self.nodes[id];
        if node.kind != other_node.kind {
            node.content = Vec::new();
            node.value = String::new();
        }
        node.kind = other_node.kind;
        if prefs.clobber_custom_tags || node.tag.starts_with("!!") || node.tag.is_empty() {
            node.tag = other_node.tag;
        }
        node.alias = other_node.alias;
        if !prefs.dont_overwrite_anchor {
            node.anchor = other_node.anchor;
        }
        if node.style == 0 {
            node.style = other_node.style;
        }
        if !other_node.foot_comment.is_empty() {
            node.foot_comment = other_node.foot_comment;
        }
        if !other_node.head_comment.is_empty() {
            node.head_comment = other_node.head_comment;
        }
        if !other_node.line_comment.is_empty() {
            node.line_comment = other_node.line_comment;
        }
    }

    /// `guessTagFromCustomType`: a custom-tagged scalar is typed by parsing
    /// its value as YAML.
    pub fn guess_tag_from_custom_type(&mut self, id: NodeId) -> String {
        let node = &self.nodes[id];
        if node.tag.starts_with("!!") || node.value.is_empty() {
            return node.tag.clone();
        }
        let value = node.value.clone();
        match crate::yaml::decode::parse_snippet(self, &value) {
            Ok(parsed) => self.nodes[parsed].tag.clone(),
            Err(_) => self.nodes[id].tag.clone(),
        }
    }

    /// Typed view of a scalar, as `GetValueRep` returns it.
    pub fn get_value_rep(&mut self, id: NodeId) -> Result<ValueRep, Error> {
        let real_tag = self.guess_tag_from_custom_type(id);
        let node = &self.nodes[id];
        match real_tag.as_str() {
            "!!int" => {
                let (_, value) = crate::util::parse_int64(&node.value)?;
                Ok(ValueRep::Int(value))
            }
            "!!float" => Ok(ValueRep::Float(crate::util::parse_float(&node.value)?)),
            "!!bool" => Ok(ValueRep::Bool(crate::util::is_truthy_node(node))),
            "!!null" => Ok(ValueRep::Null),
            _ => Ok(ValueRep::Str(node.value.clone())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValueRep {
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    Str(String),
}

/// `recursiveNodeEqual` from `lib.go`.
pub fn recursive_node_equal(arena: &mut Arena, lhs: NodeId, rhs: NodeId) -> bool {
    let (lhs_kind, rhs_kind) = (arena.get(lhs).kind, arena.get(rhs).kind);
    if lhs_kind != rhs_kind {
        return false;
    }
    if lhs_kind == Kind::Scalar {
        let lhs_tag = arena.guess_tag_from_custom_type(lhs);
        let rhs_tag = arena.guess_tag_from_custom_type(rhs);
        if lhs_tag != rhs_tag {
            return false;
        }
    }
    if arena.get(lhs).tag == "!!null" {
        return true;
    }
    match lhs_kind {
        Kind::Scalar => arena.get(lhs).value == arena.get(rhs).value,
        Kind::Sequence => {
            let (lc, rc) = (
                arena.get(lhs).content.clone(),
                arena.get(rhs).content.clone(),
            );
            if lc.len() != rc.len() {
                return false;
            }
            lc.iter()
                .zip(rc.iter())
                .all(|(l, r)| recursive_node_equal(arena, *l, *r))
        }
        Kind::Mapping => {
            let (lc, rc) = (
                arena.get(lhs).content.clone(),
                arena.get(rhs).content.clone(),
            );
            if lc.len() != rc.len() {
                return false;
            }
            let mut index = 0;
            while index + 1 < lc.len() {
                let key = lc[index];
                let value = lc[index + 1];
                match find_key_in_map(arena, rhs, key) {
                    Some(index_in_rhs) => {
                        if !recursive_node_equal(arena, value, rc[index_in_rhs + 1]) {
                            return false;
                        }
                    }
                    None => return false,
                }
                index += 2;
            }
            true
        }
        Kind::Alias => false,
    }
}

pub fn find_in_array(arena: &mut Arena, array: NodeId, item: NodeId) -> Option<usize> {
    let content = arena.get(array).content.clone();
    content
        .iter()
        .position(|candidate| recursive_node_equal(arena, *candidate, item))
}

pub fn find_key_in_map(arena: &mut Arena, map: NodeId, item: NodeId) -> Option<usize> {
    let content = arena.get(map).content.clone();
    let mut index = 0;
    while index < content.len() {
        if recursive_node_equal(arena, content[index], item) {
            return Some(index);
        }
        index += 2;
    }
    None
}

/// `getContentValueByKey`: the value for a string key in a mapping.
pub fn content_value_by_key(arena: &Arena, map: NodeId, key: &str) -> Option<NodeId> {
    let content = &arena.get(map).content;
    let mut index = 0;
    while index + 1 < content.len() {
        if arena.get(content[index]).value == key {
            return Some(content[index + 1]);
        }
        index += 2;
    }
    None
}
