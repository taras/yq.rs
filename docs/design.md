# Design

yq.rs is a port of `pkg/yqlib` from mikefarah/yq. The Go code is the model
for the Rust structure, so that anyone reading an operator in Go can find
its Rust counterpart, and so the conformance fixture's behaviour carries
over without reinterpretation.

## Module map

| Go file(s) | Rust module | Notes |
|---|---|---|
| `candidate_node.go`, `lib.go` | `node.rs` | Node arena, `Kind`, `Style`, copy, update-from, equality, `guess_tag_from_custom_type`. |
| `context.go` | `context.rs` | Matching node list, variables, `dont_auto_create`, datetime layout. |
| `operation.go` | `operation.rs` | `OperationType` table: name, arity, precedence, handler. |
| `lexer_participle.go` | `lexer.rs` | Ordered token rules. Hand-written matcher, no regex crate at parse time. |
| `expression_postfix.go`, `expression_postfix_if_validator.go` | `postfix.rs` | Shunting yard to postfix with the if/then/else validator. |
| `expression_parser.go` | `parser.rs` | Postfix to `ExpressionNode` tree. |
| `data_tree_navigator.go`, `operators.go` | `eval.rs` | `get_matching_nodes`, `cross_function`, `compound_assign`. |
| `operator_*.go` | `ops/<name>.rs` | One file per Go file. |
| `decoder_yaml.go`, `candidate_node_yaml.go` | `yaml/decode.rs` | Events to nodes, tag resolution. |
| `encoder_yaml.go` | `yaml/encode.rs` | Nodes to YAML text, yaml.v3 rules. |
| `decoder_json.go`, `encoder_json.go`, `candidate_node_json.go` | `json/` | Order preserving. |
| `printer.go` | `printer.rs` | Result printing: unwrap scalar, separators, indent. |
| `operators_test.go` (`testScenario`, `resultToString`) | `conformance.rs` | Test-format printer, used only by the harness. |

## Node model

```rust
pub type NodeId = usize;

pub struct Node {
    pub kind: Kind,                 // Scalar, Mapping, Sequence, Alias
    pub style: Style,               // bit flags, same values as yaml.v3
    pub tag: String,                // "!!str", "!!int", custom tags
    pub value: String,              // scalar text, always the source text
    pub anchor: String,
    pub alias: Option<NodeId>,
    pub content: Vec<NodeId>,       // mapping: key, value, key, value, ...
    pub head_comment: String,
    pub line_comment: String,
    pub foot_comment: String,
    pub parent: Option<NodeId>,
    pub key: Option<NodeId>,        // map key node or sequence index node
    pub leading_content: String,
    pub document: usize,
    pub file_index: usize,
    pub filename: String,
    pub line: usize,
    pub column: usize,
    pub evaluate_together: bool,
    pub is_map_key: bool,
}
```

Nodes live in an arena owned by the evaluation. yq's operators copy nodes
freely (`Copy`, `CopyAsReplacement`, `CreateReplacement`), mutate in place
through `UpdateFrom`, and walk up through `Parent` for `path`, `parent`,
`document_index` and `filename`. An arena with index handles gives all of
that without reference counting cycles. Scalars keep their source text as
`value`; numeric interpretation happens at the operator (`parse_int64` with
underscores, hex and octal, as `lib.go` does).

Mapping content is a flat key/value list, exactly as in yaml.v3, because
operators index it that way (`Content[i]`, `Content[i+1]`).

## Evaluation

`eval::get_matching_nodes(ctx, expr) -> Result<Context>` dispatches on
`expr.operation.kind` to a handler with the Go signature:

```rust
fn(d: &mut Evaluator, ctx: Context, expr: &ExpressionNode) -> Result<Context, Error>
```

`Context` holds the ordered list of matching `NodeId`s and the variable map.
The two generic helpers in `operators.go` carry most operators:
`cross_function` (every lhs result against every rhs result, with the
`evaluate_together` and `calc_when_empty` variants) and
`compound_assign_function` (`+=`, `-=`, `*=`, `//=`).

Operator precedence and arity come from `operation.go` unchanged.

## Lexer

The Go lexer is a table of regular expressions tried in order, first match
wins. The Rust lexer keeps the same table and order but matches each rule
with a hand-written function (prefix tests, character classes) so the
`regex` crate is not needed for parsing. Keep the rule order identical;
several rules only work because an earlier rule claims a longer match
(`map_values` before `map`, `.[` before `[`, `...` before `..`).

## Formats

Supported: YAML and JSON. The `Format` enum is closed; adding a format is
a deliberate decision recorded in `PLAN.md`.

YAML decoding uses an event stream (initially `saphyr-parser`) behind
`yaml::events::EventSource`, so a comment-aware scanner can replace it later
without touching node construction. Tag resolution for plain scalars follows
yaml.v3's `resolve.go`: `!!null` for `~`, `null`, empty; `!!bool` for
`true`/`false` (yaml 1.2 core schema; `yes`/`no` are strings); `!!int` for
decimal, `0x`, `0o`, with `_` separators; `!!float` including `.inf`,
`.nan`; `!!timestamp` for ISO 8601 dates; `!!merge` for `<<`; `!!str`
otherwise. Quoted scalars are always `!!str`.

The YAML emitter is hand-written to match yaml.v3: block mappings and
sequences at the configured indent, sequences inside mappings not indented
(`key:\n- a`), flow style preserved when the node has `FlowStyle`, scalars
plain unless quoting is needed to keep the type or because of special
characters, literal and folded styles preserved, `---` between documents,
comments emitted where present.

## wasm ABI

Exports from `crates/yq-wasm`:

| Export | Signature | Purpose |
|---|---|---|
| `memory` | | Linear memory. |
| `yq_alloc` | `(len: u32) -> ptr` | Buffer for an argument. |
| `yq_free` | `(ptr, len)` | Release an argument buffer. |
| `yq_eval` | `(expr_ptr, expr_len, input_ptr, input_len, opts_ptr, opts_len) -> header_ptr` | Evaluate. |
| `yq_result_free` | `(header_ptr)` | Release the result. |

The result header is 12 bytes, little-endian: `status: u32` (0 ok,
1 error), `ptr: u32`, `len: u32`. The bytes are UTF-8 output or error text.
Memory may grow during `yq_eval`; the host must re-read `memory.buffer`
after the call. `js/yq.ts` is the reference implementation.

Options are a JSON object:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `inputFormat` | `"yaml"` or `"json"` | `"yaml"` | |
| `outputFormat` | `"yaml"` or `"json"` | `"yaml"` | |
| `indent` | number | 2 | Output indent. |
| `nullInput` | boolean | false | Evaluate against a single null document. |
| `env` | object of strings | `{}` | Values for `env`, `strenv`, `envsubst`. |
| `now` | RFC 3339 string | unset | Value of `now`. Unset makes `now` an error. |
| `files` | object of strings | `{}` | Contents for `load`, `load_str`, `load_props` and `load_base64`, keyed by the exact path the expression uses. |

The engine never reads the real environment, clock or filesystem. That is
what makes the module instantiable with an empty import object on every
runtime, and it is also the security boundary xmd wants: a yq expression in
a document can only see what the host chose to pass in.
