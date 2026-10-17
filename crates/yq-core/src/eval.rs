//! `data_tree_navigator.go` and `operators.go`: dispatch plus the generic
//! helpers (`crossFunction`, `compoundAssignFunction`, traversal trees).

use crate::context::Context;
use crate::node::{Arena, Kind, Node, NodeId, PathElem};
use crate::operation::{ExprNode, MultiplyPrefs, OpType, OpValue, Operation, Prefs, TraversePrefs};
use crate::ops;
use crate::Error;

pub struct Evaluator {
    pub arena: Arena,
    pub env: Vec<(String, String)>,
    pub now: Option<String>,
    /// Guards against runaway recursion from deeply nested expressions.
    pub depth: usize,
}

pub type Handler = fn(&mut Evaluator, Context, &ExprNode) -> Result<Context, Error>;

impl Evaluator {
    pub fn new(arena: Arena) -> Evaluator {
        Evaluator {
            arena,
            env: Vec::new(),
            now: None,
            depth: 0,
        }
    }

    pub fn get_matching_nodes(
        &mut self,
        context: Context,
        expr: &ExprNode,
    ) -> Result<Context, Error> {
        self.depth += 1;
        if self.depth > 2000 {
            self.depth -= 1;
            return Err(Error("expression nesting too deep".to_string()));
        }
        let result = ops::handler(expr.op.op_type)(self, context, expr);
        self.depth -= 1;
        result
    }

    pub fn get_matching_nodes_opt(
        &mut self,
        context: Context,
        expr: Option<&ExprNode>,
    ) -> Result<Context, Error> {
        match expr {
            Some(expr) => self.get_matching_nodes(context, expr),
            None => Ok(context),
        }
    }

    pub fn lhs<'e>(&self, expr: &'e ExprNode) -> Result<&'e ExprNode, Error> {
        expr.lhs().ok_or_else(|| {
            Error(format!(
                "{} is missing its left hand side",
                expr.op.op_type.name()
            ))
        })
    }

    pub fn rhs<'e>(&self, expr: &'e ExprNode) -> Result<&'e ExprNode, Error> {
        expr.rhs().ok_or_else(|| {
            Error(format!(
                "{} is missing its right hand side",
                expr.op.op_type.name()
            ))
        })
    }

    /// `DeeplyAssign`.
    pub fn deeply_assign(
        &mut self,
        context: Context,
        path: &[PathElem],
        rhs: NodeId,
    ) -> Result<(), Error> {
        let mut assignment = Operation::with_prefs(
            OpType::Assign,
            Prefs::Assign(crate::node::AssignPrefs::default()),
        );
        if self.arena.get(rhs).kind == Kind::Mapping {
            assignment = Operation::with_prefs(
                OpType::MultiplyAssign,
                Prefs::Multiply(MultiplyPrefs {
                    append_arrays: true,
                    traverse_prefs: TraversePrefs {
                        dont_follow_alias: true,
                        ..TraversePrefs::default()
                    },
                    ..MultiplyPrefs::default()
                }),
            );
        }
        let rhs_op = Operation {
            node: Some(self.arena.get(rhs).clone()),
            ..Operation::new(OpType::Value)
        };
        let rhs_template = rhs_op;
        let tree = ExprNode::binary(
            assignment,
            create_traversal_tree(path, TraversePrefs::default(), false),
            ExprNode::leaf(rhs_template),
        );
        // The value op copies its template; for a deep assign the template
        // must be the real node so children come along.
        let mut tree = tree;
        if let Some(rhs_node) = tree.rhs.as_mut() {
            rhs_node.op = Operation::reference(rhs);
        }
        self.get_matching_nodes(context, &tree)?;
        Ok(())
    }
}

/// `createBooleanCandidate`.
pub fn create_boolean_candidate(arena: &mut Arena, owner: Option<NodeId>, value: bool) -> NodeId {
    let owner = match owner {
        Some(owner) => owner,
        None => arena.push(Node::default()),
    };
    let text = if value { "true" } else { "false" };
    let result = arena.create_replacement(owner, Kind::Scalar, "!!bool", text);
    if arena.get(owner).is_map_key {
        let node = arena.get_mut(result);
        node.is_map_key = false;
        node.key = Some(owner);
    }
    result
}

/// `createTraversalTree`.
pub fn create_traversal_tree(
    path: &[PathElem],
    traverse_prefs: TraversePrefs,
    target_key: bool,
) -> ExprNode {
    if path.is_empty() {
        return ExprNode::leaf(Operation::new(OpType::SelfReference));
    }
    if path.len() == 1 {
        let mut prefs = traverse_prefs;
        if target_key {
            prefs.include_map_keys = true;
            prefs.dont_include_map_values = true;
        }
        let (value, string_value) = match &path[0] {
            PathElem::Index(index) => (OpValue::Int(*index), index.to_string()),
            PathElem::Key(key) => (OpValue::Str(key.clone()), key.clone()),
        };
        return ExprNode::leaf(Operation {
            op_type: OpType::TraversePath,
            value,
            string_value,
            node: None,
            reference: None,
            prefs: Prefs::Traverse(prefs),
            update_assign: false,
        });
    }
    ExprNode::binary(
        Operation::new(OpType::ShortPipe),
        create_traversal_tree(&path[..1], traverse_prefs, false),
        create_traversal_tree(&path[1..], traverse_prefs, target_key),
    )
}

