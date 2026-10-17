# Plan

Goal: a wasm module under 2 MB that evaluates yq expressions over YAML and
JSON with the same results as mikefarah/yq, loadable in Deno today and Bun
later with nothing but `WebAssembly.instantiate`.

Measure of progress: passing scenarios in `cargo test --test conformance`
out of 1,111 runnable (1,121 in the fixture, 10 skipped for XML/JSON-only
decoders). Record the count at each milestone.

Status on 2026-10-17: 1,107 of 1,111 pass. Every milestone below is done;
the only open items are `shuffle` (4 scenarios, needs Go's `math/rand`
sequence) and running `js/smoke.ts` under Bun and Node, which were not
installed on the build machine.

## Milestones

### 0. Scaffold  — done

Workspace, conformance harness with ratchet, wasm ABI and loader, CLI,
fixture extracted from yq commit `a814ff5` (2026-10-09). Baseline: 0.

### 1. Node model, YAML and JSON codecs, conformance runner — done

Landed 2026-10-16. Conformance: 6 of 1,111 (every scenario whose
expression is `.`, plus one that only needs the error path). The estimate
of 20 to 40 was wrong: nearly every scenario needs the expression parser.
Release wasm: 206 KB (88 KB gzipped) before any operator exists.

- `node.rs`: port `CandidateNode` (kind, tag, value, style, anchor, alias,
  content, parent, key, document index, file index, filename, comments,
  `is_map_key`). Use an arena (`Vec<Node>` with index handles) rather than
  `Rc<RefCell>`: yq mutates nodes in place through parent pointers, copies
  subtrees constantly, and compares by identity in a few places.
- `yaml/decode.rs`: YAML to nodes with tags resolved the way yaml.v3 does
  (`!!int`, `!!float`, `!!bool`, `!!null`, `!!str`, `!!merge`, `!!timestamp`),
  styles recorded, anchors and aliases wired, multiple documents, document
  index and filename stamped on roots.
- `yaml/encode.rs`: nodes to YAML matching yaml.v3's output rules: 2-space
  indent (configurable), block style unless the node is flow, quoting only
  when the plain scalar would change type, `---` between documents, line
  and head comments when present.
- `json/decode.rs`, `json/encode.rs`: order-preserving, with yq's
  pretty-print and indent rules.
- `printer.rs`: port `printer.go` (unwrap scalar, doc separators).
- `conformance.rs`: parse `document`/`document2` as `sample.yml`/`another.yml`,
  evaluate, print each result as `D<doc>, P<path>, (<tag>)::<yaml indent 4>`.

Done when: `cargo test` reports the harness running every scenario and
evaluation of `.` passes for every scenario whose expression is `.` or that
only needs identity. Expected count: 20 to 40.

### 2. Lexer, postfix, expression tree, core operators — done

Landed 2026-10-17 (644 of 1,111 on its own; every target group above
90% once the milestone 3 operators they lean on existed).

