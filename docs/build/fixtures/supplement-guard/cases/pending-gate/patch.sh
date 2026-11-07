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
