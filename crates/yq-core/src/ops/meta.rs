//! Node metadata operators: tag, kind, style, anchor, alias, explode,
//! comments, document index, filename, file index, line, column, parent
//! and split document.

use crate::context::Context;
use crate::eval::Evaluator;
use crate::node::{
    content_value_by_key, Arena, Kind, Node, NodeId, DOUBLE_QUOTED_STYLE, FLOW_STYLE, FOLDED_STYLE,
    LITERAL_STYLE, SINGLE_QUOTED_STYLE, TAGGED_STYLE,
};
use crate::operation::{ExprNode, Prefs};
use crate::Error;

/// The common shape of `tag = x`, `style = x`, `anchor = x`, `alias = x`:
/// the RHS is read once (or per candidate for `|=`), then applied.
fn assign_attribute(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
    apply: &mut dyn FnMut(&mut Evaluator, NodeId, &str) -> Result<(), Error>,
) -> Result<Context, Error> {
    let mut value = String::new();
    if !expr.op.update_assign {
        let rhs = d.get_matching_nodes_opt(context.read_only_clone(), expr.rhs())?;
        if let Some(first) = rhs.first() {
            value = d.arena.get(first).value.clone();
        }
    }
    let lhs = d.get_matching_nodes_opt(context.writable_clone(), expr.lhs())?;
    for candidate in lhs.nodes().to_vec() {
        if expr.op.update_assign {
            let rhs = d.get_matching_nodes_opt(
                context.single_readonly_child_context(candidate),
                expr.rhs(),
            )?;
            if let Some(first) = rhs.first() {
                value = d.arena.get(first).value.clone();
            }
        }
        apply(d, candidate, &value)?;
    }
    Ok(context)
}

fn replacement_results(
    d: &mut Evaluator,
    context: Context,
    make: &dyn Fn(&Arena, NodeId) -> (String, String),
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let (tag, value) = make(&d.arena, candidate);
        results.push(
            d.arena
                .create_replacement(candidate, Kind::Scalar, &tag, &value),
        );
    }
    Ok(context.child_context(results))
}

pub fn assign_tag_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    assign_attribute(d, context, expr, &mut |d, candidate, value| {
        d.arena.get_mut(candidate).tag = value.to_string();
        Ok(())
    })
}

pub fn get_tag_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    replacement_results(d, context, &|arena, id| {
        ("!!str".to_string(), arena.get(id).tag.clone())
    })
}

pub fn get_kind_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    replacement_results(d, context, &|arena, id| {
        let text = match arena.get(id).kind {
            Kind::Mapping => "map",
            Kind::Sequence => "seq",
            Kind::Scalar => "scalar",
            Kind::Alias => "alias",
        };
        ("!!str".to_string(), text.to_string())
    })
}

fn parse_style(text: &str) -> Result<u32, Error> {
    match text {
        "tagged" => Ok(TAGGED_STYLE),
        "double" => Ok(DOUBLE_QUOTED_STYLE),
        "single" => Ok(SINGLE_QUOTED_STYLE),
        "literal" => Ok(LITERAL_STYLE),
        "folded" => Ok(FOLDED_STYLE),
        "flow" => Ok(FLOW_STYLE),
        "" => Ok(0),
        other => Err(Error(format!("unknown style {other}"))),
    }
}

pub fn assign_style_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    assign_attribute(d, context, expr, &mut |d, candidate, value| {
        d.arena.get_mut(candidate).style = parse_style(value)?;
        Ok(())
    })
}

pub fn get_style_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    replacement_results(d, context, &|arena, id| {
        let text = match arena.get(id).style {
            TAGGED_STYLE => "tagged",
            DOUBLE_QUOTED_STYLE => "double",
            SINGLE_QUOTED_STYLE => "single",
            LITERAL_STYLE => "literal",
            FOLDED_STYLE => "folded",
            FLOW_STYLE => "flow",
            0 => "",
            _ => "<unknown>",
        };
        ("!!str".to_string(), text.to_string())
    })
}

pub fn assign_anchor_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    assign_attribute(d, context, expr, &mut |d, candidate, value| {
        d.arena.get_mut(candidate).anchor = value.to_string();
        Ok(())
    })
}

