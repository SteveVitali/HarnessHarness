#!/usr/bin/env bash
# Duplicate a candidate row — one literal id, two owners' worth of state.
set -euo pipefail
python3 - "$1" <<'PY'
import json,sys
p=f"{sys.argv[1]}/docs/build/planning/2026-10-30_evidence-coverage.json"
d=json.load(open(p))
d["candidates"].append(dict(d["candidates"][1]))
open(p,"w").write(json.dumps(d,indent=2)+"\n")
PY
