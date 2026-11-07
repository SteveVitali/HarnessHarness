#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/tickets/289_GATE-G25__prov-w1.md"
t = open(p).read()
m = re.search(r"^> \|[^\n]+compute quote[^\n]*$", t, re.M)
assert m, "quoted gate row"
t = t.replace(m.group(0), m.group(0).replace("compute quote", "compute quuote"), 1)
open(p, "w").write(t)
PY
