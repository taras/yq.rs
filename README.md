# yq.rs

A Rust implementation of the [yq](https://github.com/mikefarah/yq) expression
language, built to compile to a small WebAssembly module that any JavaScript
runtime can load with the plain `WebAssembly` API. It exists so that
[executable.md](https://github.com/thefrontside/executable.md) (`xmd`) can
query and transform objects with yq syntax without shipping the Go yq's
15 MB wasm build.

**Status: 1,107 of 1,111 conformance scenarios pass** (the remaining four
need `shuffle`, whose output depends on Go's random number generator).
The conformance fixture comes from yq commit `a814ff5`: 1,121 scenarios,
with 10 skipped for XML/JSON-only decoders. The measured release module is
528,353 bytes (210,129 bytes gzipped), within the 2 MB / 700 KB budget.
The loader's smoke tests pass under Deno, Bun and Node.

## Scope

- yq expressions: traversal, pipes, assignment,
  arithmetic, comparison, booleans, collect and object construction, string
  interpolation, variables, `ireduce`, `with`, entries, sort and group
  operators, path operators, `env`, string and regex functions, `@base64`,
  `to_json`/`from_json`, `to_yaml`/`from_yaml`, document index, and so on.
- YAML and JSON input and output; `@csv`, `@tsv`, `@props`, `@base64`,
  `@base64url`, `@uri` and `@sh` as string encoders and decoders.
- Tags, anchors and aliases, YAML styles, and comment preservation (head,
  line and foot comments survive decode, operators and encode).
- `load`, `load_str`, `load_props` and `load_base64`, reading from a table
  of files the host passes in.

### Compatibility gaps

- `shuffle` parses but returns an unsupported-operator error. Matching the
  fixture requires reproducing Go's `math/rand` sequence.
- `system` always returns yq's disabled-operation error. The engine cannot
  spawn processes.
- XML expressions (`to_xml`, `from_xml`, `@xml`, `load_xml`) parse but
  cannot encode, decode or load XML.
- Whole-document input and output support YAML and JSON only. TOML, HCL,
  Lua, INI, properties, CSV, TSV, shell and kyaml formats are unavailable;
  properties, CSV and TSV support exists inside expressions.
- Format tuning options such as CSV separators and auto-parsing, properties
  separators and array brackets, and compact sequence indentation are not
  exposed through `EvalOptions` or the CLI.
- `fix_merge_anchor_to_spec`, `disable_env_ops` and `disable_file_ops`
  exist on the internal `Evaluator`, but are not exposed through
  `EvalOptions`, wasm options or CLI flags.
- The thin CLI does not implement front-matter extraction, split-file
  output, in-place editing, multi-file evaluation or most Go CLI flags.
  Public evaluation calls have no input filename or file-index options, so
  input documents report an empty `filename` and `file_index` zero.

The host supplies environment variables, the current time and file contents
through options; the engine has no filesystem, clock or process environment
access of its own.

The fixture measures compatibility with the pinned yq version, rather than
every possible expression. One known parsing difference outside the fixture:
Go yq accepts `a: "x"#c`, while this parser requires a space before `#`.

## Layout

| Path | Purpose |
|---|---|
| `crates/yq-core` | The engine. Pure Rust, no I/O, no platform dependencies. |
| `crates/yq-wasm` | `cdylib` with a hand-written ABI (`yq_alloc`, `yq_eval`, `yq_free`, `yq_result_free`). No wasm-bindgen. |
| `crates/yq-cli` | `yq-rs`, a thin CLI for diffing against the reference yq locally. |
| `js/yq.ts` | Reference loader, runtime-neutral. `js/smoke.ts` exercises it. |
| `tests/fixtures` | The conformance fixture and its format description. |

## Quick start

Install Rust with Cargo and at least one of Deno, Bun or Node with TypeScript
stripping support, then run:

```bash
rustup target add wasm32-unknown-unknown
cargo test
cargo build --release --target wasm32-unknown-unknown -p yq-wasm
cp target/wasm32-unknown-unknown/release/yq_wasm.wasm js/yq.wasm
deno run --allow-read js/smoke.ts
```

Check the wasm size against the 2 MB uncompressed / 700 KB gzipped budget:

```bash
wc -c < js/yq.wasm
gzip -9 -c js/yq.wasm | wc -c
```

The same smoke test also runs with:

```bash
bun js/smoke.ts
node --experimental-strip-types js/smoke.ts
```

For local CLI use:

```bash
printf 'a: [1, 2]\n' | cargo run -q -p yq-cli -- '.a | map(. * 2)'
```

For a fresh reference checkout matching the conformance fixture:

```bash
git clone https://github.com/mikefarah/yq.git ref/yq
git -C ref/yq checkout a814ff5166040ec745a96bddd79ee31c3a548e83
```

See [the fixture README](tests/fixtures/README.md) for the conformance
result format.

## Using yq-core from Rust

```rust
use yq_core::{evaluate, EvalOptions};

let output = evaluate(".a.b[0]", "a:\n  b: [1, 2]\n", &EvalOptions::default())?;
assert_eq!(output, "1\n");
```

## Using the module

```ts
import { createYq } from "./js/yq.ts";
const yq = await createYq(await Deno.readFile("js/yq.wasm"));
yq.evaluate(".a.b[0]", "a:\n  b: [1, 2]\n");          // "1\n"
yq.evaluate(".[] | .name", '[{"name":"Ada"}]', {
  inputFormat: "json",
  outputFormat: "json",
});                                               // '"Ada"\n'
```

`createYq` accepts wasm bytes or a compiled `WebAssembly.Module` and
instantiates it with an empty import object. In Bun or Node, read the bytes
with `readFile` from `node:fs/promises`; in a browser, use
`new Uint8Array(await (await fetch("/yq.wasm")).arrayBuffer())`.
`evaluate` returns the printed output and throws `YqError` on evaluation
errors.

Pass external values explicitly through options:

```ts
yq.evaluate('load("config.yml") | .name = strenv(NAME)', "", {
  nullInput: true,
  env: { NAME: "Ada" },
  files: { "config.yml": "name: Grace\n" },
});                                               // "name: Ada\n"
yq.evaluate("now", "", {
  nullInput: true,
  now: "2026-10-17T00:00:00Z",
});                                               // "2026-10-17T00:00:00Z\n"
```

File keys must match the exact path used by the expression. `now` requires
a supplied RFC 3339 timestamp. YAML output unwraps scalar results by default,
matching yq's `-r`; set `unwrapScalar: false` to preserve scalar quotes,
styles and comments. JSON output keeps string quotes by default. The Rust
API exposes the same settings through `EvalOptions` using snake_case names.

### Options

| Key | Type | Default | Meaning |
|---|---|---|---|
| `inputFormat` | `"yaml"` or `"json"` | `"yaml"` | Input serialization format. |
| `outputFormat` | `"yaml"` or `"json"` | `"yaml"` | Output serialization format. |
| `indent` | number | 2 | Output indentation. |
| `nullInput` | boolean | false | Evaluate against a single null document. |
| `unwrapScalar` | boolean | true for YAML, false for JSON | Print scalar results bare (yq's `-r`). |
| `env` | object of strings | `{}` | Values for `env`, `strenv` and `envsubst`. |
| `now` | RFC 3339 string | unset | Value of `now`; unset makes it an error. |
| `files` | object of strings | `{}` | File contents for `load`, `load_str`, `load_props` and `load_base64`. |

## Performance

Measured on 2026-10-17 on macOS arm64, using engine commit `d75ebac`
against Go yq v4.54.1 (`a814ff5`). Rust used this project's size-optimized
release profile (`opt-level=z`, LTO, one codegen unit); Go used its default
optimized build. Toolchains: Rust 1.99.1, Go 1.26.0 and Deno 2.9.7.
All 19 benchmark cases produced byte-identical output across Go, native
Rust and Rust wasm before timing.

Median milliseconds per evaluation, from five warmed batches:

| Workload | Records | Go native | Rust native | Rust wasm (Deno) |
|---|---:|---:|---:|---:|
| YAML round trip | 5,000 | 42.6 | 59.1 | 59.2 |
| YAML filtering | 5,000 | 32.5 | 47.6 | 47.3 |
| JSON filtering | 5,000 | 23.9 | 20.9 | 22.4 |
| YAML sorting | 5,000 | 35.9 | 60.4 | 59.0 |
| JSON sorting | 5,000 | 28.0 | 31.7 | 34.5 |
| YAML bulk update | 500 | 439.1 | 434.2 | 485.3 |

Each evaluation parses the expression and input, evaluates, and serializes
the result. Timings exclude process startup, file reading and wasm
compilation/instantiation; wasm includes the TypeScript loader's string
copies. Each sample warms for at least 200 ms, then times a batch targeting
100 ms. Runs are serial, with randomized implementation order and normal
Go garbage collection.

Inputs contain `items` with `id`, `name`, `active` and `score` fields:
names are `user-00000` onward, every third record is active, and
`score = (id * 37) % 10000`. YAML uses block collections (~337 KB for
5,000 records); JSON is compact (~297 KB). Filtering uses
`.items | map(select(.active == true)) | length`; sorting uses
`.items | sort_by(.score) | .[0:10]`; updates use
`.items |= map(.score += 1)`.

For CLI use, startup dominates small inputs: a 50-byte YAML lookup took
**9.6 ms in Go versus 4.8 ms in Rust**, including startup, file reading and
output to `/dev/null` (median of 11 runs after two warmups).

Go is faster on these large YAML evaluations, while Rust leads on JSON
filtering. Rust wasm stays close to native Rust on the large read-only
cases. Replacing insertion sort with stable merge sort reduced native Rust's
5,000-record sorts from roughly 1.9 seconds at `275d3fa` to 60 ms for YAML
and 32 ms for JSON. Bulk updates remain expensive in both engines: going
from 100 to 500 records takes about 24 times as long. These measurements
describe the current builds and these synthetic workloads.

## Architecture

The Rust modules follow Go's `pkg/yqlib` so operators can be compared
directly with the reference source. jq implementations such as jaq do not
provide yq-specific syntax and semantics (`tag`, `with`, `env(NAME)`,
`ireduce`, comments and styles). Byte-for-byte conformance to the pinned
fixture drives implementation choices.

### Module map

Rust paths below are relative to `crates/yq-core/src/`.

| Go file(s) | Rust module | Purpose |
|---|---|---|
| `candidate_node.go`, `lib.go` | `node.rs`, `util.rs` | Node arena, copying, updates, equality and scalar helpers. |
| `context.go` | `context.rs` | Matching nodes, variables, traversal flags and datetime layout. |
| `operation.go` | `operation.rs` | Operator types, preferences, arity and precedence. |
| `lexer_participle.go` | `lexer/mod.rs`, `lexer/regex.rs` | Ordered token rules and their matcher. |
| `expression_postfix.go`, `expression_postfix_if_validator.go` | `postfix.rs` | Shunting yard and conditional validation. |
| `expression_parser.go` | `parser.rs` | Postfix to expression tree. |
| `data_tree_navigator.go`, `operators.go` | `eval.rs`, `ops/mod.rs` | Evaluation helpers and handler dispatch. |
| `operator_*.go` | `ops/` | Operators, with related Go files sometimes grouped together. |
| `decoder_yaml.go`, `candidate_node_yaml.go` | `yaml/decode.rs`, `yaml/resolve.rs`, `yaml/saphyr/` | YAML parsing, tag resolution and node construction. |
| `encoder_yaml.go`, go-yaml serializer/emitter | `yaml/encode.rs`, `yaml/emitter.rs` | YAML serialization matching the reference. |
| `decoder_json.go`, `encoder_json.go`, `candidate_node_json.go` | `json/` | Order-preserving JSON codecs. |
| `printer.go` | `printer.rs` | Scalar unwrapping, document separators and output formatting. |
| `operators_test.go` (`testScenario`, `resultToString`) | `conformance.rs` | Fixture evaluation and result formatting. |

### Nodes and evaluation

Each evaluation owns an `Arena` (`Vec<Node>`). A `NodeId` is an index into
it. Nodes retain kind, tag, style bits, scalar text, anchor and alias links,
comments, parent and key links, document/file metadata and source positions.
Mapping content is an ordered, flat key/value list. Scalars retain their
source text; operators interpret numbers when needed, including Go's
underscore, hex and octal rules.

The arena preserves Go's node identity, in-place updates, subtree copies
and parent traversal without reference-counting cycles. `Context` holds an
ordered matching-node list and a variable map. Matching lists use `Rc`
because `union` compares list identity to avoid duplicating unchanged input.
Variables use `Rc<RefCell<_>>`: `clone_context()` shares the map, as a Go
context passed by value does, while `child_context()` copies it. This is
observable when decode operators record the original node under
`decoded: <key>` and callers subsequently read it.

The lexer preserves Go's rule order: the first matching rule wins, so
`map_values` must precede `map`, `.[` must precede `[` and `...` must
precede `..`. A dedicated matcher handles the token patterns without a
general regex dependency at parse time. `regex-lite` serves expression
operators such as `match`, `test` and `sub`, and handles the fixture's
patterns.

Parsing converts tokens to postfix and then an `ExprNode` tree, retaining
Go's precedence and arity. `Evaluator::get_matching_nodes` dispatches to
handlers with this signature:

```rust
fn(&mut Evaluator, Context, &ExprNode) -> Result<Context, Error>
```

`cross_function` combines left and right results;
`compound_assign_function` implements compound assignments. Evaluation
nesting is capped at 2,000 levels. Release builds use `panic = "abort"`,
so invalid expressions must return errors rather than panic.

### YAML and JSON fidelity

The fixture's yq version uses `go.yaml.in/yaml/v4` with
`WithV3Defaults()`. The Rust encoder ports its serializer and "desolver"
pass: redundant map, sequence and inferable scalar tags are dropped, and
strings whose text would resolve to another type are quoted.
`yaml/emitter.rs` ports `internal/libyaml/emitter.go`; the reference
checkout lives in `ref/go-yaml`.

The emitter preserves flow collections, literal/folded scalars and comments,
uses block collections and configured indentation by default, and prints
`---` between documents. Plain scalar resolution follows go-yaml:
`true`/`false` are booleans; `yes`/`no` are strings; numeric forms, nulls,
timestamps and merge keys retain the reference tags. Quoted scalars resolve
as strings unless explicitly tagged otherwise.

`saphyr-parser` is vendored under `yaml/saphyr/` (MIT OR Apache-2.0). The decoder needs
anchor names (`anchor_name`) and block/flow collection styles
(`last_start_was_flow`), which its upstream API did not expose. Go-yaml's
comment scanning and attachment logic was then ported into this scanner
and parser: `scanComments`, `scanLineComment`, comment-aware
`unrollIndent`, two-token lookahead, `UnfoldComments`, `splitStemComment`
and `setEventComments`.

Two subtle details affect comment fidelity: Go's loop increments `peek`
even on `continue`, skipping the line break at the end of a comment line;
foot-comment classification then depends on a following blank line or
dedent. Go-yaml also uses 1-based columns and records comment positions
one character beyond the comment. The port retains that bookkeeping;
`go_indent()` converts the scanner's 0-based block indent.
`yaml/decode.rs` handles comment attachment between keys, values and maps.

Legacy merge-anchor explosion preserves a Go bug for `<<: [*a, *b]`:
`applyAlias` receives the alias-list position from a shadowed loop variable,
which affects key order. The internal `fix_merge_anchor_to_spec` switch
selects the corrected behavior.

The JSON parser and emitter are handwritten to preserve map order, tags and
Go's number formatting while keeping serde out of the wasm binary.

### WebAssembly ABI and host boundary

A handwritten ABI keeps the same module usable in Deno, Bun, Node and
browsers without generated wasm-bindgen glue or matching CLI versions.
`yq-core` receives all external values through `EvalOptions`; the wasm
module instantiates with an empty import object. An expression can only
read the environment variables and file contents supplied by its host.

Exports from `crates/yq-wasm`:

| Export | Signature | Purpose |
|---|---|---|
| `memory` | Linear memory | Argument and result storage. |
| `yq_alloc` | `(len: u32) -> ptr` | Allocate an argument buffer. |
| `yq_free` | `(ptr, len)` | Release an argument buffer. |
| `yq_eval` | `(expr_ptr, expr_len, input_ptr, input_len, opts_ptr, opts_len) -> header_ptr` | Evaluate UTF-8 expression, input and JSON options. |
| `yq_result_free` | `(header_ptr)` | Release a result and its header. |

The result header is 12 bytes, little-endian: `status: u32` (0 for success,
1 for error), `ptr: u32`, `len: u32`. Result bytes contain UTF-8 output or
error text. The host copies them, calls `yq_result_free`, then releases its
argument buffers with `yq_free`. Memory can grow during calls, so the host
must re-read `memory.buffer` before accessing it again. `js/yq.ts`
implements this protocol; the JSON option keys are listed above.

## Development

[AGENTS.md](AGENTS.md) contains the working rules and required checks.
`tests/fixtures/scenarios.json` is the behavior contract; its
[README](tests/fixtures/README.md) describes the result format.

```bash
cargo test --test conformance
YQ_CONFORMANCE_GROUP=addOperator cargo test --test conformance
YQ_CONFORMANCE_VERBOSE=1 YQ_CONFORMANCE_GROUP=sort cargo test --test conformance
cat target/conformance-report.txt
```

The baseline in `crates/yq-core/tests/conformance_baseline.txt` is a ratchet:
raise it in the same commit when more scenarios pass, and never lower it.
The report always includes input, expected output and actual output for
each failure; the verbose flag only changes stdout.

The harness enables `fix_merge_anchor_to_spec` for `fixed*` groups and
`disable_env_ops`/`disable_file_ops` for the corresponding security-disabled
groups, matching Go's test setup. It fills the host file table for `load`
from `tests/fixtures/examples/`.

`crates/yq-core/tests/regressions.rs` covers additional differences found by
comparison with the pinned Go binary: comments after quoted scalars, flow
collections closing at their parent's column, and scalar output defaults.
Build that binary from the fetched checkout with:

```bash
(cd ref/yq && go build -o ../yq-ref .)
```

## License

Original yq.rs contributions are available under the [MIT license](LICENSE).
Vendored and ported portions retain their upstream MIT and Apache-2.0 terms;
the combined packages declare `MIT AND Apache-2.0`. See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for source attribution and
complete license texts. Both registry packages include these notices and
the supporting license files.
