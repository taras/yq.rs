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
with 10 skipped for XML/JSON-only decoders. The optimized release module is
468,437 bytes (204,497 bytes gzipped), within the 2 MB / 700 KB budget.
The package's smoke tests run under Node and Bun.

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
- The npm CLI does not implement `eval-all`/`ea`, front-matter extraction,
  header preprocessing, split-file output or the merge-anchor compatibility
  flag. Dynamic `load` paths are unsupported; literal file paths are preloaded.
  Single-input calls report an empty `filename` and `fileIndex` zero.
  Sequential input evaluation supplies the input index as `fileIndex`;
  filenames remain empty.

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
| `crates/yq-wasm` | `cdylib` with a hand-written ABI for evaluation, match status and memory management. No wasm-bindgen. |
| `crates/yq-cli` | Native `yq` command with the same flags and output as the wasm CLI. |
| `js/` | Wasm loader, inline package build, declarations and installed-package tests. |
| `bin/yq.js` | Node/Bun CLI source, packaged as `dist/cli.mjs`. |
| `crates/yq-core/tests/fixtures` | The conformance fixture and its format description, included in the published crate. |

## Quick start

Run a query with Node 22 or newer and npm:

```bash
printf 'name: Ada\n' | npx --yes yq.rs '.name'
# Ada
```

