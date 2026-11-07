#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-decomposition.md"
t = open(p).read()
m = re.search(r"^\| 254 \| `[^`]+` \| R6.2 \|[^\n]*$", t, re.M)
assert m
line = m.group(0); cells = line.split("|")
cells[10] = cells[10] + " operator-ruled "
t = t.replace(line, "|".join(cells), 1)
open(p, "w").write(t)
PY
