#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-coverage.json"
d = json.load(open(p))
c = d["candidates"][0]
c["appendix_a_row"] = c["appendix_a_row"] + " tampered"
json.dump(d, open(p, "w"), indent=1)
PY
