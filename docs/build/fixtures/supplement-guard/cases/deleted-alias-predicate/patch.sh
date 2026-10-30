#!/usr/bin/env bash
# Erase a merged alias's literal predicate — the alias must retain its source text.
set -euo pipefail
python3 - "$1" <<'PY'
import json,sys
p=f"{sys.argv[1]}/docs/build/planning/2026-10-30_evidence-coverage.json"
d=json.load(open(p))
for c in d["candidates"]:
    if c["id"]=="REQ-E-001":
        c["literal_row"]=""
        c["requirement"]=""
open(p,"w").write(json.dumps(d,indent=2)+"\n")
PY
