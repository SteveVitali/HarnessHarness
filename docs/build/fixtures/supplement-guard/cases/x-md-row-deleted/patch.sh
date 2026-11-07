#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-coverage.md"
t = open(p).read()
lines = t.split("\n")
i = next(i for i, l in enumerate(lines) if re.match(r"^\| REQ-XB-128 \|", l))
del lines[i]
open(p, "w").write("\n".join(lines))
PY
