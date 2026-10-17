//! `context.go`: the list of matching nodes plus variables. The node list is
//! reference counted because yq compares lists by identity in `union`: two
//! operators that both return their input context unchanged must not have
//! their results duplicated. The variable map is reference counted too: a Go
//! `Context` passed by value shares its map, so a variable set inside an
//! operator (the decode operator does this) is visible to the caller, while
//! `ChildContext` copies the map.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::node::NodeId;

#[derive(Debug, Clone, Default)]
pub struct Context {
    pub matching_nodes: Rc<Vec<NodeId>>,
    pub variables: Rc<RefCell<HashMap<String, Rc<Vec<NodeId>>>>>,
    pub dont_auto_create: bool,
    pub datetime_layout: Option<String>,
}

pub const RFC3339: &str = "2006-01-02T15:04:05Z07:00";

impl Context {
    pub fn new(nodes: Vec<NodeId>) -> Context {
        Context {
            matching_nodes: Rc::new(nodes),
            ..Context::default()
        }
    }

    pub fn nodes(&self) -> &[NodeId] {
        &self.matching_nodes
    }

    pub fn len(&self) -> usize {
        self.matching_nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.matching_nodes.is_empty()
    }

    pub fn first(&self) -> Option<NodeId> {
        self.matching_nodes.first().copied()
    }

    pub fn same_list(&self, other: &Context) -> bool {
        Rc::ptr_eq(&self.matching_nodes, &other.matching_nodes)
    }

    pub fn datetime_layout(&self) -> &str {
        match &self.datetime_layout {
            Some(layout) if !layout.is_empty() => layout,
            _ => RFC3339,
        }
    }

    pub fn set_datetime_layout(&mut self, layout: &str) {
        self.datetime_layout = Some(layout.to_string());
    }

    pub fn get_variable(&self, name: &str) -> Option<Rc<Vec<NodeId>>> {
        self.variables.borrow().get(name).cloned()
    }

    pub fn set_variable(&mut self, name: &str, value: Vec<NodeId>) {
        self.variables
            .borrow_mut()
            .insert(name.to_string(), Rc::new(value));
    }

    /// `ChildContext`: same flags, a copy of the variables, a new node list.
    pub fn child_context(&self, results: Vec<NodeId>) -> Context {
        self.child_context_shared(Rc::new(results))
    }

    pub fn child_context_shared(&self, results: Rc<Vec<NodeId>>) -> Context {
        Context {
            matching_nodes: results,
            variables: Rc::new(RefCell::new(self.variables.borrow().clone())),
            dont_auto_create: self.dont_auto_create,
            datetime_layout: self.datetime_layout.clone(),
        }
    }

    pub fn single_child_context(&self, candidate: NodeId) -> Context {
        self.child_context(vec![candidate])
    }

    pub fn single_readonly_child_context(&self, candidate: NodeId) -> Context {
        let mut context = self.child_context(vec![candidate]);
        context.dont_auto_create = true;
        context
    }

    /// A Go `Context` passed by value: the same node list and the same
    /// variable map.
    pub fn clone_context(&self) -> Context {
        self.clone()
    }

    /// `ReadOnlyClone`: `Clone` copies the variables through `ChildContext`.
    pub fn read_only_clone(&self) -> Context {
        let mut context = self.child_context_shared(self.matching_nodes.clone());
        context.dont_auto_create = true;
        context
    }

    pub fn writable_clone(&self) -> Context {
        let mut context = self.child_context_shared(self.matching_nodes.clone());
        context.dont_auto_create = false;
        context
    }
}