The npm package is `yq.rs`; it installs an executable named `yq`. The package
includes native binaries for all eight supported platforms in the same
tarball. The launcher selects the matching binary and otherwise uses its
included WebAssembly engine. CLI use needs no Rust or Go
toolchain.
See [Command line](#command-line) for project and global installation,
file editing and npm scripts.

## Build from source

For development, install Rust through rustup, Node 22 or newer, and Bun.
The toolchain file pins Rust and installs the wasm target automatically.
From a checkout, run:

```bash
cargo test
cargo build --release --target wasm32-unknown-unknown -p yq-wasm
cp target/wasm32-unknown-unknown/release/yq_wasm.wasm js/yq.wasm
npm ci
cargo build --release -p yq-cli
npm run build
npm test
```

Check the wasm size against the 2 MB uncompressed / 700 KB gzipped budget:

```bash
wc -c < js/yq.wasm
gzip -n -9 -c js/yq.wasm | wc -c
```

`npm test` packs and installs the built package in an isolated directory,
then exercises its inline and raw entries and installed `yq` executable
under both Node and Bun. Use
`npm run smoke:node` or `npm run smoke:bun` to test a single runtime.

A normal `npm run build` copies any available native builds from
`target/native-artifacts` into `dist/native`; without them, development
packages use wasm. To build and stage the current Mac arm64 binary, run
`python3 .github/release/build_native.py darwin-arm64` before `npm run build`.
Use the appropriate ID from [.github/release/platforms.json](.github/release/platforms.json)
on other hosts. Release verification requires every platform build.

To run the native CLI from the checkout:

```bash
printf 'a: [1, 2]\n' | cargo run -q -p yq-cli -- '.a | map(. * 2)'
```

For a fresh reference checkout matching the conformance fixture:

```bash
git clone https://github.com/mikefarah/yq.git ref/yq
git -C ref/yq checkout a814ff5166040ec745a96bddd79ee31c3a548e83
```

See [the fixture README](crates/yq-core/tests/fixtures/README.md) for the conformance
result format.

## Using yq-core from Rust

```bash
cargo add yq-core
```

```rust
use yq_core::{evaluate, EvalOptions, Format};

fn main() -> Result<(), yq_core::Error> {
    let mut options = EvalOptions::default();
    options.output_format = Format::Json;
    let output = evaluate(".a.b[0]", "a:\n  b: [1, 2]\n", &options)?;
    assert_eq!(output, "1\n");
    Ok(())
}
```

`EvalOptions` is non-exhaustive: start with `Default` and set the fields you
need. The supported public API is `evaluate`, `evaluate_with_status`,
`evaluate_sequence_with_status`, `Evaluation`, `SequenceError`,
`EvalOptions`, `Format` and `Error`. Engine modules are private; the
doc-hidden conformance harness exists for the upstream tests and is outside
that API. Rust 1.87 or newer is required.

## Using the module

Install the same package in Node or Bun:

```bash
npm install yq.rs
# or: bun add yq.rs
```

```ts
import { yq, createYq } from "yq.rs";

yq.evaluate(".a.b[0]", "a:\n  b: [1, 2]\n");          // "1\n"
yq.evaluate(".[] | .name", '[{"name":"Ada"}]', {
  inputFormat: "json",
  outputFormat: "json",
});                                               // '"Ada"\n'

const isolated = await createYq();
isolated.evaluate(".name", "name: Ada\n");          // "Ada\n"
```

The primary import embeds wasm as base64: it needs no file access or
TypeScript runtime support. It exports a ready `yq` instance and an async
`createYq()` factory for independent instances. `yq.rs/inline`
provides the same entry. The package also installs the `yq` executable
for Node and Bun.

Programmatic imports always use wasm on both Node and Bun. The CLI's native
binaries do not change the JavaScript library API. `YQ_FORCE_WASM=1` selects
the wasm CLI explicitly.

For bundlers or hosts that load wasm themselves, use the raw asset and
explicit loader:

```ts
import { readFile } from "node:fs/promises";
import { createYq } from "yq.rs/loader";

const bytes = await readFile(new URL(import.meta.resolve("yq.rs/yq.wasm")));
const yq = await createYq(bytes);
```

The explicit loader also accepts a compiled `WebAssembly.Module`; bundlers
can resolve `yq.rs/yq.wasm` as an asset URL. The module
instantiates with an empty import object.
`evaluate` returns the printed output and throws `YqError` on evaluation
errors. `evaluateWithStatus` returns `{ output, matched }`, where `matched`
means at least one result is neither null nor boolean false. Rust exposes
the same result through `evaluate_with_status`. This distinguishes the
string `"false"` from boolean `false` even when their printed output matches.

`evaluateSequenceWithStatus(expression, inputs, options)` accepts an array
of input strings. It evaluates each independently with a shared printer, so
file separators follow result-node metadata. Variables and collected nodes
do not cross input boundaries; this is sequential `eval`, not `eval-all`.
`fileIndex` is the input position and `filename` is empty. Rust exposes this
as `evaluate_sequence_with_status`. A later failure returns earlier printed
output in `SequenceError.output`, or `YqError.output` in JavaScript. With
`nullInput`, the expression is evaluated once, ignoring the input array.

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
| `printDocSeparators` | boolean | true | Emit YAML document separators; false implements `-N`. |
| `env` | object of strings | `{}` | Values for `env`, `strenv` and `envsubst`. |
| `now` | RFC 3339 string | unset | Value of `now`; unset makes it an error. |
| `files` | object of strings | `{}` | File contents for `load`, `load_str`, `load_props` and `load_base64`. |

## Command line

### Run with npx

Run directly from the `yq.rs` package without adding a project dependency:

```bash
npx --yes yq.rs '.name' config.yml
printf 'items: [1, 2]\n' | npx --yes yq.rs -o json '.items'
npx --yes yq.rs --help
```

You can also select the package and executable explicitly:

```bash
npx --yes --package=yq.rs yq '.name' config.yml
```

[npx](https://docs.npmjs.com/cli/v11/commands/npx/) fetches the package when
needed; `--yes` accepts its installation prompt. Use `yq.rs` as the package
name for these one-off runs. After a local install, `npx yq` uses the project's
`yq` executable.

### Install in a project

Add the CLI as a development dependency:

```bash
npm install --save-dev yq.rs
# or: bun add --dev yq.rs
npx yq '.name' config.yml
```

npm scripts resolve the installed `yq` executable automatically. Add a script
to your `package.json`:

```json
{
  "scripts": {
    "query": "yq '.name' config.yml"
  }
}
```

Then run `npm run query` (or `bun run query`). Commit `package.json` and the
lockfile so teammates and CI install the same version with `npm ci`.

### Install globally

Install once to invoke `yq` directly from your shell:

```bash
npm install --global yq.rs
yq --version
yq '.name' config.yml
```

### Query and edit files

These examples use a project-local installation:

```bash
npx yq eval '.items[] | select(.enabled)' config.yml
npx yq -i '.version = "2.0"' package.json
npx yq -o json '.' config.yml
npx yq --from-file query.yq config.yml
npx yq -e '.enabled == true' config.yml
```

The last command exits with status 1 when `.enabled` is not true, making it
useful in CI checks. Use `--help` to see available flags.

### Supported behavior

`yq [eval|e] <expression> [files...]` evaluates files independently, reading
stdin when no file is supplied or a filename is `-`. `--expression` forces
an expression and `--from-file` loads it from a file. `-i`/`--inplace` writes
the combined results into the first input file, preserving its permissions.
YAML comments, anchors and scalar styles follow the engine's normal output.

Supported flags are `-p`/`--input-format`, `-o`/`--output-format` (YAML/JSON),
`-I`/`--indent`, `-n`/`--null-input`, `-r`/`--unwrapScalar` (including `=false`),
`-e`/`--exit-status`, and `-N`/`--no-doc`. JSON filenames select JSON input
and output by default; other filenames and stdin default to YAML. An explicit
input format defaults output to YAML unless an output format is also set.
`-M` and `-C` are accepted; output has no ANSI colors. `--help` lists the
supported and deferred behavior; `--version` reports the npm package version
and pinned Go yq version. Unlike Go yq, explicit input format flags do not
emit the timestamped compatibility warning about output defaults. Exit
status is 1 for errors and, with `-e`, when
all results are null/false or no results were produced.
Broken pipe errors exit silently with status 141, matching Go yq.

The CLI supplies `process.env` and the current clock value. It preloads
string-literal paths used in `load`, `load_str`, `load_props` and
`load_base64`; computed and interpolated load paths fail clearly. The library
continues to receive these capabilities only through explicit options.

`eval-all`/`ea`, split-file output, front matter, header preprocessing,
`--yaml-fix-merge-anchor-to-spec`, and other document formats are unsupported.
Input `filename` remains empty; `fileIndex` tracks the input position.

The npm launcher selects `dist/native/<platform>/yq` (`yq.exe` on Windows)
from its own package and runs it with inherited stdio. It loads wasm when
the platform is unsupported, the binary is missing, or the binary cannot
be started. An executed binary's exit status or signal is propagated without
retrying the evaluation.

Supported builds are macOS arm64/x64, Linux x64/arm64 with glibc 2.28 or
newer, Linux x64/arm64 with musl, and Windows x64/arm64. All eight binaries ship in
one `yq.rs` tarball, without runtime or optional dependencies, install-time
downloads, or build scripts. Omitting optional dependencies does not affect
the native CLI. Bundling the binaries increases the compressed npm package
from roughly 0.5 MB to an estimated 4 MB; users download all platforms so an
installation cannot lose its native package through lockfile resolution.

The npm launcher still starts Node before spawning native `yq`. To avoid
that startup cost, use the matching `yq-<platform>` binary from the GitHub
release directly (`chmod +x` on macOS/Linux), or build `cargo build --release
-p yq-cli` and run `target/release/yq`. Both expose the same CLI flags.

Run installed-package CLI comparisons against the pinned Go oracle with:

```bash
(cd ref/yq && go build -o ../yq-ref .)
# Build and stage the host binary, then rebuild the npm package first.
# For example, on macOS arm64:
python3 .github/release/build_native.py darwin-arm64
npm run build
npm run test:cli
# An external oracle path can be supplied with YQ_REFERENCE=/path/to/yq.
# Add -- node, -- bun, -- native or -- launcher to test a single runtime.
# Set YQ_NATIVE=/path/to/yq to use a different native build.
```

This command requires the oracle and compares stdout, stderr, exit status,
and in-place output across the wasm Node/Bun CLIs, native binary and installed
Node launcher. The launcher runtime unsets `YQ_FORCE_WASM` and hides the
installed wasm CLI and inline module, so every comparison must use its bundled
native binary. Decoder error wording can differ from Go outside the
conformance fixture; the tests check retained output and failure status
separately for those cases. The regular `npm test` also tests the installed
CLI on both runtimes without requiring Go. CI also builds the pinned Go
oracle with Go 1.26.0 and runs the full comparison command. Windows x64 and
arm64 jobs also run the native and launcher comparison suites. POSIX broken-pipe checks
run on Unix; Windows file-not-found wording is normalized to the engine's
portable diagnostic for comparisons.

## Performance

Measured on 2026-10-17 on an Apple M1 Max, macOS 27.0 arm64, with Node
22.23.3, Bun 1.3.14, Rust 1.99.1 and Go 1.26.0. Go yq is v4.54.1
(`a814ff5`); the Rust engine is unchanged from `98f8eb9`. Rust uses the
size-optimized release profile (`opt-level=z`, LTO, one codegen unit).
Outputs were checked byte for byte before timing. These are synthetic
workloads on one machine.

CLI medians in milliseconds, including process startup, file reading,
evaluation and printing. The startup column runs `--version`; the other
columns run `.name` on 1,024-byte and 10,000,000-byte YAML files:

| CLI | Startup ms | 1 KB ms | 10 MB ms | 1 KB peak RSS MiB | 10 MB peak RSS MiB |
|---|---:|---:|---:|---:|---:|
| Go | 8.7 | 8.7 | 135.2 | 14.3 | 44.9 |
| Rust native | 3.9 | 4.1 | 283.6 | 2.5 | 50.5 |
| npm native (Node) | 38.1 | 38.1 | 319.6 | 41.5 | 50.5 |
| wasm (Node) | 40.5 | 46.0 | 556.5 | 51.5 | 179.6 |
| wasm (Bun) | 29.3 | 34.5 | 335.4 | 51.7 | 184.8 |

Timings use 11 serial runs after two warmups, with shuffled implementation
order and warm filesystem caches. RSS is the median of three separate
`/usr/bin/time -l` runs. For the npm native launcher, that is the largest
individual process RSS, not the combined Node-plus-child memory footprint.
The inputs contain `name: Ada` and a sequence of mappings with roughly
1 KB string values; padding comments make their byte sizes exact.

The standalone native binary starts fastest. The npm launcher still pays
Node startup: its 1 KB lookup is about 17% faster than wasm in Node, while
its 10 MB lookup is about 43% faster. Go remains faster for the large YAML
input. Bun's wasm path is substantially faster than Node's on this input.

The Node-API addon is no longer shipped. The native CLI handles process
invocations, while library callers use the portable wasm API. A separate
addon package can be reconsidered if a Node workload justifies it.

Reproduce from a built checkout with the packed `yq.rs` package installed in
`target/bench/install`:

```bash
python3 .github/bench/cli.py --go /path/to/go-yq --node /path/to/node22 \
  --entry target/bench/install/node_modules/yq.rs/dist/cli.mjs
```

Benchmark source: [CLI benchmark](.github/bench/cli.py). Raw samples are
written to the ignored `target/bench` directory and are not checked into
the repository.

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

A single UTF-8 BOM at the start of the YAML decoder's stream is consumed
after leading-content preprocessing, matching the pinned Go decoder.
In-stream BOMs and U+FEFF inside scalar values remain content. JSON input
rejects a leading BOM, matching Go's JSON decoder. The JavaScript loader
also preserves U+FEFF at the start of printed results.

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

A handwritten ABI keeps the module usable in Node and Bun without
generated wasm-bindgen glue or matching CLI versions.
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
| `yq_eval_with_status` | Same arguments as `yq_eval` | Also reports successful evaluation without truthy matches as status 2. |
| `yq_eval_sequence` | Same arguments as `yq_eval` | Input is an ordered JSON object of string values evaluated independently with shared printing. |
| `yq_result_free` | `(header_ptr)` | Release a result and its header. |

The result header is 12 bytes, little-endian: `status: u32` (0 for success,
1 for error; `yq_eval_with_status` additionally uses 2 for success without
truthy matches; `yq_eval_sequence` uses the same statuses plus 3 for partial
failure), `ptr: u32`, `len: u32`. Status 3 contains a UTF-8 JSON object
`{ "output": "...", "error": "..." }`; other result bytes contain output or
error text. The host copies them, calls `yq_result_free`, then releases its
argument buffers with `yq_free`. Memory can grow during calls, so the host
must re-read `memory.buffer` before accessing it again. `js/yq.ts`
implements this protocol; the JSON option keys are listed above.

## Development

[AGENTS.md](AGENTS.md) contains the working rules and required checks.
`crates/yq-core/tests/fixtures/scenarios.json` is the behavior contract; its
[README](crates/yq-core/tests/fixtures/README.md) describes the result format.

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
from `crates/yq-core/tests/fixtures/examples/`.

`crates/yq-core/tests/regressions.rs` covers additional differences found by
comparison with the pinned Go binary: comments after quoted scalars, flow
collections closing at their parent's column, and scalar output defaults.
Build that binary from the fetched checkout with:

```bash
(cd ref/yq && go build -o ../yq-ref .)
```

## Releases

[The release workflow](.github/workflows/release.yml) runs on pushes and PRs.
It checks formatting, Clippy, docs, conformance, the Rust 1.87 minimum,
standalone packaged-crate tests, and installed npm-package tests in Node
and Bun. `rust-toolchain.toml` pins the release compiler; runtime versions
and checksum-verified Binaryen 133 assets are pinned in
[versions.json](.github/release/versions.json).

To reproduce the optimized package build locally, use Python 3.12 or newer:

```bash
cargo build --locked --release --target wasm32-unknown-unknown -p yq-wasm
binaryen_bin="$(python3 .github/release/install_binaryen.py)"
"$binaryen_bin/wasm-opt" -Oz --enable-bulk-memory --enable-nontrapping-float-to-int \
  target/wasm32-unknown-unknown/release/yq_wasm.wasm -o js/yq.wasm
python3 .github/release/verify.py size
npm ci
cargo build --release -p yq-cli
npm run build
npm test
```

The explicit wasm feature flags preserve the bulk-memory and saturating
float-conversion instructions emitted by the pinned Rust compiler. CI
fails above 2,000,000 raw bytes or 700,000 gzip bytes. The gzip measurement
uses compression level 9 and a zero timestamp for reproducibility.

Prepare a version change with `python3 .github/release/version.py X.Y.Z`.
It updates the workspace, npm manifest and lockfiles together; review and commit the result before tagging.
Workspace Cargo, npm and lockfile versions must agree. A pushed `vX.Y.Z`
tag must match that version and runs verification before publishing
`yq-core` to crates.io and the single `yq.rs` package to npm. The eight-platform
matrix uploads raw CLI binaries; verification stages them in `dist/native/`,
adds their Cargo dependency license texts, and packs one tarball. The verifier
requires all eight binaries, checks executable mode 0755 and verifies that
packed bytes match the native builds, including both Windows executables when
packing on Linux. Node/Bun tests and all eight platform smoke jobs install
that exact tarball; Alpine tests both musl architectures and Windows ARM64
builds and tests on `windows-11-arm`.

The publish job compares the Cargo archive with the verified artifact,
publishes the npm tarball with provenance through the existing `yq.rs`
trusted publisher, attests all artifacts, and attaches the standalone
binaries, wasm, package archives and `SHA256SUMS` to the GitHub release.
The standalone binaries let callers avoid Node startup entirely. A retry
skips an already-published version only after checking its archive integrity.
The job waits for npm processing before creating the GitHub release.

Only two registry packages need trusted publishers: `yq-core` on crates.io
and `yq.rs` on npm. Both use GitHub owner `taras`, repository `yq.rs`, workflow
`release.yml` and environment `release`; npm must allow **Direct npm publish**.
There are no new npm package names to bootstrap. Tag-triggered releases use
OIDC without long-lived registry tokens. See the
[crates.io setup](https://crates.io/docs/trusted-publishing) and
[npm setup](https://docs.npmjs.com/trusted-publishers/).

The failed `v0.1.3` run published `yq-core` only. Version `0.1.4` introduces
this single-tarball layout; the old tag is retained and is not reused.

## License

Original yq.rs contributions are available under the [MIT license](LICENSE).
Vendored and ported portions retain their upstream MIT and Apache-2.0 terms;
the combined packages declare `MIT AND Apache-2.0`. See
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for source attribution and
complete license texts. All packages include these notices and supporting license files. The npm
package also includes Cargo dependency licenses for its native binaries
and a generated dependency index.
