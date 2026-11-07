#!/usr/bin/env bash
# Reuse GATE-G5's recorded verbatim as GATE-G8's signature — old acceptance
# signatures cannot satisfy new acceptance.
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p=f"{sys.argv[1]}/docs/build/readouts/GATE-G8.md"
t=open(p).read()
old='"passed. please proceed"'
assert old in t, "GATE-G8 verbatim absent"
open(p,"w").write(t.replace(old,'"passed, proceed"',1))
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
