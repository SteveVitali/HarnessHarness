#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-coverage.json"
d = json.load(open(p))
c = next(x for x in d["candidates"] if x["disposition"] != "adopted")
c["literal_row"] = ""
json.dump(d, open(p, "w"), indent=1)
PY
