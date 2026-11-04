#!/usr/bin/env bash
# Mark a Run-line-carrying contract as kind `skeleton` in the manifest —
# skeletons never dispatch. (The round's skeleton contracts were fleshed
# skeleton -> ticket, so the fixture now flips the manifest kind of a
# contract that carries a Run line; the guard must still refuse.)
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p = f"{sys.argv[1]}/docs/tickets/00_MANIFEST.md"
t = open(p).read()
old = "| 217 | `217_R5.43__useful.md` | Round 5 | ticket |"
assert old in t, "manifest row 217 not in expected ticket form"
new = "| 217 | `217_R5.43__useful.md` | Round 5 | skeleton |"
open(p, "w").write(t.replace(old, new, 1))
PY
