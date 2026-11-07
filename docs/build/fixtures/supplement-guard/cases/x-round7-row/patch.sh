#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/tickets/00_MANIFEST.md"
t = open(p).read()
m = re.search(r"^\| 325 \| `325_DOC6.1__docs.md` \|[^\n]*$", t, re.M)
assert m
t = t.replace(m.group(0), m.group(0) +
    "\n| 325a | `325a_R7.1__later-tranche.md` | Round 6 | ticket | later tranche | — |", 1)
open(p, "w").write(t)
open(f"{ROOT}/docs/tickets/325a_R7.1__later-tranche.md", "w").write(
    "# R7.1 later\n- **Depends on:** R6.1\n")
PY
