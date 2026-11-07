#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-decomposition.md"
t = open(p).read()
t = re.sub(r"\d+k \+ 10k", "100k + 10k", t, count=1)
assert "100k + 10k" in t
open(p, "w").write(t)
PY
