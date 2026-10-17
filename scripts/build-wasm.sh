#!/usr/bin/env bash
# Builds the release wasm module and copies it next to the TypeScript loader.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --target wasm32-unknown-unknown -p yq-wasm
cp target/wasm32-unknown-unknown/release/yq_wasm.wasm js/yq.wasm
if command -v wasm-opt >/dev/null 2>&1; then
  wasm-opt -Oz js/yq.wasm -o js/yq.wasm
fi
ls -la js/yq.wasm
gzip -k -9 -f js/yq.wasm && ls -la js/yq.wasm.gz && rm js/yq.wasm.gz
