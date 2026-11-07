#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-coverage.json"
d = json.load(open(p))
c = next(x for x in d["candidates"] if x.get("synthesis_allocated"))
c["literal_source"] = "docs/design/40_program-plan.md"
json.dump(d, open(p, "w"), indent=1)
PY
