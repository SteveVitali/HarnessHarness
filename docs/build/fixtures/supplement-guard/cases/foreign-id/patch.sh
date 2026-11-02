#!/usr/bin/env bash
# Rename a candidate to an id outside the REQ-[A-H]-* registry.
set -euo pipefail
python3 - "$1" <<'PY'
import json,sys
p=f"{sys.argv[1]}/docs/build/planning/2026-10-30_evidence-coverage.json"
d=json.load(open(p))
d["candidates"][1]["id"]="REQ-Z-999"
open(p,"w").write(json.dumps(d,indent=2)+"\n")
PY
