#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-decomposition.md"
t = open(p).read()
m = re.search(r"^\| 316 \| `[^`]+` \| R6.49 \|[^\n]*$", t, re.M)
assert m
line = m.group(0)
assert "HUMAN-H16" in line
t = t.replace(line, line.replace(", HUMAN-H16", "", 1), 1)
open(p, "w").write(t)
PY
