# yq.rs

A Rust implementation of the [yq](https://github.com/mikefarah/yq) expression
language, built to compile to a small WebAssembly module that any JavaScript
runtime can load with the plain `WebAssembly` API. It exists so that
[executable.md](https://github.com/thefrontside/executable.md) (`xmd`) can
query and transform objects with yq syntax without shipping the Go yq's
15 MB wasm build.

**Status: 1,107 of 1,111 conformance scenarios pass** (the remaining four
need `shuffle`, whose output depends on Go's random number generator).
Milestones 1–5 are implemented; [PLAN.md](PLAN.md) records progress and
findings, [AGENTS.md](AGENTS.md) the working rules, and
[docs/design.md](docs/design.md) the architecture. The conformance fixture
comes from yq commit `a814ff5`: 1,121 scenarios, with 10 skipped for
XML/JSON-only decoders. The release module is 521,537 bytes
(207,348 bytes gzipped), within the 2 MB / 700 KB budget. The loader's
smoke tests pass under Deno, Bun and Node.

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

Out of scope: XML, TOML, HCL, Lua and INI formats; `system`, `shuffle`, file
writes; anything that needs a filesystem, a clock or process environment
inside the engine. The host supplies environment variables, the current time
and file contents through options.

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
| `scripts/` | Fetch the Go reference, regenerate the fixture, build the wasm, diff against yq. |

## Quick start

Install Rust with Cargo and at least one of Deno, Bun or Node with TypeScript
stripping support, then run:

```bash
rustup target add wasm32-unknown-unknown
cargo test                      # runs the conformance ratchet, see target/conformance-report.txt
scripts/build-wasm.sh           # release wasm into js/yq.wasm, prints size
deno run --allow-read js/smoke.ts
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

To compare behaviour while developing, `scripts/fetch-reference.sh` fetches
the Go source into `ref/`. See [the fixture README](tests/fixtures/README.md)
for the conformance result format.

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
