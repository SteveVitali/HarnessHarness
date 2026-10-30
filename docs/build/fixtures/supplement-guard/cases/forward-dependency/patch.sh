#!/usr/bin/env bash
# Give R5.1 a dependency on a later chain row.
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p=f"{sys.argv[1]}/docs/tickets/169_R5.1__guard.md"
t=open(p).read()
old="- **Depends on:** GATE-G8"
new="- **Depends on:** GATE-G8, R5.55"
assert old in t, "expected Depends header absent"
open(p,"w").write(t.replace(old,new,1))
PY
