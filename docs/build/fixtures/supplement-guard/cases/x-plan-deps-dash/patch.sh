#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-decomposition.md"
t = open(p).read()
t = t.replace("| nothing |", "| — |", 1)
assert "| — |" in t
open(p, "w").write(t)
PY
