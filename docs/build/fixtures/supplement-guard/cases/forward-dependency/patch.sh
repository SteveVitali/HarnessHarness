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
# 2026-11-07 (Round-6 insertion): this case exercises round-5 rows; the guard's
# active-round checks apply only to the LEDGER's active round, which is now 6.
# Pin the scratch copy's active round to 5 so the case keeps testing what it was
# written for (round-6 equivalents: cases round6-*).
python3 - "$1" <<'PY'
import re, sys
p = f"{sys.argv[1]}/docs/build/LEDGER.md"
t = open(p).read()
t, n = re.subn(r"(?m)^(round:\s*)\S+", r"\g<1>5", t, count=1)
assert n == 1, "round key absent"
open(p, "w").write(t)
PY
