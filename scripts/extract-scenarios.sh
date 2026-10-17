#!/usr/bin/env bash
# Regenerates tests/fixtures/scenarios.json from the Go yq test suite in
# ref/yq. Needs a Go toolchain. Drops the generated dump test afterwards so
# the reference checkout stays clean.
set -euo pipefail
cd "$(dirname "$0")/.."
scripts/fetch-reference.sh ${1:+"$1"}
pkg=ref/yq/pkg/yqlib
python3 - "$pkg" <<'PY'
import re, glob, sys, os
pkg = sys.argv[1]
names = sorted({m for f in glob.glob(f"{pkg}/*_test.go")
                for m in re.findall(r'^var ([A-Za-z0-9_]+) = \[\]expressionScenario', open(f).read(), re.M)})
template = open("scripts/dump_scenarios_test.go.tmpl").read()
calls = "".join(f'\tdumpGroup(&all, "{n}", {n})\n' for n in names)
open(f"{pkg}/zz_dump_scenarios_test.go", "w").write(template.replace("\t//DUMP_GROUPS\n", calls))
print(f"{len(names)} scenario groups")
PY
out="$PWD/tests/fixtures/scenarios.json"
(cd "$pkg" && SCENARIO_DUMP="$out" GOFLAGS=-mod=mod go test -run '^TestDumpScenarios$' . >/dev/null)
rm -f "$pkg/zz_dump_scenarios_test.go"
echo "wrote $out ($(python3 -c "import json;print(len(json.load(open('$out'))))") scenarios)"
