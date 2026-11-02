#!/usr/bin/env bash
# Route at the legacy GATE-G1 — a completed historical row never dispatches.
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p=f"{sys.argv[1]}/docs/build/LEDGER.md"
t=open(p).read()
old="nextTicket:      R5.1"
assert old in t, "nextTicket cell absent"
open(p,"w").write(t.replace(old,"nextTicket:      GATE-G1",1))
PY
