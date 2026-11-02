#!/usr/bin/env bash
# Give R5.2 an undecided gate dependency — a pending gate stops dispatch.
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p=f"{sys.argv[1]}/docs/tickets/170_R5.2__dispatch.md"
t=open(p).read()
old="- **Depends on:** R5.1"
assert old in t, "Depends header absent"
open(p,"w").write(t.replace(old, old+", GATE-G9",1))
PY
