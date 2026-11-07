#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/tickets/00_MANIFEST.md"
t = open(p).read()
old = "| `290_R6.32__w1-run.md` | Round 6 | skeleton |"
assert old in t
t = t.replace(old, "| `290_R6.32__w1-run.md` | Round 6 | ticket |", 1)
open(p, "w").write(t)
PY
