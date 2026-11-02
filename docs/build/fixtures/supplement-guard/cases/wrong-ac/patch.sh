#!/usr/bin/env bash
# Point an adopted target's acceptance anchor at one that does not exist.
set -euo pipefail
python3 - "$1" <<'PY'
import json,sys
p=f"{sys.argv[1]}/docs/build/planning/2026-10-30_evidence-coverage.json"
d=json.load(open(p))
for c in d["candidates"]:
    if c["id"]=="REQ-C-104":
        c["acceptance_anchor"]="ac-r5-1-99"
open(p,"w").write(json.dumps(d,indent=2)+"\n")
PY
