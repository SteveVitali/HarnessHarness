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
