//! `lexer_participle.go` and `lexer.go`: the ordered rule table, token
//! creation and the post-processing pass that inserts implied operators.

pub mod regex;

use crate::node::AssignPrefs;
use crate::operation::{
    EncodeFormat, MultiplyPrefs, OpType, OpValue, Operation, Prefs, RecursiveDescentPrefs,
    TraversePrefs,
};
use crate::util::process_escape_characters;
use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Operation,
    OpenBracket,
    CloseBracket,
    OpenCollect,
    CloseCollect,
    OpenCollectObject,
    CloseCollectObject,
    TraverseArrayCollect,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub operation: Option<Operation>,
    pub assign_operation: Option<Operation>,
    pub check_for_post_traverse: bool,
    pub match_text: String,
    pub expected_closer: String,
}

impl Token {
    fn literal(token_type: TokenType, check_for_post: bool, text: &str) -> Token {
        Token {
            token_type,
            operation: None,
            assign_operation: None,
            check_for_post_traverse: check_for_post,
            match_text: text.to_string(),
            expected_closer: String::new(),
        }
    }

    fn operation(op: Operation) -> Token {
        let check = op.op_type.info().check_for_post_traverse;
        Token {
            token_type: TokenType::Operation,
            operation: Some(op),
            assign_operation: None,
            check_for_post_traverse: check,
            match_text: String::new(),
            expected_closer: String::new(),
        }
    }

    pub fn is_op(&self, op_type: OpType) -> bool {
        self.token_type == TokenType::Operation
            && self
                .operation
                .as_ref()
                .is_some_and(|op| op.op_type == op_type)
    }