pub fn get_anchor_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    replacement_results(d, context, &|arena, id| {
        ("!!str".to_string(), arena.get(id).anchor.clone())
    })
}

pub fn assign_alias_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    assign_attribute(d, context, expr, &mut |d, candidate, value| {
        if !value.is_empty() {
            let node = d.arena.get_mut(candidate);
            node.kind = Kind::Alias;
            node.value = value.to_string();
        }
        Ok(())
    })
}

pub fn get_alias_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    replacement_results(d, context, &|arena, id| {
        ("!!str".to_string(), arena.get(id).value.clone())
    })
}

pub fn explode_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let fix = d.fix_merge_anchor_to_spec;
    for candidate in context.nodes().to_vec() {
        let rhs = d.get_matching_nodes_opt(context.single_child_context(candidate), expr.rhs())?;
        for child in rhs.nodes().to_vec() {
            explode_node(&mut d.arena, child, fix)?;
        }
    }
    Ok(context)
}

/// `explodeNode`: resolves aliases and merge keys in place.
pub fn explode_node(arena: &mut Arena, node: NodeId, fix: bool) -> Result<(), Error> {
    arena.get_mut(node).anchor = String::new();
    match arena.get(node).kind {
        Kind::Sequence => {
            for child in arena.get(node).content.clone() {
                explode_node(arena, child, fix)?;
            }
            Ok(())
        }
        Kind::Alias => {
            if let Some(target) = arena.get(node).alias {
                let target_node = arena.get(target).clone();
                {
                    let current = arena.get_mut(node);
                    current.kind = target_node.kind;
                    current.style = target_node.style;
                    current.tag = target_node.tag;
                }
                arena.add_children(node, &target_node.content);
                let current = arena.get_mut(node);
                current.value = target_node.value;
                current.alias = None;
            }
            Ok(())
        }
        Kind::Mapping => {
            let content = arena.get(node).content.clone();
            let has_alias = content
                .iter()
                .step_by(2)
                .any(|key| arena.get(*key).value == "<<");
            if has_alias {
                if fix {
                    return fixed_reconstruct_aliased_map(arena, node);
                }
                return reconstruct_aliased_map(arena, node);
            }
            let mut index = 0;
            while index + 1 < content.len() {
                explode_node(arena, content[index], fix)?;
                explode_node(arena, content[index + 1], fix)?;
                index += 2;
            }
            Ok(())
        }
        Kind::Scalar => Ok(()),
    }
}

fn reconstruct_aliased_map(arena: &mut Arena, node: NodeId) -> Result<(), Error> {
    let mut new_content: Vec<NodeId> = Vec::new();
    let content = arena.get(node).content.clone();
    let mut index = 0;
    while index + 1 < content.len() {
        let key = content[index];
        let value = content[index + 1];
        if arena.get(key).value != "<<" {
            override_entry(arena, node, key, value, index, &mut new_content)?;
        } else if arena.get(value).kind == Kind::Sequence {
            let aliases = arena.get(value).content.clone();
            // Go passes the position within the merge list, not within the
            // map (a shadowed loop variable); the output order depends on it.
            for (alias_index, alias_node) in aliases.iter().enumerate().rev() {
                let target = arena.get(*alias_node).alias;
                apply_alias(arena, node, target, alias_index, &mut new_content)?;
            }
        } else {
            let target = arena.get(value).alias;
            apply_alias(arena, node, target, index, &mut new_content)?;
        }
        index += 2;
    }
    arena.get_mut(node).content = Vec::new();
    let mut i = 0;
    while i + 1 < new_content.len() {
        arena.add_key_value_child(node, new_content[i], new_content[i + 1]);
        i += 2;
    }
    Ok(())
}

fn apply_alias(
    arena: &mut Arena,
    node: NodeId,
    alias: Option<NodeId>,
    alias_index: usize,
    new_content: &mut Vec<NodeId>,
) -> Result<(), Error> {
    let alias = match alias {
        Some(alias) => alias,
        None => return Ok(()),
    };
    if arena.get(alias).kind != Kind::Mapping {
        return Err(Error(format!(
            "can only use merge anchors with maps (!!map) or sequences (!!seq) of maps, but got sequence containing {}",
            arena.get(alias).tag
        )));
    }
    let content = arena.get(alias).content.clone();
    let mut index = 0;
    while index + 1 < content.len() {
        override_entry(
            arena,
            node,
            content[index],
            content[index + 1],
            alias_index,
            new_content,
        )?;
        index += 2;
    }
    Ok(())
}

