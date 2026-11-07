#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, re
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-decomposition.md"
t = open(p).read()
m = re.search(r"### Shared-decision ownership[^\n]*\n(.*?)(\n### |\Z)", t, re.S)
assert m, "shared-decision section"
seg = m.group(1)
# first real decision row: owner cell holds a chain id
mm = re.search(r"^\| [^|\n]+ \| (R6\.\d+|CAP6\.\d+|GATE-G\d+|HUMAN-H\d+) \|", seg, re.M)
assert mm, "decision row"
t = t.replace(mm.group(0), mm.group(0).replace("| " + mm.group(1) + " |", "| R6.50 |", 1), 1)
open(p, "w").write(t)
PY
