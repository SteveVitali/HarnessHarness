#!/usr/bin/env bash
# Drop REQ-A-002's candidate row — the supplement registry still names it.
set -euo pipefail
python3 - "$1" <<'PY'
import json,sys
p=f"{sys.argv[1]}/docs/build/planning/2026-10-30_evidence-coverage.json"
d=json.load(open(p))
d["candidates"]=[c for c in d["candidates"] if c["id"]!="REQ-A-002"]
open(p,"w").write(json.dumps(d,indent=2)+"\n")
PY