    pub fn describe(&self) -> String {
        match self.token_type {
            TokenType::Operation => self
                .operation
                .as_ref()
                .map(|op| op.op_type.name().to_string())
                .unwrap_or_default(),
            TokenType::OpenBracket => "(".to_string(),
            TokenType::CloseBracket => ")".to_string(),
            TokenType::OpenCollect => "[".to_string(),
            TokenType::CloseCollect => "]".to_string(),
            TokenType::OpenCollectObject => "{".to_string(),
            TokenType::CloseCollectObject => "}".to_string(),
            TokenType::TraverseArrayCollect => ".[".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
enum Action {
    Skip,
    Literal(TokenType, bool),
    LiteralWithCloser(TokenType, bool, &'static str),
    Op(OpType),
    OpPrefs(OpType, Prefs),
    Assignable(OpType, OpType, Prefs),
    Path(bool),
    RecursiveDescent(bool),
    Expression(&'static str),
    FlattenDepth,
    AssignAllComments(bool),
    AssignOp(bool),
    Bool(bool),
    Null,
    StringValue,
    Env(bool),
    EnvSubstOptions,
    MultiplyPrefs(OpType),
    GetVariable,
    Hex,
    Float,
    Number,
    ParentLevel,
    ParentDefault,
    EncodeIndent(EncodeFormat),
    EncodeWithIndent(EncodeFormat, usize),
    Decode(EncodeFormat),
    Load(EncodeFormat),
}

struct Rule {
    pattern: Vec<regex::Re>,
    action: Action,
}

fn comment_prefs(line: bool, head: bool, foot: bool) -> Prefs {
    Prefs::Comment { line, head, foot }
}

fn rules() -> Vec<Rule> {
    use Action::*;
    use OpType as T;
    let table: Vec<(&str, Action)> = vec![
        (
            r"line_?comment|lineComment",
            Assignable(
                T::GetComment,
                T::AssignComment,
                comment_prefs(true, false, false),
            ),
        ),
        (
            r"head_?comment|headComment",
            Assignable(
                T::GetComment,
                T::AssignComment,
                comment_prefs(false, true, false),
            ),
        ),
        (
            r"foot_?comment|footComment",
            Assignable(
                T::GetComment,
                T::AssignComment,
                comment_prefs(false, false, true),
            ),
        ),
        (r"\(", Literal(TokenType::OpenBracket, false)),
        (r"\)", Literal(TokenType::CloseBracket, true)),
        (r"\.\[", Literal(TokenType::TraverseArrayCollect, false)),
        (r"\[", Literal(TokenType::OpenCollect, false)),
        (r"\]\??", Literal(TokenType::CloseCollect, true)),
        (r"\{", Literal(TokenType::OpenCollectObject, false)),
        (r"\}", Literal(TokenType::CloseCollectObject, true)),
        (r"\.\.\.", RecursiveDescent(true)),
        (r"\.\.", RecursiveDescent(false)),
        (r"\$[a-zA-Z_\-0-9]+", GetVariable),
        (
            r"as",
            OpPrefs(
                T::AssignVariable,
                Prefs::AssignVar {
                    is_reference: false,
                },
            ),
        ),
        (
            r"ref",
            OpPrefs(T::AssignVariable, Prefs::AssignVar { is_reference: true }),
        ),
        (r":\s*", Op(T::CreateMap)),
        (r"length", Op(T::Length)),
        (r"line", Op(T::Line)),
        (r"column", Op(T::Column)),
        (r"eval", Op(T::Eval)),
        (r"to_?number", Op(T::ToNumber)),
        (r"map_?values", Op(T::MapValues)),
        (r"map", Op(T::Map)),
        (r"filter", Op(T::Filter)),
        (r"pick", Op(T::Pick)),
        (r"omit", Op(T::Omit)),
        (r"flatten\([0-9]+\)", FlattenDepth),
        (
            r"flatten",
            OpPrefs(T::Flatten, Prefs::Flatten { depth: -1 }),
        ),
        (r"format_datetime", Op(T::FormatDateTime)),
        (r"now", Op(T::Now)),
        (r"tz", Op(T::Tz)),
        (r"from_?unix", Op(T::FromUnix)),
        (r"to_?unix", Op(T::ToUnix)),
        (r"with_dtf", Op(T::WithDtf)),
        (r"error", Op(T::Error)),
        (r"shuffle", Op(T::Shuffle)),
        (r"sortKeys", Op(T::SortKeys)),
        (r"sort_?keys", Op(T::SortKeys)),
        (
            r"array_?to_?map",
            Expression(r"(.[] | select(. != null) ) as $i ireduce({}; .[$i | key] = $i)"),
        ),
        (r"root", Expression(r"parent(-1)")),
        (r"to_?yaml\([0-9]+\)", EncodeIndent(EncodeFormat::Yaml)),
        (r"to_?xml\([0-9]+\)", EncodeIndent(EncodeFormat::Xml)),
        (r"to_?json\([0-9]+\)", EncodeIndent(EncodeFormat::Json)),
        (
            r"from_?yaml|@yamld|from_?json|@jsond",
            Decode(EncodeFormat::Yaml),
        ),
        (r"to_?yaml|@yaml", EncodeWithIndent(EncodeFormat::Yaml, 2)),
        (r"to_?json", EncodeWithIndent(EncodeFormat::Json, 2)),
        (r"@json", EncodeWithIndent(EncodeFormat::Json, 0)),
        (r"from_?props|@propsd", Decode(EncodeFormat::Props)),
        (
            r"to_?props|@props",
            EncodeWithIndent(EncodeFormat::Props, 2),
        ),
        (r"from_?xml|@xmld", Decode(EncodeFormat::Xml)),
        (r"to_?xml", EncodeWithIndent(EncodeFormat::Xml, 2)),
        (r"@xml", EncodeWithIndent(EncodeFormat::Xml, 0)),
        (r"from_?csv|@csvd", Decode(EncodeFormat::Csv)),
        (r"to_?csv|@csv", EncodeWithIndent(EncodeFormat::Csv, 0)),
        (r"from_?tsv|@tsvd", Decode(EncodeFormat::Tsv)),
        (r"to_?tsv|@tsv", EncodeWithIndent(EncodeFormat::Tsv, 0)),
        (r"@base64urld", Decode(EncodeFormat::Base64Url)),
        (r"@base64url", EncodeWithIndent(EncodeFormat::Base64Url, 0)),
        (r"@base64d", Decode(EncodeFormat::Base64)),
        (r"@base64", EncodeWithIndent(EncodeFormat::Base64, 0)),
        (r"@urid", Decode(EncodeFormat::Uri)),
        (r"@uri", EncodeWithIndent(EncodeFormat::Uri, 0)),
        (r"@sh", EncodeWithIndent(EncodeFormat::Sh, 0)),
        (r"load_?xml|xml_?load", Load(EncodeFormat::Xml)),
        (r"load_?base64", Load(EncodeFormat::Base64)),
        (r"load_?props", Load(EncodeFormat::Props)),
        (r"load_?str|str_?load", Op(T::LoadString)),
        (r"load", Load(EncodeFormat::Yaml)),
        (r"system", Op(T::System)),
        (r"splitDoc|split_?doc", Op(T::SplitDoc)),
        (r"select", Op(T::Select)),
        (r"has", Op(T::Has)),
        (r"unique_?by", Op(T::UniqueBy)),
        (r"unique", Op(T::Unique)),
        (r"group_?by", Op(T::GroupBy)),
        (r"explode", Op(T::Explode)),
        (r"or", Op(T::Or)),
        (r"and", Op(T::And)),
        (r"not", Op(T::Not)),
        (r"ireduce", Op(T::Reduce)),
        (r"join", Op(T::Join)),
        (r"sub", Op(T::Sub)),
        (r"match", Op(T::Match)),
        (r"capture", Op(T::Capture)),
        (r"test", Op(T::Test)),
        (r"sort_?by", Op(T::SortBy)),
        (r"sort", Op(T::Sort)),
        (r"first", Op(T::First)),
        (r"reverse", Op(T::Reverse)),
        (r"any_c", Op(T::AnyCondition)),
        (r"any", Op(T::Any)),
        (r"all_c", Op(T::AllCondition)),
        (r"all", Op(T::All)),
        (r"contains", Op(T::Contains)),
        (r"split", Op(T::Split)),
        (r"parents", Op(T::GetParents)),
        (r"parent\(-?[0-9]+\)", ParentLevel),
        (r"parent", ParentDefault),
        (r"keys", Op(T::Keys)),
        (r"key", Op(T::GetKey)),
        (r"is_?key", Op(T::IsKey)),
        (r"file_?name|fileName", Op(T::GetFilename)),
        (r"file_?index|fileIndex|fi", Op(T::GetFileIndex)),
        (r"path", Op(T::GetPath)),
        (r"set_?path", Op(T::SetPath)),
        (r"del_?paths", Op(T::DelPaths)),
        (r"to_?entries|toEntries", Op(T::ToEntries)),
        (r"from_?entries|fromEntries", Op(T::FromEntries)),
        (r"with_?entries|withEntries", Op(T::WithEntries)),
        (r"with", Op(T::With)),
        (r"collect", Op(T::Collect)),
        (r"del", Op(T::Delete)),
        (
            r"style",
            Assignable(T::GetStyle, T::AssignStyle, Prefs::None),
        ),
        (
            r"tag|type",
            Assignable(T::GetTag, T::AssignTag, Prefs::None),
        ),
        (r"kind", Op(T::GetKind)),
        (
            r"anchor",
            Assignable(T::GetAnchor, T::AssignAnchor, Prefs::None),
        ),
        (
            r"alias",
            Assignable(T::GetAlias, T::AssignAlias, Prefs::None),
        ),
        (r"comments\s*=", AssignAllComments(false)),
        (r"comments\s*\|=", AssignAllComments(true)),
        (
            r"if",
            LiteralWithCloser(TokenType::OpenBracket, false, "end"),
        ),
        (r"then", Op(T::IfThen)),
        (r"elif", Op(T::IfElse)),
        (r"else", Op(T::IfElse)),
        (r"end", Literal(TokenType::CloseBracket, true)),
        (r";", Op(T::Block)),
        (r"\/\/", Op(T::Alternative)),
        (r"documentIndex|document_?index|di", Op(T::GetDocumentIndex)),
        (
            r"upcase|ascii_?upcase",
            OpPrefs(T::ChangeCase, Prefs::ChangeCase { upper: true }),
        ),
        (
            r"downcase|ascii_?downcase",
            OpPrefs(T::ChangeCase, Prefs::ChangeCase { upper: false }),
        ),
        (r"trim", Op(T::Trim)),
        (r"to_?string", Op(T::ToString)),
        (r"0[xX][0-9A-Fa-f]+", Hex),
        (r"-?[1-9](\.\d+)?[Ee][-+]?\d+", Float),
        (r"-?\d+(\.\d+)", Float),
        (r"-?\d+", Number),
        (r"[Tt][Rr][Uu][Ee]", Bool(true)),
        (r"[Ff][Aa][Ll][Ss][Ee]", Bool(false)),
        (r"[Nn][Uu][Ll][Ll]|~", Null),
        (r#""([^"\\]*(\\.[^"\\]*)*)""#, StringValue),
        (r"strenv\([^\)]+\)", Env(true)),
        (r"env\([^\)]+\)", Env(false)),
        (r"envsubst\((ne|nu|ff| |,)+\)", EnvSubstOptions),
        (r"envsubst", Op(T::Envsubst)),
        (r"\s*==\s*", Op(T::Equals)),
        (r"\s*!=\s*", Op(T::NotEquals)),
        (
            r"\s*>=\s*",
            OpPrefs(
                T::Compare,
                Prefs::Compare {
                    or_equal: true,
                    greater: true,
                },
            ),
        ),
        (
            r"\s*<=\s*",
            OpPrefs(
                T::Compare,
                Prefs::Compare {
                    or_equal: true,
                    greater: false,
                },
            ),
        ),
        (
            r"\s*>\s*",
            OpPrefs(
                T::Compare,
                Prefs::Compare {
                    or_equal: false,
                    greater: true,
                },
            ),
        ),
        (
            r"\s*<\s*",
            OpPrefs(
                T::Compare,
                Prefs::Compare {
                    or_equal: false,
                    greater: false,
                },
            ),
        ),
        (r"min", Op(T::Min)),
        (r"max", Op(T::Max)),
        (r"\|=[c]*", AssignOp(true)),
        (r"=[c]*", AssignOp(false)),
        (r"[ \t\n]+", Skip),
        (r#"\."[^ "]+"\??"#, Path(true)),
        (r"\.[^ ;\}\{\:\[\],\|\.\[\(\)=\n!]+\??", Path(false)),
        (r"\|", Op(T::Pipe)),
        (r"\.", Op(T::SelfReference)),
        (r",", Op(T::Union)),
        (r"\*=[\+|\?cdn]*", MultiplyPrefs(T::MultiplyAssign)),
        (r"\*[\+|\?cdn]*", MultiplyPrefs(T::Multiply)),
        (r"\/", Op(T::Divide)),
        (r"%", Op(T::Modulo)),
        (r"\+=", Op(T::AddAssign)),
        (r"\+", Op(T::Add)),
        (r"\-=", Op(T::SubtractAssign)),
        (r"\-", Op(T::Subtract)),
        (r"#.*", Skip),
        (r"pivot", Op(T::Pivot)),
    ];
    table
        .into_iter()
        .map(|(pattern, action)| Rule {
            pattern: regex::compile(pattern),
            action,
        })
        .collect()
}

/// `extractNumberParameter`: the integer inside the trailing parentheses.
fn extract_number_parameter(value: &str) -> Result<i64, Error> {
    let open = value
        .rfind('(')
        .ok_or_else(|| Error(format!("bad parameter in {value}")))?;
    let close = value
        .rfind(')')
        .ok_or_else(|| Error(format!("bad parameter in {value}")))?;
    crate::util::parse_int(&value[open + 1..close])
}

fn has_option_parameter(value: &str, option: &str) -> bool {
    let open = match value.find('(') {
        Some(index) => index,
        None => return false,
    };
    let inner = &value[open + 1..];
    let inner = inner.strip_suffix(')').unwrap_or(inner);
    inner
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|word| word == option)
}

fn unwrap(value: &str) -> &str {
    &value[1..value.len() - 1]
}

fn create_token(action: &Action, text: &str) -> Result<Token, Error> {
    let token = match action {
        Action::Skip => return Err(Error("skip".to_string())),
        Action::Literal(token_type, check) => Token::literal(*token_type, *check, text),
        Action::LiteralWithCloser(token_type, check, closer) => Token {
            expected_closer: (*closer).to_string(),
            ..Token::literal(*token_type, *check, text)
        },
        Action::Op(op_type) => {
            let mut op = Operation::new(*op_type);
            op.string_value = text.to_string();
            Token::operation(op)
        }
        Action::OpPrefs(op_type, prefs) => {
            let mut op = Operation::with_prefs(*op_type, prefs.clone());
            op.string_value = text.to_string();
            Token::operation(op)
        }
        Action::Assignable(op_type, assign_type, prefs) => {
            let mut op = Operation::with_prefs(*op_type, prefs.clone());
            op.string_value = text.to_string();
            let mut assign = Operation::with_prefs(*assign_type, prefs.clone());
            assign.string_value = text.to_string();
            Token {
                assign_operation: Some(assign),
                ..Token::operation(op)
            }
        }
        Action::Path(wrapped) => {
            let mut value = text;
            let mut prefs = TraversePrefs::default();
            if value.ends_with('?') {
                prefs.optional_traverse = true;
                value = &value[..value.len() - 1];
            }
            value = &value[1..];
            if *wrapped {
                value = unwrap(value);
            }
            let op = Operation {
                op_type: OpType::TraversePath,
                value: OpValue::Str(value.to_string()),
                string_value: value.to_string(),
                node: None,
                reference: None,
                prefs: Prefs::Traverse(prefs),
                update_assign: false,
            };
            Token {
                check_for_post_traverse: true,
                ..Token::operation(op)
            }
        }
        Action::RecursiveDescent(include_map_keys) => {
            let prefs = RecursiveDescentPrefs {
                recurse_array: true,
                traverse_prefs: TraversePrefs {
                    dont_follow_alias: true,
                    include_map_keys: *include_map_keys,
                    ..TraversePrefs::default()
                },
            };
            let mut op =
                Operation::with_prefs(OpType::RecursiveDescent, Prefs::RecursiveDescent(prefs));
            op.string_value = text.to_string();
            Token::operation(op)
        }
        Action::Expression(expression) => {
            let op = Operation::with_prefs(
                OpType::Expression,
                Prefs::Expression((*expression).to_string()),
            );
            Token {
                check_for_post_traverse: false,
                ..Token::operation(op)
            }
        }
        Action::FlattenDepth => {
            let depth = extract_number_parameter(text)?;
            let mut op = Operation::with_prefs(OpType::Flatten, Prefs::Flatten { depth });
            op.string_value = text.to_string();
            Token::operation(op)
        }
        Action::AssignAllComments(update_assign) => {
            let mut op =
                Operation::with_prefs(OpType::AssignComment, comment_prefs(true, true, true));
            op.string_value = text.to_string();
            op.update_assign = *update_assign;
            Token {
                check_for_post_traverse: false,
                ..Token::operation(op)
            }
        }
        Action::AssignOp(update_assign) => {
            let prefs = AssignPrefs {
                dont_overwrite_anchor: true,
                clobber_custom_tags: text.contains('c'),
                only_write_null: false,
            };
            let mut op = Operation::with_prefs(OpType::Assign, Prefs::Assign(prefs));
            op.string_value = text.to_string();
            op.update_assign = *update_assign;
            Token {
                check_for_post_traverse: false,
                ..Token::operation(op)
            }
        }
        Action::Bool(value) => Token::operation(Operation::value(OpValue::Bool(*value), text)),
        Action::Null => Token::operation(Operation::value(OpValue::Null, text)),
        Action::StringValue => {
            let value = process_escape_characters(unwrap(text));
            let op = Operation {
                op_type: OpType::StringInterpolation,
                value: OpValue::Str(value.clone()),
                string_value: value,
                node: None,
                reference: None,
                prefs: Prefs::None,
                update_assign: false,
            };
            Token::operation(op)
        }
        Action::Env(strenv) => {
            let value = if *strenv {
                &text[7..text.len() - 1]
            } else {
                &text[4..text.len() - 1]
            };
            let mut op = Operation::value(OpValue::Str(value.to_string()), value);
            op.op_type = OpType::Env;
            op.prefs = Prefs::Env {
                string_value: *strenv,
                no_unset: false,
                no_empty: false,
                fail_fast: false,
            };
            Token {
                check_for_post_traverse: true,
                ..Token::operation(op)
            }
        }
        Action::EnvSubstOptions => {
            let prefs = Prefs::Env {
                string_value: false,
                no_empty: has_option_parameter(text, "ne"),
                no_unset: has_option_parameter(text, "nu"),
                fail_fast: has_option_parameter(text, "ff"),
            };
            let mut op = Operation::with_prefs(OpType::Envsubst, prefs);
            op.string_value = text.to_string();
            Token::operation(op)
        }
        Action::MultiplyPrefs(op_type) => {
            let mut prefs = MultiplyPrefs::default();
            if text.contains('+') {
                prefs.append_arrays = true;
            }
            if text.contains('?') {
                prefs.traverse_prefs = TraversePrefs {
                    dont_auto_create: true,
                    ..TraversePrefs::default()
                };
            }
            if text.contains('n') {
                prefs.assign_prefs.only_write_null = true;
            }
            if text.contains('d') {
                prefs.deep_merge_arrays = true;
            }
            if text.contains('c') {
                prefs.assign_prefs.clobber_custom_tags = true;
            }
            prefs.traverse_prefs.dont_follow_alias = true;
            prefs.traverse_prefs.exact_key_match = true;
            let mut op = Operation::with_prefs(*op_type, Prefs::Multiply(prefs));
            op.value = OpValue::Str(OpType::Multiply.name().to_string());
            op.string_value = text.to_string();
            Token::operation(op)
        }
        Action::GetVariable => {
            let value = &text[1..];
            let mut op = Operation::value(OpValue::Str(value.to_string()), value);
            op.op_type = OpType::GetVariable;
            Token {
                check_for_post_traverse: true,
                ..Token::operation(op)
            }
        }
        Action::Hex => {
            let number = i64::from_str_radix(&text[2..], 16).map_err(|_| {
                Error(format!(
                    "strconv.ParseInt: parsing \"{}\": value out of range",
                    &text[2..]
                ))
            })?;
            Token::operation(Operation::value(OpValue::Int(number), text))
        }
        Action::Float => {
            let number: f64 = text.parse().map_err(|_| {
                Error(format!(
                    "strconv.ParseFloat: parsing \"{text}\": invalid syntax"
                ))
            })?;
            Token::operation(Operation::value(OpValue::Float(number), text))
        }
        Action::Number => {
            let number: i64 = text.parse().map_err(|_| {
                Error(format!(
                    "strconv.ParseInt: parsing \"{text}\": value out of range"
                ))
            })?;
            Token::operation(Operation::value(OpValue::Int(number), text))
        }
        Action::ParentLevel => {
            let level = extract_number_parameter(text)?;
            let mut op = Operation::with_prefs(OpType::GetParent, Prefs::Parent { level });
            op.string_value = text.to_string();
            Token {
                check_for_post_traverse: true,
                ..Token::operation(op)
            }
        }
        Action::ParentDefault => {
            let op = Operation::with_prefs(OpType::GetParent, Prefs::Parent { level: 1 });
            Token {
                check_for_post_traverse: true,
                ..Token::operation(op)
            }
        }
        Action::EncodeIndent(format) => {
            let indent = extract_number_parameter(text)?;
            let mut op = Operation::with_prefs(
                OpType::Encode,
                Prefs::Encode {
                    format: *format,
                    indent: indent.max(0) as usize,
                },
            );
            op.string_value = text.to_string();
            Token {
                check_for_post_traverse: false,
                ..Token::operation(op)
            }
        }
        Action::EncodeWithIndent(format, indent) => {
            let mut op = Operation::with_prefs(
                OpType::Encode,
                Prefs::Encode {
                    format: *format,
                    indent: *indent,
                },
            );
            op.string_value = text.to_string();
            Token::operation(op)
        }
        Action::Decode(format) => {
            let mut op = Operation::with_prefs(OpType::Decode, Prefs::Decode { format: *format });
            op.string_value = text.to_string();
            Token::operation(op)
        }
        Action::Load(format) => {
            let mut op = Operation::with_prefs(OpType::Load, Prefs::Load { format: *format });
            op.string_value = text.to_string();
            Token::operation(op)
        }
    };
    Ok(token)
}

/// `Tokenise`: runs the rule table over the expression, first match wins at
/// each position, then applies `postProcessTokens`.
pub fn tokenise(expression: &str) -> Result<Vec<Token>, Error> {
    let rules = rules();
    let chars: Vec<char> = expression.chars().collect();
    let mut pos = 0;
    let mut tokens = Vec::new();
    while pos < chars.len() {
        let mut matched = None;
        for rule in &rules {
            if let Some(length) = regex::match_prefix(&rule.pattern, &chars[pos..]) {
                if length == 0 {
                    continue;
                }
                matched = Some((length, &rule.action));
                break;
            }
        }
        let (length, action) = match matched {
            Some(found) => found,
            None => {
                let line = 1 + chars[..pos].iter().filter(|c| **c == '\n').count();
                let column = pos
                    - chars[..pos]
                        .iter()
                        .rposition(|c| *c == '\n')
                        .map_or(0, |i| i + 1)
                    + 1;
                return Err(Error(format!(
                    "{line}:{column}: invalid input text {:?}",
                    chars[pos..].iter().collect::<String>()
                )));
            }
        };
        let text: String = chars[pos..pos + length].iter().collect();
        pos += length;
        if matches!(action, Action::Skip) {
            continue;
        }
        tokens.push(create_token(action, &text)?);
    }
    Ok(post_process_tokens(tokens))
}

fn post_process_tokens(tokens: Vec<Token>) -> Vec<Token> {
    let mut processed = Vec::new();
    let mut skip_next = false;
    for index in 0..tokens.len() {
        if skip_next {
            skip_next = false;
            continue;
        }
        skip_next = handle_token(&tokens, index, &mut processed);
    }
    processed
}

fn op_token(op_type: OpType, string_value: &str) -> Token {
    let mut op = Operation::new(op_type);
    op.string_value = string_value.to_string();
    Token {
        check_for_post_traverse: false,
        ..Token::operation(op)
    }
}

fn handle_token(tokens: &[Token], index: usize, processed: &mut Vec<Token>) -> bool {
    let mut skip_next = false;
    let mut current = tokens[index].clone();
    let last = index == tokens.len() - 1;

    if current.token_type == TokenType::TraverseArrayCollect {
        processed.push(op_token(OpType::SelfReference, "SELF"));
        processed.push(op_token(OpType::TraverseArray, "TRAVERSE_ARRAY"));
        current = Token::literal(TokenType::OpenCollect, false, "");
    }

    if current.is_op(OpType::CreateMap) {
        let previous_is_traverse_collect =
            index > 0 && tokens[index - 1].token_type == TokenType::TraverseArrayCollect;
        let previous_is_open_after_value = index >= 2
            && tokens[index - 1].token_type == TokenType::OpenCollect
            && matches!(
                tokens[index - 2].token_type,
                TokenType::Operation | TokenType::CloseCollect | TokenType::CloseCollectObject
            );
        if previous_is_traverse_collect || previous_is_open_after_value {
            processed.push(Token {
                check_for_post_traverse: false,
                ..Token::operation(Operation::value(OpValue::Int(0), "0"))
            });
        }
    }

    if !last && current.assign_operation.is_some() && tokens[index + 1].is_op(OpType::Assign) {
        let update_assign = tokens[index + 1]
            .operation
            .as_ref()
            .is_some_and(|op| op.update_assign);
        let mut assign = current
            .assign_operation
            .take()
            .unwrap_or_else(|| Operation::new(OpType::Assign));
        assign.update_assign = update_assign;
        current.operation = Some(assign);
        skip_next = true;
    }

    let is_create_map = current.is_op(OpType::CreateMap);
    let current_type = current.token_type;
    let check_post = current.check_for_post_traverse;
    processed.push(current);

    if is_create_map && !last && tokens[index + 1].token_type == TokenType::CloseCollect {
        processed.push(Token {
            check_for_post_traverse: false,
            ..Token::operation(Operation::new(OpType::Length))
        });
    }

    if !last
        && ((current_type == TokenType::OpenCollect
            && tokens[index + 1].token_type == TokenType::CloseCollect)
            || (current_type == TokenType::OpenCollectObject
                && tokens[index + 1].token_type == TokenType::CloseCollectObject))
    {
        processed.push(op_token(OpType::Empty, "EMPTY"));
    }

    if !last
        && check_post
        && (tokens[index + 1].is_op(OpType::TraversePath)
            || tokens[index + 1].token_type == TokenType::TraverseArrayCollect)
    {
        let mut op = Operation::new(OpType::ShortPipe);
        op.value = OpValue::Str("PIPE".to_string());
        op.string_value = ".".to_string();
        processed.push(Token {
            check_for_post_traverse: false,
            ..Token::operation(op)
        });
    }
    if !last && check_post && tokens[index + 1].token_type == TokenType::OpenCollect {
        processed.push(Token {
            check_for_post_traverse: false,
            ..Token::operation(Operation::new(OpType::TraverseArray))
        });
    }
    skip_next
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(expression: &str) -> Vec<String> {
        tokenise(expression)
            .unwrap()
            .iter()
            .map(|t| t.describe())
            .collect()
    }

    #[test]
    fn tokenises_paths_and_operators() {
        assert_eq!(
            names(".a.b"),
            vec!["TRAVERSE_PATH", "SHORT_PIPE", "TRAVERSE_PATH"]
        );
        assert_eq!(
            names(".a[0]"),
            vec!["TRAVERSE_PATH", "TRAVERSE_ARRAY", "[", "VALUE", "]"]
        );
        assert_eq!(
            names(".[]"),
            vec!["SELF", "TRAVERSE_ARRAY", "[", "EMPTY", "]"]
        );
        assert_eq!(names(".a == 1"), vec!["TRAVERSE_PATH", "EQUALS", "VALUE"]);
        assert_eq!(names("tag = \"!!str\""), vec!["ASSIGN_TAG", "STRING_INT"]);
        assert_eq!(names("length"), vec!["LENGTH"]);
        assert_eq!(
            names(".[1:]"),
            vec![
                "SELF",
                "TRAVERSE_ARRAY",
                "[",
                "VALUE",
                "CREATE_MAP",
                "LENGTH",
                "]"
            ]
        );
    }
}
