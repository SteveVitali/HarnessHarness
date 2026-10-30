#!/usr/bin/env bash
# Make the first-unlanded row a skeleton — skeletons never dispatch.
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p=f"{sys.argv[1]}/docs/tickets/00_MANIFEST.md"
t=open(p).read()
old="| 170 | `170_R5.2__dispatch.md` | Round 5 | ticket |"
assert old in t, "R5.2 manifest row absent"
open(p,"w").write(t.replace(old, old.replace("| ticket |","| skeleton |"),1))
PY