/// `collectTogether`: every result of `expr` over every node, in one sequence.
pub fn collect_together(
    d: &mut Evaluator,
    context: &Context,
    expr: &ExprNode,
) -> Result<NodeId, Error> {
    let collected = d.arena.push(Node {
        kind: Kind::Sequence,
        tag: "!!seq".to_string(),
        ..Node::default()
    });
    for candidate in context.nodes().to_vec() {
        let results =
            d.get_matching_nodes(context.single_readonly_child_context(candidate), expr)?;
        for result in results.nodes().to_vec() {
            d.arena.add_child(collected, result);
        }
    }
    Ok(collected)
}

pub type Calculation<'f> = &'f dyn Fn(
    &mut Evaluator,
    &Context,
    Option<NodeId>,
    Option<NodeId>,
) -> Result<Option<NodeId>, Error>;
pub type LhsResultValue<'f> =
    &'f dyn Fn(&mut Evaluator, Option<NodeId>) -> Result<Option<NodeId>, Error>;

pub struct CrossPrefs<'f> {
    pub calc_when_empty: bool,
    pub lhs_result_value: Option<LhsResultValue<'f>>,
    pub calculation: Calculation<'f>,
}

fn results_for_rhs(
    d: &mut Evaluator,
    context: &Context,
    lhs_candidate: Option<NodeId>,
    prefs: &CrossPrefs<'_>,
    rhs_exp: &ExprNode,
    results: &mut Vec<NodeId>,
) -> Result<(), Error> {
    if let Some(lhs_result_value) = prefs.lhs_result_value {
        if let Some(result) = lhs_result_value(d, lhs_candidate)? {
            results.push(result);
            return Ok(());
        }
    }
    let rhs = d.get_matching_nodes(context.clone_context(), rhs_exp)?;
    if prefs.calc_when_empty && rhs.is_empty() {
        if let Some(result) = (prefs.calculation)(d, context, lhs_candidate, None)? {
            results.push(result);
        }
        return Ok(());
    }
    for rhs_candidate in rhs.nodes().to_vec() {
        if let Some(result) = (prefs.calculation)(d, context, lhs_candidate, Some(rhs_candidate))? {
            results.push(result);
        }
    }
    Ok(())
}

fn do_cross_func(
    d: &mut Evaluator,
    context: &Context,
    expr: &ExprNode,
    prefs: &CrossPrefs<'_>,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    let lhs_exp = d.lhs(expr)?;
    let rhs_exp = d.rhs(expr)?;
    let lhs = d.get_matching_nodes(context.clone_context(), lhs_exp)?;
    if prefs.calc_when_empty && !context.is_empty() && lhs.is_empty() {
        results_for_rhs(d, context, None, prefs, rhs_exp, &mut results)?;
    }
    for lhs_candidate in lhs.nodes().to_vec() {
        results_for_rhs(
            d,
            context,
            Some(lhs_candidate),
            prefs,
            rhs_exp,
            &mut results,
        )?;
    }
    Ok(context.child_context(results))
}

pub fn cross_function(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
    calculation: Calculation<'_>,
    calc_when_empty: bool,
) -> Result<Context, Error> {
    let prefs = CrossPrefs {
        calc_when_empty,
        lhs_result_value: None,
        calculation,
    };
    cross_function_with_prefs(d, context, expr, &prefs)
}

pub fn evaluate_all_together(arena: &Arena, context: &Context) -> bool {
    context
        .nodes()
        .iter()
        .all(|id| arena.get(*id).evaluate_together)
}

pub fn cross_function_with_prefs(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
    prefs: &CrossPrefs<'_>,
) -> Result<Context, Error> {
    if evaluate_all_together(&d.arena, &context) {
        return do_cross_func(d, &context, expr, prefs);
    }
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let inner = do_cross_func(d, &context.single_child_context(candidate), expr, prefs)?;
        results.extend(inner.nodes().iter().copied());
    }
    Ok(context.child_context(results))
}

pub type CompoundCalculation = fn(&Operation, ExprNode, ExprNode) -> ExprNode;

/// `compoundAssignFunction`: `.a += .b` becomes `.a = (.a + .b)` per match.
pub fn compound_assign_function(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
    calculation: CompoundCalculation,
) -> Result<Context, Error> {
    let lhs_exp = d.lhs(expr)?;
    let rhs_exp = d.rhs(expr)?;
    let lhs = d.get_matching_nodes(context.writable_clone(), lhs_exp)?;
    let prefs = match &expr.op.prefs {
        Prefs::Assign(prefs) => *prefs,
        Prefs::Multiply(multiply) => crate::node::AssignPrefs {
            clobber_custom_tags: multiply.assign_prefs.clobber_custom_tags,
            ..crate::node::AssignPrefs::default()
        },
        _ => crate::node::AssignPrefs::default(),
    };
    let assignment_op = Operation::with_prefs(OpType::Assign, Prefs::Assign(prefs));
    for candidate in lhs.nodes().to_vec() {
        let clone = d.arena.copy(candidate);
        let value_copy_exp = ExprNode::leaf(Operation::reference(clone));
        let value_exp = ExprNode::leaf(Operation::reference(candidate));
        let tree = ExprNode::binary(
            assignment_op.clone(),
            value_exp,
            calculation(&expr.op, value_copy_exp, rhs_exp.clone()),
        );
        d.get_matching_nodes(context.clone_context(), &tree)?;
    }
    Ok(context)
}
