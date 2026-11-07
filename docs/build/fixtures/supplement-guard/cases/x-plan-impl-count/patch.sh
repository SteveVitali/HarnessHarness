#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-decomposition.md"
t = open(p).read()
m = re.search(r"\*\*implement-spec rows\*\* \| \*\*(\d+)\*\*", t)
assert m
t = t.replace(m.group(0), m.group(0).replace(m.group(1), str(int(m.group(1)) + 7)), 1)
open(p, "w").write(t)
PY
