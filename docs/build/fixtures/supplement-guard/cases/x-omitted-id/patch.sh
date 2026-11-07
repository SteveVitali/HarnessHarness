#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-coverage.json"
d = json.load(open(p))
d["candidates"] = [c for c in d["candidates"] if c["id"] != "REQ-XB-128"]
json.dump(d, open(p, "w"), indent=1)
PY
