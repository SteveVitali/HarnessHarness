#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-coverage.json"
d = json.load(open(p))
c = next(x for x in d["candidates"] if x.get("round_scope", "").startswith("routed"))
c["primary"] = "CAP6.1"
c["primary_file"] = "docs/tickets/321_CAP6.1__cap1.md"
json.dump(d, open(p, "w"), indent=1)
PY
