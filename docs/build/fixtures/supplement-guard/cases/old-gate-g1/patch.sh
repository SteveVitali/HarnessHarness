#!/usr/bin/env bash
# Route at the legacy GATE-G1 — a completed historical row never dispatches.
# Reads the live nextTicket value rather than pinning it (BM-TEST-01: a
# fixture must not pin a living record — R5.2 repair, the R5.1 pin broke the
# moment nextTicket advanced).
set -euo pipefail
python3 - "$1" <<'PY'
import re, sys
p=f"{sys.argv[1]}/docs/build/LEDGER.md"
t=open(p).read()
m=re.search(r"^nextTicket:\s+(\S+)", t, re.M)
assert m, "nextTicket cell absent"
open(p,"w").write(t[:m.start(1)]+"GATE-G1"+t[m.end(1):])
PY
