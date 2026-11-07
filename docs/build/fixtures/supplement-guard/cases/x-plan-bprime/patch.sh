#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-decomposition.md"
t = open(p).read()
t = t.replace("<!-- linearization-B-prime: R6.6 R6.20 R6.21 HUMAN-H12 GATE-G22 R6.31 -->",
              "<!-- linearization-B-prime: R6.6 R6.20 R6.21 HUMAN-H12 GATE-G22 R6.31 R6.5 -->", 1)
assert "R6.31 R6.5 -->" in t
open(p, "w").write(t)
PY
