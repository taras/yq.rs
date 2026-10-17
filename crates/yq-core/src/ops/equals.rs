//! `operator_equals.go`.

use crate::context::Context;
use crate::eval::{create_boolean_candidate, cross_function, Evaluator};
use crate::node::{Kind, NodeId};
use crate::operation::ExprNode;
use crate::ops::traverse::match_key;
use crate::Error;

fn is_equals(
    flip: bool,
) -> impl Fn(&mut Evaluator, &Context, Option<NodeId>, Option<NodeId>) -> Result<Option<NodeId>, Error>
{
    move |d, _context, lhs, rhs| {
        let (lhs, rhs) = match (lhs, rhs) {
            (None, None) => {
                return Ok(Some(create_boolean_candidate(&mut d.arena, None, !flip)));
            }
            (None, Some(rhs)) => {
                let mut value = d.arena.get(rhs).tag == "!!null";
                if flip {
                    value = !value;
                }
                return Ok(Some(create_boolean_candidate(
                    &mut d.arena,
                    Some(rhs),
                    value,
                )));
            }
            (Some(lhs), None) => {
                let mut value = d.arena.get(lhs).tag == "!!null";
                if flip {
                    value = !value;
                }
                return Ok(Some(create_boolean_candidate(
                    &mut d.arena,
                    Some(lhs),
                    value,
                )));
            }
            (Some(lhs), Some(rhs)) => (lhs, rhs),
        };
        let lhs_node = d.arena.get(lhs);
        let rhs_node = d.arena.get(rhs);
        let mut value = false;
        if lhs_node.tag == "!!null" {
            value = rhs_node.tag == "!!null";
        } else if lhs_node.kind == Kind::Scalar && rhs_node.kind == Kind::Scalar {
            value = match_key(&lhs_node.value, &rhs_node.value);
        }
        if flip {
            value = !value;
        }
        Ok(Some(create_boolean_candidate(
            &mut d.arena,
            Some(lhs),
            value,
        )))
    }
}

pub fn equals_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    cross_function(d, context, expr, &is_equals(false), true)
}

pub fn not_equals_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    cross_function(d, context.read_only_clone(), expr, &is_equals(true), true)
}
