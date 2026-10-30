#!/usr/bin/env bash
# Record GATE-G8's decision as FAILED — a failed prerequisite stops dispatch.
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p=f"{sys.argv[1]}/docs/build/LEDGER.md"
t=open(p).read()
old="**PASSED** — reviewed artifact identity"
assert old in t, "GATE-G8 decision marker absent"
open(p,"w").write(t.replace(old,"**FAILED** — reviewed artifact identity",1))
PY
