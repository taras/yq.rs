//! One module per `operator_*.go` file, plus the dispatch table.

pub mod add;
pub mod assign;
pub mod booleans;
pub mod collect;
pub mod collect_object;
pub mod collections;
pub mod compare;
pub mod control;
pub mod create_map;
pub mod datetime;
pub mod delete;
pub mod divide;
pub mod dtops;
pub mod encoder;
pub mod env;
pub mod equals;
pub mod eval_op;
pub mod formats;
pub mod keys;
pub mod load;
pub mod meta;
pub mod multiply;
pub mod path;
pub mod regex_ops;
pub mod slice;
pub mod strings;
pub mod subtract;
pub mod text;
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

fn system_unavailable(
    _d: &mut Evaluator,
    _context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    Err(Error(
        "system operations are disabled, use --security-enable-system-operator to enable"
            .to_string(),
    ))
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
        OpType::AssignTag => meta::assign_tag_operator,
        OpType::GetTag => meta::get_tag_operator,
        OpType::GetKind => meta::get_kind_operator,
        OpType::AssignStyle => meta::assign_style_operator,
        OpType::GetStyle => meta::get_style_operator,
        OpType::AssignAnchor => meta::assign_anchor_operator,
        OpType::GetAnchor => meta::get_anchor_operator,
        OpType::AssignAlias => meta::assign_alias_operator,
        OpType::GetAlias => meta::get_alias_operator,
        OpType::Explode => meta::explode_operator,
        OpType::AssignComment => meta::assign_comments_operator,
        OpType::GetComment => meta::get_comments_operator,
        OpType::GetDocumentIndex => meta::get_document_index_operator,
        OpType::GetFilename => meta::get_filename_operator,
        OpType::GetFileIndex => meta::get_file_index_operator,
        OpType::Line => meta::line_operator,
        OpType::Column => meta::column_operator,
        OpType::GetParent => meta::get_parent_operator,
        OpType::GetParents => meta::get_parents_operator,
        OpType::SplitDoc => meta::split_document_operator,
        OpType::GetPath => path::get_path_operator,
        OpType::SetPath => path::set_path_operator,
        OpType::DelPaths => path::del_paths_operator,
        OpType::Sort => collections::sort_operator,
        OpType::SortBy => collections::sort_by_operator,
        OpType::SortKeys => collections::sort_keys_operator,
        OpType::Reverse => collections::reverse_operator,
        OpType::Unique => collections::unique_operator,
        OpType::UniqueBy => collections::unique_by_operator,
        OpType::GroupBy => collections::group_by_operator,
        OpType::Flatten => collections::flatten_operator,
        OpType::First => collections::first_operator,
        OpType::Filter => collections::filter_operator,
        OpType::Pick => collections::pick_operator,
        OpType::Omit => collections::omit_operator,
        OpType::Pivot => collections::pivot_operator,
        OpType::Contains => collections::contains_operator,
        OpType::ToNumber => collections::to_number_operator,
        OpType::ToEntries => collections::to_entries_operator,
        OpType::FromEntries => collections::from_entries_operator,
        OpType::WithEntries => collections::with_entries_operator,
        OpType::Encode => encoder::encode_operator,
        OpType::Decode => encoder::decode_operator,
        OpType::Env => env::env_operator,
        OpType::Envsubst => env::envsubst_operator,
        OpType::Now => dtops::now_operator,
        OpType::WithDtf => dtops::with_dtf_operator,
        OpType::FormatDateTime => dtops::format_datetime_operator,
        OpType::Tz => dtops::tz_operator,
        OpType::FromUnix => dtops::from_unix_operator,
        OpType::ToUnix => dtops::to_unix_operator,
        OpType::Eval => eval_op::eval_operator,
        OpType::Trim => text::trim_operator,
        OpType::ToString => text::to_string_operator,
        OpType::ChangeCase => text::change_case_operator,
        OpType::Join => text::join_string_operator,
        OpType::Split => text::split_string_operator,
        OpType::Sub => regex_ops::substitute_operator,
        OpType::Match => regex_ops::match_operator,
        OpType::Capture => regex_ops::capture_operator,
        OpType::Test => regex_ops::test_operator,
        OpType::Load => load::load_operator,
        OpType::LoadString => load::load_string_operator,
        OpType::System => system_unavailable,
        _ => unsupported,
    }
}
