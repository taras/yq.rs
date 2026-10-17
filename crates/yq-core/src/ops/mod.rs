//! One module per `operator_*.go` file, plus the dispatch table.

pub mod add;
pub mod assign;
pub mod booleans;
pub mod collect;
pub mod collect_object;
pub mod compare;
pub mod control;
pub mod create_map;
pub mod datetime;
pub mod delete;
pub mod divide;
pub mod equals;
pub mod keys;
pub mod multiply;
pub mod slice;
pub mod strings;
pub mod subtract;
pub mod traverse;
pub mod variables;

use crate::context::Context;
use crate::eval::{Evaluator, Handler};
use crate::operation::{ExprNode, OpType};
use crate::Error;

fn unsupported(_d: &mut Evaluator, _context: Context, expr: &ExprNode) -> Result<Context, Error> {
    Err(Error(format!(
        "operator {} is not supported yet",
        expr.op.op_type.name()
    )))
}

pub fn handler(op_type: OpType) -> Handler {
    match op_type {
        OpType::Or => booleans::or_operator,
        OpType::And => booleans::and_operator,
        OpType::Not => booleans::not_operator,
        OpType::Any | OpType::AnyCondition => booleans::any_operator,
        OpType::All | OpType::AllCondition => booleans::all_operator,
        OpType::Reduce => variables::reduce_operator,
        OpType::IfThen => control::if_then_operator,
        OpType::IfElse => control::if_else_operator,
        OpType::Block | OpType::Empty => control::empty_operator,
        OpType::Union => control::union_operator,
        OpType::Pipe | OpType::ShortPipe => control::pipe_operator,
        OpType::Assign => assign::assign_update_operator,
        OpType::AssignAttributes => assign::assign_attributes_operator,
        OpType::AddAssign => add::add_assign_operator,
        OpType::SubtractAssign => subtract::subtract_assign_operator,
        OpType::AssignVariable => variables::use_with_pipe,
        OpType::Multiply => multiply::multiply_operator,
        OpType::MultiplyAssign => multiply::multiply_assign_operator,
        OpType::Divide => divide::divide_operator,
        OpType::Modulo => divide::modulo_operator,
        OpType::Add => add::add_operator,
        OpType::Subtract => subtract::subtract_operator,
        OpType::Alternative => booleans::alternative_operator,
        OpType::Equals => equals::equals_operator,
        OpType::NotEquals => equals::not_equals_operator,
        OpType::Compare => compare::compare_operator,
        OpType::Min => compare::min_operator,
        OpType::Max => compare::max_operator,
        OpType::CreateMap => create_map::create_map_operator,
        OpType::Length => control::length_operator,
        OpType::Expression => control::expression_operator,
        OpType::Collect => collect::collect_operator,
        OpType::CollectObject => collect_object::collect_object_operator,
        OpType::Map => control::map_operator,
        OpType::MapValues => control::map_values_operator,
        OpType::Error => control::error_operator,
        OpType::With => control::with_operator,
        OpType::GetVariable => variables::get_variable_operator,
        OpType::GetKey => keys::get_key_operator,
        OpType::IsKey => keys::is_key_operator,
        OpType::Keys => keys::keys_operator,
        OpType::Has => keys::has_operator,
        OpType::StringInterpolation => strings::string_interpolation_operator,
        OpType::TraversePath => traverse::traverse_path_operator,
        OpType::TraverseArray => traverse::traverse_array_operator,
        OpType::SelfReference => control::self_operator,
        OpType::Value => control::value_operator,
        OpType::Reference => control::reference_operator,
        OpType::RecursiveDescent => traverse::recursive_descent_operator,
        OpType::Select => control::select_operator,
        OpType::Delete => delete::delete_child_operator,
        _ => unsupported,
    }
}