fn override_entry(
    arena: &mut Arena,
    node: NodeId,
    key: NodeId,
    value: NodeId,
    start_index: usize,
    new_content: &mut Vec<NodeId>,
) -> Result<(), Error> {
    explode_node(arena, value, false)?;
    let key_value = arena.get(key).value.clone();
    let key_has_alias = arena.get(key).alias.is_some();
    let mut i = 0;
    while i + 1 < new_content.len() {
        let existing_key = new_content[i];
        if arena.get(existing_key).value == key_value
            && arena.get(existing_key).alias.is_none()
            && !key_has_alias
        {
            new_content[i + 1] = value;
            return Ok(());
        }
        i += 2;
    }
    let content = arena.get(node).content.clone();
    let mut index = start_index + 2;
    while index < content.len() {
        let later_key = content[index];
        if arena.get(later_key).value == key_value && arena.get(later_key).alias.is_none() {
            return Ok(());
        }
        index += 2;
    }
    explode_node(arena, key, false)?;
    new_content.push(key);
    new_content.push(value);
    Ok(())
}

/// `fixedReconstructAliasedMap`, used when merge anchors follow the spec.
fn fixed_reconstruct_aliased_map(arena: &mut Arena, node: NodeId) -> Result<(), Error> {
    let mut new_content: Vec<NodeId> = Vec::new();
    let content = arena.get(node).content.clone();
    let mut index = 0;
    while index + 1 < content.len() {
        let key = content[index];
        let value = content[index + 1];
        if arena.get(key).tag != "!!merge" {
            explode_node(arena, key, true)?;
            explode_node(arena, value, true)?;
            new_content.push(key);
            new_content.push(value);
        } else {
            let mut sequence = value;
            if arena.get(sequence).kind == Kind::Alias {
                sequence = arena.get(sequence).alias.unwrap_or(sequence);
            }
            let items = if arena.get(sequence).kind == Kind::Sequence {
                arena.get(sequence).content.clone()
            } else {
                vec![sequence]
            };
            for item in items {
                let mut merge_node = item;
                if arena.get(merge_node).kind == Kind::Alias {
                    merge_node = arena.get(merge_node).alias.unwrap_or(merge_node);
                }
                let merge_node = arena.copy(merge_node);
                explode_node(arena, merge_node, true)?;
                if arena.get(merge_node).kind != Kind::Mapping {
                    return Err(Error(format!(
                        "can only use merge anchors with maps (!!map) or sequences (!!seq) of maps, but got sequence containing {}",
                        arena.get(merge_node).tag
                    )));
                }
                let merge_content = arena.get(merge_node).content.clone();
                let mut j = 0;
                while j + 1 < merge_content.len() {
                    let merge_key = merge_content[j];
                    let key_text = arena.get(merge_key).value.clone();
                    let in_node = content_value_by_key(arena, node, &key_text).is_some();
                    let in_new = {
                        let mut found = false;
                        let mut k = 0;
                        while k + 1 < new_content.len() {
                            if arena.get(new_content[k]).value == key_text {
                                found = true;
                                break;
                            }
                            k += 2;
                        }
                        found
                    };
                    if !in_node && !in_new {
                        let copied_key = arena.copy(merge_key);
                        let copied_value = arena.copy(merge_content[j + 1]);
                        arena.get_mut(copied_key).parent = Some(node);
                        arena.get_mut(copied_value).parent = Some(node);
                        new_content.push(copied_key);
                        new_content.push(copied_value);
                    }
                    j += 2;
                }
            }
        }
        index += 2;
    }
    arena.get_mut(node).content = new_content;
    Ok(())
}

fn comment_prefs(prefs: &Prefs) -> (bool, bool, bool) {
    match prefs {
        Prefs::Comment { line, head, foot } => (*line, *head, *foot),
        _ => (false, false, false),
    }
}

