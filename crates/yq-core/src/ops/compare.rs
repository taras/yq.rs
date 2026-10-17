//! `operator_compare.go`.

use crate::context::Context;
use crate::eval::{create_boolean_candidate, cross_function, Evaluator};
use crate::node::{Kind, NodeId};
use crate::operation::{ExprNode, Prefs, TraversePrefs};
use crate::ops::traverse::splat;
use crate::util::{is_truthy_node, parse_float, parse_int64};
use crate::Error;

#[derive(Clone, Copy)]
pub struct ComparePrefs {
    pub or_equal: bool,
    pub greater: bool,
}

fn compare_scalars(
    d: &mut Evaluator,
    context: &Context,
    prefs: ComparePrefs,
    lhs: NodeId,
    rhs: NodeId,
) -> Result<bool, Error> {
    let lhs_tag = d.arena.guess_tag_from_custom_type(lhs);
    let rhs_tag = d.arena.guess_tag_from_custom_type(rhs);
    let lhs_value = d.arena.get(lhs).value.clone();
    let rhs_value = d.arena.get(rhs).value.clone();
    let mut is_datetime = d.arena.get(lhs).tag == "!!timestamp";
    if lhs_tag == "!!str" {
        is_datetime =
            crate::ops::datetime::parse_datetime(context.datetime_layout(), &lhs_value).is_ok();
    }
    if is_datetime {
        let layout = context.datetime_layout().to_string();
        let lhs_time = crate::ops::datetime::parse_datetime(&layout, &lhs_value)?;
        let rhs_time = crate::ops::datetime::parse_datetime(&layout, &rhs_value)?;
        if prefs.or_equal && lhs_time == rhs_time {
            return Ok(true);
        }
        if prefs.greater {
            return Ok(lhs_time > rhs_time);
        }
        return Ok(lhs_time < rhs_time);
    }
    if lhs_tag == "!!int" && rhs_tag == "!!int" {
        let (_, lhs_num) = parse_int64(&lhs_value)?;
        let (_, rhs_num) = parse_int64(&rhs_value)?;
        if prefs.or_equal && lhs_num == rhs_num {
            return Ok(true);
        }
        return Ok(if prefs.greater {
            lhs_num > rhs_num
        } else {
            lhs_num < rhs_num
        });
    }
    let numeric = |tag: &str| tag == "!!int" || tag == "!!float";
    if numeric(&lhs_tag) && numeric(&rhs_tag) {
        let lhs_num = parse_float(&lhs_value)?;
        let rhs_num = parse_float(&rhs_value)?;
        if prefs.or_equal && lhs_num == rhs_num {
            return Ok(true);
        }
        return Ok(if prefs.greater {
            lhs_num > rhs_num
        } else {
            lhs_num < rhs_num
        });
    }
    if lhs_tag == "!!str" && rhs_tag == "!!str" {
        if prefs.or_equal && lhs_value == rhs_value {
            return Ok(true);
        }
        return Ok(if prefs.greater {
            lhs_value > rhs_value
        } else {
            lhs_value < rhs_value
        });
    }
    if lhs_tag == "!!null" && rhs_tag == "!!null" && prefs.or_equal {
        return Ok(true);
    }
    if lhs_tag == "!!null" || rhs_tag == "!!null" {
        return Ok(false);
    }
    Err(Error(format!(
        "{} not yet supported for comparison",
        d.arena.get(lhs).tag
    )))
}

pub fn compare_nodes(
    d: &mut Evaluator,
    context: &Context,
    prefs: ComparePrefs,
    lhs: Option<NodeId>,
    rhs: Option<NodeId>,
) -> Result<Option<NodeId>, Error> {
    let (lhs, rhs) = match (lhs, rhs) {
        (None, None) => {
            return Ok(Some(create_boolean_candidate(
                &mut d.arena,
                None,
                prefs.or_equal,
            )));
        }
        (None, Some(rhs)) => {
            return Ok(Some(create_boolean_candidate(
                &mut d.arena,
                Some(rhs),
                false,
            )));
        }
        (Some(lhs), None) => {
            return Ok(Some(create_boolean_candidate(
                &mut d.arena,
                Some(lhs),
                false,
            )));
        }
        (Some(lhs), Some(rhs)) => (lhs, rhs),
    };
    match d.arena.get(lhs).kind {
        Kind::Mapping => Err(Error("maps not yet supported for comparison".to_string())),
        Kind::Sequence => Err(Error("arrays not yet supported for comparison".to_string())),
        _ => {
            if d.arena.get(rhs).kind != Kind::Scalar {
                return Err(Error(format!(
                    "{} ({}) cannot be subtracted from {}",
                    d.arena.get(rhs).tag,
                    d.arena.get_nice_path(rhs),
                    d.arena.get(lhs).tag
                )));
            }
            let target = d.arena.copy_without_content(lhs);
            let value = compare_scalars(d, context, prefs, lhs, rhs)?;
            Ok(Some(create_boolean_candidate(
                &mut d.arena,
                Some(target),
                value,
            )))
        }
    }
}

pub fn compare_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let prefs = match expr.op.prefs {
        Prefs::Compare { or_equal, greater } => ComparePrefs { or_equal, greater },
        _ => ComparePrefs {
            or_equal: false,
            greater: false,
        },
    };
    let calculation =
        move |d: &mut Evaluator, context: &Context, lhs: Option<NodeId>, rhs: Option<NodeId>| {
            compare_nodes(d, context, prefs, lhs, rhs)
        };
    cross_function(d, context, expr, &calculation, true)
}

fn superlative(d: &mut Evaluator, context: Context, prefs: ComparePrefs) -> Result<Context, Error> {
    let mut results = Vec::new();
    for sequence in context.nodes().to_vec() {
        let splatted = splat(
            d,
            context.single_child_context(sequence),
            TraversePrefs::default(),
        )?;
        let nodes = splatted.nodes().to_vec();
        if let Some(mut result) = nodes.first().copied() {
            for candidate in nodes.iter().skip(1) {
                let cmp = compare_nodes(d, &context, prefs, Some(*candidate), Some(result))?;
                if cmp.is_some_and(|id| is_truthy_node(d.arena.get(id))) {
                    result = *candidate;
                }
            }
            results.push(result);
        }
    }
    Ok(context.child_context(results))
}

pub fn min_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    superlative(
        d,
        context,
        ComparePrefs {
            or_equal: false,
            greater: false,
        },
    )
}

pub fn max_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    superlative(
        d,
        context,
        ComparePrefs {
            or_equal: false,
            greater: true,
        },
    )
}
