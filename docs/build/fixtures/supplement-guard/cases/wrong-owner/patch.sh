#!/usr/bin/env bash
# Point an adopted target at a different, real ticket — contradicting the md
# owner cell and its merged aliases' inherited owner.
set -euo pipefail
python3 - "$1" <<'PY'
import json,sys
p=f"{sys.argv[1]}/docs/build/planning/2026-10-30_evidence-coverage.json"
d=json.load(open(p))
for c in d["candidates"]:
    if c["id"]=="REQ-C-102":
        c["primary"]="R5.28"
        c["primary_file"]="docs/tickets/198_R5.28__recipe.md"
        c["acceptance_anchor"]="ac-r5-28-3"
open(p,"w").write(json.dumps(d,indent=2)+"\n")
PY
