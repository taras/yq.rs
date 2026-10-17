#!/usr/bin/env bash
# Clones the Go yq source into ref/yq (gitignored) for reading while porting.
# The fixture in tests/fixtures was extracted from the commit recorded in
# tests/fixtures/README.md; pass a ref as $1 to check out something else.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ ! -d ref/yq ]; then
  git clone --quiet https://github.com/mikefarah/yq.git ref/yq
fi
if [ $# -gt 0 ]; then
  git -C ref/yq checkout --quiet "$1"
fi
echo "reference at ref/yq ($(git -C ref/yq rev-parse --short HEAD))"
