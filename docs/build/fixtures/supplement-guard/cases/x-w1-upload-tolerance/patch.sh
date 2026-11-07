#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/tickets/00_MANIFEST.md"
t = open(p).read()
anchor = "| 252 | `252_GATE-G20__bootstrap.md`"
assert anchor in t
t = t.replace(anchor, "<!-- 2026-11-08 note: W1 ARCHIVE-UPLOAD is R6.40's first step (GATE-G20 D-1) -->\n" + anchor, 1)
open(p, "w").write(t)
PY
