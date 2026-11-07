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
