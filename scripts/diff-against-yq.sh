#!/usr/bin/env bash
# Compares yq-rs with the reference yq on one expression and input file.
#   scripts/diff-against-yq.sh '.a.b' input.yml
# Requires the Go yq on PATH (https://github.com/mikefarah/yq).
set -uo pipefail
cd "$(dirname "$0")/.."
expression="$1"; shift
diff <(yq "$expression" "$@" 2>&1) <(cargo run -q -p yq-cli -- "$expression" "$@" 2>&1) && echo "identical"
