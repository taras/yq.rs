# yq.rs

A Rust implementation of the [yq](https://github.com/mikefarah/yq) expression
language, built to compile to a small WebAssembly module that any JavaScript
runtime can load with the plain `WebAssembly` API. It exists so that
[executable.md](https://github.com/thefrontside/executable.md) (`xmd`) can
query and transform objects with yq syntax without shipping the Go yq's
15 MB wasm build.

**Status: 1,107 of 1,111 conformance scenarios pass** (the remaining four
need `shuffle`, whose output depends on Go's random number generator).
`PLAN.md` has the build order and findings, `AGENTS.md` the working rules,
`docs/design.md` the architecture. Progress is measured by the conformance
suite: yq's own 1,100+ operator test scenarios, extracted verbatim into
`tests/fixtures/scenarios.json`. The release module is about 520 KB
(210 KB gzipped).

## Scope

- The complete yq expression language: traversal, pipes, assignment,
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

```bash
cargo test                      # runs the conformance ratchet, see target/conformance-report.txt
scripts/build-wasm.sh           # release wasm into js/yq.wasm, prints size
deno run --allow-read js/smoke.ts
bun js/smoke.ts
scripts/fetch-reference.sh      # Go yq source into ref/ for reading while porting
```

## Using the module

```ts
import { createYq } from "./js/yq.ts";
const yq = await createYq(await Deno.readFile("js/yq.wasm"));
yq.evaluate(".a.b[0]", "a:\n  b: [1, 2]\n");          // "1\n"
yq.evaluate(".[] | .name", json, { inputFormat: "json", outputFormat: "json" });
```
