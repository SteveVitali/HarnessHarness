#!/usr/bin/env bash
# Hand a skeleton contract a Run line — skeletons never dispatch.
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p=f"{sys.argv[1]}/docs/tickets/217_R5.43__useful.md"
t=open(p).read()
old="- **Depends on:** GATE-G11, R5.28"
new=old+"\n- **Run:** `implement-spec spec=docs/tickets/217_R5.43__useful.md`"
assert old in t, "expected Depends header absent"
open(p,"w").write(t.replace(old,new,1))
PY
