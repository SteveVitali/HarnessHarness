#!/usr/bin/env bash
# Turn GATE-G8's recorded decision into a non-PASSED state — a pending/failed
# gate stops dependent dispatch.
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p=f"{sys.argv[1]}/docs/build/LEDGER.md"
t=open(p).read()
old="**PASSED** — reviewed artifact identity"
assert old in t, "GATE-G8 decision marker absent"
open(p,"w").write(t.replace(old,"**PENDING** — reviewed artifact identity",1))
PY