Port `lexer_participle.go` (token table with the same rule order, since
order resolves ambiguity), `expression_postfix.go` (shunting yard with
yq's precedence table from `operation.go`), `expression_parser.go`,
`expression_postfix_if_validator.go`. Then the operators everything else
builds on:

self, traverse path, traverse array, pipe, short pipe, union, value,
collect, collect object, create map, assign (`=`, `|=`), add, subtract,
multiply (merge with its flags), divide, modulo, equals, compare, and, or,
not, alternative, select, recursive descent, string interpolation, variables
(`as`, `ref`, `$x`), `ireduce`, if/then/elif/else/end, block (`;`), `with`,
map, map_values, length, keys, has, del, empty.

Done when: traversePath, pipe, union, assign, add, multiply, booleans,
equals, compare, select, collect, collectObject, createMap, variables,
recursiveDescent, delete and keys groups are each above 90%.
Expected count: 500 to 600.

### 3. The long tail — done

Landed 2026-10-17: 1,062 of 1,111 with every group except `load`,
`shuffle` and comments above 90%. `regex-lite` handles every pattern in
the fixture. Not ported: `load`/`load_str` (no filesystem; they return
yq's security error when file operations are disabled and a clear error
otherwise), `system` (returns yq's disabled error), `shuffle` (Go's
`math/rand` sequence would have to be reproduced bit for bit), and the
XML, TOML, HCL, Lua and INI formats.

entries (to/from/with), sort, sort_by, sort_keys, reverse, unique,
unique_by, group_by, min, max, flatten, any, all, any_c, all_c, contains,
first, pick, omit, pivot, array_to_map, path, paths, getpath, setpath,
del_paths, parent, parents, key, is_key, kind, tag, type, style, anchor,
alias, explode, document_index, filename, file_index, line, column, split,
join, sub, match, test, capture, trim, upcase, downcase, to_string,
to_number, env, strenv, envsubst, `@base64`/`@base64d`, `@uri`/`@urid`,
`@sh`, `@json`, `to_json`/`from_json`, `to_yaml`/`from_yaml`, error, eval,
splitDoc, now, tz, format_datetime, from_unix, to_unix, with_dtf, shuffle
(seeded so it is deterministic in tests), slice.

Regex: yq uses Go's RE2 syntax. `regex-lite` keeps the wasm small; check
which fixture patterns it rejects before choosing `regex` instead.

Done when: every group except comments, anchors/aliases, style and load
is above 90%. Expected count: 850 to 950.

### 4. wasm module and loader — done except Bun/Node runs

Release wasm on 2026-10-17: 518 KB uncompressed, 206 KB gzipped (no
`wasm-opt` on this machine). `js/smoke.ts` passes under Deno; Bun and
Node were not available to try. `yq-core` has no `unwrap`/`expect` outside
tests; expression nesting is capped at 2,000 levels to keep recursion
bounded.

- `scripts/build-wasm.sh` produces `js/yq.wasm`; record size here.
- `js/smoke.ts` passes under Deno, Bun and Node.
- Panics must not reach the host: `yq_eval` catches nothing under
  `panic = "abort"`, so the engine must return `Err` instead of panicking.
  Audit `unwrap`/`expect`/indexing in `yq-core`.
- Size budget: 2 MB uncompressed, 700 KB gzipped. If over, profile with
  `twiggy` or `cargo bloat` before cutting features.

### 5. Fidelity — done except `load`

Landed 2026-10-17: 1,107 of 1,111. Comments, anchors, aliases, styles and
`load` all pass. `load` reads from `EvalOptions::files`, a path-to-contents
table the host supplies (the conformance harness fills it from
`tests/fixtures/examples/`, copied from yq's `examples/`). The 4 remaining
scenarios need `shuffle`, which would require reproducing Go's `math/rand`
sequence bit for bit.

- Comment preservation: head, line and foot comments through decode,
  operators and encode. This needs a comment-aware YAML scanner, since no
  Rust YAML parser exposes comments. Decide between extending the parser in
  use and writing a scanner ported from yaml.v3's `scannerc.go`.
- Anchor and alias behaviour under merge (`*`, `explode`), the
  `fixedAnchorOperatorScenarios` group.
- Style round-tripping (`styleOperatorScenarios`).
- `load`, `strload`: served from the `files` option rather than a
  callback, so the module still instantiates with an empty import object.

Target: above 1,050 of 1,111.

## Decisions and findings

- 2026-10-16: No existing Rust port of yq. jaq (Rust jq) exists but jq syntax
  is not yq syntax (`tag`, `with`, `env(NAME)`, `ireduce`, comments, styles).
  Go yq compiles to wasip1 but is 14.5 MB stripped and lean, too large for xmd.
  A Rust jq engine with YAML and regex (jaq) compiles to 0.9 MB on the same
  target, which sets the expectation for this project.
- 2026-10-16: Hand-written ABI instead of wasm-bindgen so one module serves
  Deno, Bun, Node and browsers without generated glue and without matching
  wasm-bindgen CLI versions.
- 2026-10-16: YAML parsing. Start with `saphyr-parser` (pure Rust, event
  based, exposes tags, styles, anchors, aliases, marks; no comments) behind
  a small event interface so a comment-aware scanner can replace it in
  milestone 5. Write the emitter by hand; yq's output format is yaml.v3's,
  and no Rust emitter reproduces it.
- 2026-10-16: JSON. Write the parser and emitter by hand; order preservation,
  yq's number formatting and tag assignment are simpler to own than to adapt
  from serde_json, and it keeps serde out of the wasm binary.
- 2026-10-16: The yq commit the fixture came from uses `go.yaml.in/yaml/v4`
  with `WithV3Defaults()`, not yaml.v3 itself. v4 adds a "desolver" pass
  before serialising (drops `!!map`/`!!seq`/inferable scalar tags, and
  double-quotes a `!!str` whose text would resolve to another type) and the
  `CompactSeqIndent` emitter option. `yaml/encode.rs` ports the desolver
  and serializer; `yaml/emitter.rs` is a line-for-line port of
  `internal/libyaml/emitter.go` so the output matches byte for byte. The
  reference checkout lives in `ref/go-yaml`.
- 2026-10-16: `saphyr-parser` is vendored into `yaml/saphyr/` (MIT) rather
  than depended on, with two local changes: anchor names are exposed on the
  parser (`anchor_name`), and collection start events report block vs flow
  style (`last_start_was_flow`). Both are needed by the decoder and neither
  is in the upstream API.
- 2026-10-17: Comments. Rather than a new scanner, go-yaml's comment
  logic (`scanComments`, `scanLineComment`, the comment-aware
  `unrollIndent`, the two-token lookahead, `UnfoldComments`,
  `splitStemComment`, `setEventComments`) was ported into the vendored
  saphyr scanner and parser. Two things matter for fidelity: go-yaml's
  `for` loop increments `peek` on `continue`, so the line break that ends
  a comment line is never examined (it only becomes a foot comment when a
  blank line or a dedent follows); and go-yaml's columns are 1-based with
  `scanComments` recording them one past the character, which the Rust
  port keeps (`go_indent()` converts the scanner's 0-based block indent).
  The composer rules for moving foot comments between keys, values and
  mappings live in `yaml/decode.rs`.
- 2026-10-17: The Go test suite flips `FixMergeAnchorToSpec` on for the
  `fixed*` groups and `DisableEnvOps`/`DisableFileOps` for the
  `*SecurityDisabled*` groups; the conformance runner sets the matching
  `Evaluator` flags from the group name.
- 2026-10-17: A Go `Context` passed by value shares its variables map, so
  a variable set inside an operator (the decode operator records the
  original node under `decoded: <key>`) is visible to the caller. The Rust
  `Context` keeps the map behind `Rc<RefCell>`; `clone_context()` shares it
  and `child_context()` copies it, matching Go's `ChildContext`.
- 2026-10-17: The legacy merge-anchor explode reproduces a Go bug: for
  `<<: [*a, *b]` the index passed to `applyAlias` is the position inside
  the alias list (a shadowed loop variable), which changes key order.
- 2026-10-16: `target/conformance-report.txt` always records document,
  expected and actual for each failure; `YQ_CONFORMANCE_VERBOSE` only
  affects stdout. The unattended permission allowlist rejects commands with
  a leading `VAR=value`, so the report file is the way to see failures.
