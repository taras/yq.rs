//! `operation.go`: operation types with their arity and precedence, the
//! `Operation` carried by each token, and the expression tree.

use crate::node::{AssignPrefs, Node, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpType {
    Or,
    And,
    Reduce,
    IfThen,
    IfElse,
    Block,
    Union,
    Pipe,
    Assign,
    AddAssign,
    SubtractAssign,
    AssignAttributes,
    AssignStyle,
    AssignVariable,
    AssignTag,
    AssignComment,
    AssignAnchor,
    AssignAlias,
    Multiply,
    MultiplyAssign,
    Divide,
    Modulo,
    Add,
    Subtract,
    Alternative,
    Equals,
    NotEquals,
    Compare,
    Min,
    Max,
    CreateMap,
    ShortPipe,
    Length,
    Line,
    Column,
    Expression,
    Collect,
    Map,
    Filter,
    Error,
    Pick,
    Omit,
    Eval,
    MapValues,
    FormatDateTime,
    WithDtf,
    Now,
    Tz,
    FromUnix,
    ToUnix,
    Encode,
    Decode,
    Any,
    All,
    Contains,
    AnyCondition,
    AllCondition,
    ToEntries,
    FromEntries,
    WithEntries,
    With,
    SplitDoc,
    GetVariable,
    GetStyle,
    GetTag,
    GetKind,
    GetKey,
    IsKey,
    GetParent,
    GetParents,
    GetComment,
    GetAnchor,
    GetAlias,
    GetDocumentIndex,
    GetFilename,
    GetFileIndex,
    GetPath,
    SetPath,
    DelPaths,
    Explode,
    SortBy,
    First,
    Reverse,
    Sort,
    Shuffle,
    SortKeys,
    Join,
    Sub,
    Match,
    Capture,
    Test,
    Split,
    ChangeCase,
    Trim,
    ToString,
    StringInterpolation,
    Load,
    LoadString,
    System,
    Keys,
    CollectObject,
    TraversePath,
    TraverseArray,
    SelfReference,
    Value,
    Reference,
    Env,
    Not,
    ToNumber,
    Empty,
    Envsubst,
    RecursiveDescent,
    Select,
    Has,
    Unique,
    UniqueBy,
    GroupBy,
    Flatten,
    Delete,
    Pivot,
}

pub struct OpInfo {
    pub name: &'static str,
    pub num_args: u8,
    pub precedence: u32,
    pub check_for_post_traverse: bool,
}

const fn info(name: &'static str, num_args: u8, precedence: u32) -> OpInfo {
    OpInfo {
        name,
        num_args,
        precedence,
        check_for_post_traverse: false,
    }
}

const fn info_post(name: &'static str, num_args: u8, precedence: u32) -> OpInfo {
    OpInfo {
        name,
        num_args,
        precedence,
        check_for_post_traverse: true,
    }
}

impl OpType {
    pub fn info(self) -> OpInfo {
        match self {
            OpType::Or => info("OR", 2, 20),
            OpType::And => info("AND", 2, 20),
            OpType::Reduce => info("REDUCE", 2, 35),
            OpType::IfThen => info("IF_THEN", 2, 5),
            OpType::IfElse => info("IF_ELSE", 2, 4),
            OpType::Block => info("BLOCK", 2, 10),
            OpType::Union => info("UNION", 2, 10),
            OpType::Pipe => info("PIPE", 2, 30),
            OpType::Assign => info("ASSIGN", 2, 40),
            OpType::AddAssign => info("ADD_ASSIGN", 2, 40),
            OpType::SubtractAssign => info("SUBTRACT_ASSIGN", 2, 40),
            OpType::AssignAttributes => info("ASSIGN_ATTRIBUTES", 2, 40),
            OpType::AssignStyle => info("ASSIGN_STYLE", 2, 40),
            OpType::AssignVariable => info("ASSIGN_VARIABLE", 2, 40),
            OpType::AssignTag => info("ASSIGN_TAG", 2, 40),
            OpType::AssignComment => info("ASSIGN_COMMENT", 2, 40),
            OpType::AssignAnchor => info("ASSIGN_ANCHOR", 2, 40),
            OpType::AssignAlias => info("ASSIGN_ALIAS", 2, 40),
            OpType::Multiply => info("MULTIPLY", 2, 42),
            OpType::MultiplyAssign => info("MULTIPLY_ASSIGN", 2, 42),
            OpType::Divide => info("DIVIDE", 2, 42),
            OpType::Modulo => info("MODULO", 2, 42),
            OpType::Add => info("ADD", 2, 42),
            OpType::Subtract => info("SUBTRACT", 2, 42),
            OpType::Alternative => info("ALTERNATIVE", 2, 42),
            OpType::Equals => info("EQUALS", 2, 40),
            OpType::NotEquals => info("NOT_EQUALS", 2, 40),
            OpType::Compare => info("COMPARE", 2, 40),
            OpType::Min => info("MIN", 0, 40),
            OpType::Max => info("MAX", 0, 40),
            OpType::CreateMap => info("CREATE_MAP", 2, 15),
            OpType::ShortPipe => info("SHORT_PIPE", 2, 45),
            OpType::Length => info("LENGTH", 0, 50),
            OpType::Line => info("LINE", 0, 50),
            OpType::Column => info("LINE", 0, 50),
            OpType::Expression => info("EXP", 0, 50),
            OpType::Collect => info("COLLECT", 1, 50),
            OpType::Map => info_post("MAP", 1, 52),
            OpType::Filter => info_post("FILTER", 1, 52),
            OpType::Error => info("ERROR", 1, 50),
            OpType::Pick => info_post("PICK", 1, 52),
            OpType::Omit => info_post("OMIT", 1, 52),
            OpType::Eval => info_post("EVAL", 1, 52),
            OpType::MapValues => info_post("MAP_VALUES", 1, 52),
            OpType::FormatDateTime => info("FORMAT_DATE_TIME", 1, 50),
            OpType::WithDtf => info("WITH_DATE_TIME_FORMAT", 1, 50),
            OpType::Now => info("NOW", 0, 50),
            OpType::Tz => info("TIMEZONE", 1, 50),
            OpType::FromUnix => info("FROM_UNIX", 0, 50),
            OpType::ToUnix => info("TO_UNIX", 0, 50),
            OpType::Encode => info("ENCODE", 0, 50),
            OpType::Decode => info("DECODE", 0, 50),
            OpType::Any => info("ANY", 0, 50),
            OpType::All => info("ALL", 0, 50),
            OpType::Contains => info("CONTAINS", 1, 50),
            OpType::AnyCondition => info("ANY_CONDITION", 1, 50),
            OpType::AllCondition => info("ALL_CONDITION", 1, 50),
            OpType::ToEntries => info_post("TO_ENTRIES", 0, 52),
            OpType::FromEntries => info("FROM_ENTRIES", 0, 50),
            OpType::WithEntries => info("WITH_ENTRIES", 1, 50),
            OpType::With => info_post("WITH", 1, 52),
            OpType::SplitDoc => info_post("SPLIT_DOC", 0, 52),
            OpType::GetVariable => info("GET_VARIABLE", 0, 55),
            OpType::GetStyle => info("GET_STYLE", 0, 50),
            OpType::GetTag => info("GET_TAG", 0, 50),
            OpType::GetKind => info("GET_KIND", 0, 50),
            OpType::GetKey => info("GET_KEY", 0, 50),
            OpType::IsKey => info("IS_KEY", 0, 50),
            OpType::GetParent => info("GET_PARENT", 0, 50),
            OpType::GetParents => info("GET_PARENTS", 0, 50),
            OpType::GetComment => info("GET_COMMENT", 0, 50),
            OpType::GetAnchor => info("GET_ANCHOR", 0, 50),
            OpType::GetAlias => info("GET_ALIAS", 0, 50),
            OpType::GetDocumentIndex => info("GET_DOCUMENT_INDEX", 0, 50),
            OpType::GetFilename => info("GET_FILENAME", 0, 50),
            OpType::GetFileIndex => info("GET_FILE_INDEX", 0, 50),
            OpType::GetPath => info_post("GET_PATH", 0, 52),
            OpType::SetPath => info("SET_PATH", 1, 50),
            OpType::DelPaths => info_post("DEL_PATHS", 1, 52),
            OpType::Explode => info_post("EXPLODE", 1, 52),
            OpType::SortBy => info_post("SORT_BY", 1, 52),
            OpType::First => info_post("FIRST", 1, 52),
            OpType::Reverse => info_post("REVERSE", 0, 52),
            OpType::Sort => info_post("SORT", 0, 52),
            OpType::Shuffle => info_post("SHUFFLE", 0, 52),
            OpType::SortKeys => info_post("SORT_KEYS", 1, 52),
            OpType::Join => info("JOIN", 1, 50),
            OpType::Sub => info("SUBSTR", 1, 50),
            OpType::Match => info("MATCH", 1, 50),
            OpType::Capture => info("CAPTURE", 1, 50),
            OpType::Test => info("TEST", 1, 50),
            OpType::Split => info_post("SPLIT", 1, 52),
            OpType::ChangeCase => info("CHANGE_CASE", 0, 50),
            OpType::Trim => info("TRIM", 0, 50),
            OpType::ToString => info("TO_STRING", 0, 50),
            OpType::StringInterpolation => info("STRING_INT", 0, 50),
            OpType::Load => info_post("LOAD", 1, 52),
            OpType::LoadString => info("LOAD_STRING", 1, 52),
            OpType::System => info("SYSTEM", 1, 50),
            OpType::Keys => info_post("KEYS", 0, 52),
            OpType::CollectObject => info("COLLECT_OBJECT", 0, 50),
            OpType::TraversePath => info("TRAVERSE_PATH", 0, 55),
            OpType::TraverseArray => info("TRAVERSE_ARRAY", 2, 50),
            OpType::SelfReference => info("SELF", 0, 55),
            OpType::Value => info("VALUE", 0, 50),
            OpType::Reference => info("REF", 0, 50),
            OpType::Env => info_post("ENV", 0, 52),
            OpType::Not => info("NOT", 0, 50),
            OpType::ToNumber => info("TO_NUMBER", 0, 50),
            OpType::Empty => info("EMPTY", 0, 50),
            OpType::Envsubst => info("ENVSUBST", 0, 50),
            OpType::RecursiveDescent => info("RECURSIVE_DESCENT", 0, 50),
            OpType::Select => info_post("SELECT", 1, 52),
            OpType::Has => info("HAS", 1, 50),
            OpType::Unique => info_post("UNIQUE", 0, 52),
            OpType::UniqueBy => info_post("UNIQUE_BY", 1, 52),
            OpType::GroupBy => info_post("GROUP_BY", 1, 52),
            OpType::Flatten => info_post("FLATTEN_BY", 0, 52),
            OpType::Delete => info("DELETE", 1, 40),
            OpType::Pivot => info_post("PIVOT", 0, 52),
        }
    }

    pub fn name(self) -> &'static str {
        self.info().name
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TraversePrefs {
    pub dont_follow_alias: bool,
    pub include_map_keys: bool,
    pub dont_auto_create: bool,
    pub dont_include_map_values: bool,
    pub optional_traverse: bool,
    pub exact_key_match: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MultiplyPrefs {
    pub append_arrays: bool,
    pub deep_merge_arrays: bool,
    pub traverse_prefs: TraversePrefs,
    pub assign_prefs: AssignPrefs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecursiveDescentPrefs {
    pub traverse_prefs: TraversePrefs,
    pub recurse_array: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeFormat {
    Yaml,
    Json,
    Props,
    Xml,
    Csv,
    Tsv,
    Base64,
    Base64Url,
    Uri,
    Sh,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum Prefs {
    #[default]
    None,
    Traverse(TraversePrefs),
    Assign(AssignPrefs),
    Multiply(MultiplyPrefs),
    RecursiveDescent(RecursiveDescentPrefs),
    Compare {
        or_equal: bool,
        greater: bool,
    },
    Comment {
        line: bool,
        head: bool,
        foot: bool,
    },
    AssignVar {
        is_reference: bool,
    },
    Flatten {
        depth: i64,
    },
    Expression(String),
    Encode {
        format: EncodeFormat,
        indent: usize,
    },
    Decode {
        format: EncodeFormat,
    },
    Env {
        string_value: bool,
        no_unset: bool,
        no_empty: bool,
        fail_fast: bool,
    },
    Parent {
        level: i64,
    },
    ChangeCase {
        upper: bool,
    },
    Load {
        format: EncodeFormat,
    },
}

impl Prefs {
    pub fn traverse(&self) -> TraversePrefs {
        match self {
            Prefs::Traverse(prefs) => *prefs,
            Prefs::Multiply(prefs) => prefs.traverse_prefs,
            _ => TraversePrefs::default(),
        }
    }

    pub fn assign(&self) -> AssignPrefs {
        match self {
            Prefs::Assign(prefs) => *prefs,
            Prefs::Multiply(prefs) => prefs.assign_prefs,
            _ => AssignPrefs::default(),
        }
    }
}

/// The typed `Value` of an operation: ints and the path elements that come
/// from `GetPath` are ints, everything else is text.
#[derive(Debug, Clone, PartialEq)]
pub enum OpValue {
    None,
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
}

impl std::fmt::Display for OpValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpValue::None => Ok(()),
            OpValue::Str(s) => f.write_str(s),
            OpValue::Int(i) => write!(f, "{i}"),
            OpValue::Float(v) => f.write_str(&crate::util::format_float(*v)),
            OpValue::Bool(b) => write!(f, "{b}"),
            OpValue::Null => f.write_str("<nil>"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Operation {
    pub op_type: OpType,
    pub value: OpValue,
    pub string_value: String,
    /// Template for the node a value operation produces; copied per use.
    pub node: Option<Node>,
    /// The live node a `REF` operation yields.
    pub reference: Option<NodeId>,
    pub prefs: Prefs,
    pub update_assign: bool,
}

impl Operation {
    pub fn new(op_type: OpType) -> Operation {
        Operation {
            op_type,
            value: OpValue::Str(op_type.name().to_string()),
            string_value: op_type.name().to_string(),
            node: None,
            reference: None,
            prefs: Prefs::None,
            update_assign: false,
        }
    }

    pub fn with_prefs(op_type: OpType, prefs: Prefs) -> Operation {
        Operation {
            prefs,
            ..Operation::new(op_type)
        }
    }

    pub fn reference(id: NodeId) -> Operation {
        Operation {
            reference: Some(id),
            ..Operation::new(OpType::Reference)
        }
    }

    /// `createValueOperation`.
    pub fn value(value: OpValue, string_value: &str) -> Operation {
        let tag = match value {
            OpValue::Float(_) => "!!float",
            OpValue::Int(_) => "!!int",
            OpValue::Bool(_) => "!!bool",
            OpValue::Str(_) => "!!str",
            OpValue::Null => "!!null",
            OpValue::None => "",
        };
        Operation {
            op_type: OpType::Value,
            value,
            string_value: string_value.to_string(),
            node: Some(Node::scalar(tag, string_value)),
            reference: None,
            prefs: Prefs::None,
            update_assign: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExprNode {
    pub op: Operation,
    pub lhs: Option<Box<ExprNode>>,
    pub rhs: Option<Box<ExprNode>>,
}

impl ExprNode {
    pub fn leaf(op: Operation) -> ExprNode {
        ExprNode {
            op,
            lhs: None,
            rhs: None,
        }
    }

    pub fn binary(op: Operation, lhs: ExprNode, rhs: ExprNode) -> ExprNode {
        ExprNode {
            op,
            lhs: Some(Box::new(lhs)),
            rhs: Some(Box::new(rhs)),
        }
    }

    pub fn lhs(&self) -> Option<&ExprNode> {
        self.lhs.as_deref()
    }

    pub fn rhs(&self) -> Option<&ExprNode> {
        self.rhs.as_deref()
    }
}
