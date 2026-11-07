#!/usr/bin/env bash
# A `correction`-kind GATE DECISIONS row is an annotation, never the gate's
# decision record: it must neither hide a PASSED decision nor mask a real
# non-PASSED one. Flip GATE-G8's decision to PENDING, then append a
# correction row keyed GATE-G8 whose body contains PASSED text — the guard
# must still emit failed-gate (the correction cannot launder the decision).
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p = f"{sys.argv[1]}/docs/build/LEDGER.md"
t = open(p).read()
old = "**PASSED** — reviewed artifact identity"
assert old in t, "GATE-G8 decision marker absent"
t = t.replace(old, "**PENDING** — reviewed artifact identity", 1)
anchor = "\n## "
i = t.index("\n## ", t.index("## GATE DECISIONS") + 5)
row = ("| 2026-11-04T00:00:00Z | GATE-G8 | timestamp correction | — | — | "
       "Correction (fixture): an earlier PASSED phrasing was restated — "
       "annotation only, not a decision. | correction |\n")
open(p, "w").write(t[:i] + row + t[i:])
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