pub fn assign_comments_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let lhs = d.get_matching_nodes_opt(context.writable_clone(), expr.lhs())?;
    let (line, head, foot) = comment_prefs(&expr.op.prefs);
    let mut comment = String::new();
    if !expr.op.update_assign {
        let rhs = d.get_matching_nodes_opt(context.read_only_clone(), expr.rhs())?;
        if let Some(first) = rhs.first() {
            comment = d.arena.get(first).value.clone();
        }
    }
    for candidate in lhs.nodes().to_vec() {
        if expr.op.update_assign {
            let rhs = d.get_matching_nodes_opt(
                context.single_readonly_child_context(candidate),
                expr.rhs(),
            )?;
            if let Some(first) = rhs.first() {
                comment = d.arena.get(first).value.clone();
            }
        }
        let node = d.arena.get_mut(candidate);
        if line {
            node.line_comment = comment.clone();
        }
        if head {
            node.head_comment = comment.clone();
            node.leading_content = String::new();
        }
        if foot {
            node.foot_comment = comment.clone();
        }
    }
    Ok(context)
}

pub fn get_comments_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let (line, head, foot) = comment_prefs(&expr.op.prefs);
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let node = d.arena.get(candidate).clone();
        let mut comment = String::new();
        if line {
            comment = node.line_comment.clone();
        } else if head && !node.leading_content.is_empty() {
            comment = crate::yaml::encode::leading_content(&node.leading_content, false);
            if let Some(stripped) = comment.strip_suffix('\n') {
                comment = stripped.to_string();
            }
        } else if head {
            comment = node.head_comment.clone();
        } else if foot {
            comment = node.foot_comment.clone();
        }
        if let Some(stripped) = comment.strip_prefix("# ") {
            comment = stripped.to_string();
        }
        comment = comment.replace("\n# ", "\n");
        let result = d
            .arena
            .create_replacement(candidate, Kind::Scalar, "!!str", &comment);
        if node.is_map_key {
            let result_node = d.arena.get_mut(result);
            result_node.is_map_key = false;
            result_node.key = Some(candidate);
        }
        results.push(result);
    }
    Ok(context.child_context(results))
}

pub fn get_document_index_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    replacement_results(d, context, &|arena, id| {
        ("!!int".to_string(), arena.get_document(id).to_string())
    })
}

pub fn get_filename_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    replacement_results(d, context, &|arena, id| {
        ("!!str".to_string(), arena.get_filename(id).to_string())
    })
}

pub fn get_file_index_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    replacement_results(d, context, &|arena, id| {
        ("!!int".to_string(), arena.get_file_index(id).to_string())
    })
}

pub fn line_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    replacement_results(d, context, &|arena, id| {
        ("!!int".to_string(), arena.get(id).line.to_string())
    })
}

pub fn column_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    replacement_results(d, context, &|arena, id| {
        ("!!int".to_string(), arena.get(id).column.to_string())
    })
}

pub fn get_parents_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let list = d.arena.push(Node {
            kind: Kind::Sequence,
            tag: "!!seq".to_string(),
            ..Node::default()
        });
        let mut parent = d.arena.get(candidate).parent;
        while let Some(id) = parent {
            d.arena.add_child(list, id);
            parent = d.arena.get(id).parent;
        }
        results.push(list);
    }
    Ok(context.child_context(results))
}

pub fn get_parent_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let level = match expr.op.prefs {
        Prefs::Parent { level } => level,
        _ => 1,
    };
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let mut levels_to_go_up = level;
        if level < 0 {
            let mut total = 0i64;
            let mut temp = d.arena.get(candidate).parent;
            while let Some(id) = temp {
                total += 1;
                temp = d.arena.get(id).parent;
            }
            levels_to_go_up = (total + level + 1).max(0);
        }
        let mut current = Some(candidate);
        let mut current_level = 0;
        while current_level < levels_to_go_up {
            current = match current {
                Some(id) => d.arena.get(id).parent,
                None => None,
            };
            current_level += 1;
            if current.is_none() {
                break;
            }
        }
        if let Some(id) = current {
            results.push(id);
        }
    }
    Ok(context.child_context(results))
}

pub fn split_document_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    for (index, candidate) in context.nodes().to_vec().into_iter().enumerate() {
        let node = d.arena.get_mut(candidate);
        node.document = index;
        node.parent = None;
    }
    Ok(context)
}
