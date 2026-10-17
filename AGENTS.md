# Working on yq.rs

## Code rules

1. Do not use braceless `if` statements.
2. Comments describe surprising behavior only. No comments that restate code.
3. `crates/yq-core` must stay free of `std::env`, `std::fs`, `std::time`,
   `std::process` and threads. Everything the engine needs from the outside
   world arrives through `EvalOptions`. This is what keeps it loadable as
   `wasm32-unknown-unknown` with an empty import object.
4. Prefer no dependency over a dependency. Every crate added to `yq-core`
   must earn its place in the wasm size check documented in `README.md`.
   Budget: 2 MB uncompressed, 700 KB gzipped, for the release module.
5. `cargo fmt` and `cargo clippy --all-targets -- -D warnings` must pass.

## The reference is the Go source, the spec is the fixture

- Clone the pinned mikefarah/yq reference into `ref/yq` (gitignored) using
  the commands in `README.md`. Port from `ref/yq/pkg/yqlib`.
  Each operator lives in `operator_<name>.go`
  with its tests in `operator_<name>_test.go`. The module map in
  `README.md` maps the Go files to the Rust modules.
- `crates/yq-core/tests/fixtures/scenarios.json` is the behaviour to match, down to the
  byte. Its README explains the result string format. When the Go code and
  the fixture seem to disagree, the fixture wins; it was dumped from the
  same commit.
- The local Go `yq` binary, when present, is a second oracle. Compare
  `yq '<expr>' file.yml` with `cargo run -q -p yq-cli -- '<expr>' file.yml`.

## Conformance loop

```bash
cargo test --test conformance                        # full run, ratchet enforced
YQ_CONFORMANCE_GROUP=addOperator cargo test --test conformance   # one group, no ratchet
YQ_CONFORMANCE_VERBOSE=1 YQ_CONFORMANCE_GROUP=sort cargo test --test conformance
cat target/conformance-report.txt                    # per-group counts and every failure
```

The test fails if fewer scenarios pass than `crates/yq-core/tests/conformance_baseline.txt`
says. When more pass, raise the baseline in the same commit. Never lower it.

## Commits and PRs

- Work on a branch; the default branch is `main`.
- Commit after every operator group that goes green, with the baseline bump
  in the same commit. Small commits make the work resumable from any point.
- Keep each PR focused on one operator group or coherent change. The
  smallest PR that serves its purpose; no unrelated improvements.
- Before pushing: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`, and `cargo build --release --target wasm32-unknown-unknown -p yq-wasm`
  must all succeed. Copy the wasm module and check its size as documented in
  `README.md`.

## Keeping the documentation current

`README.md` records the current scope, compatibility gaps, architecture and
design rationale. Keep its conformance count and supported behavior current.
When you learn something that changes the approach (a yq behaviour that
needs a different data model, a crate that does not build for wasm), record
it in the relevant architecture section before moving on.
