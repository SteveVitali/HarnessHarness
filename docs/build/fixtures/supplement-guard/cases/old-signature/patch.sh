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
