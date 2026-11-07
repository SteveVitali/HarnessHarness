#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-coverage.json"
d = json.load(open(p))
c = next(x for x in d["candidates"] if x["id"] == "REQ-XH-001")
c["ac_hook"] = "AC-9.9-9"
json.dump(d, open(p, "w"), indent=1)
PY
