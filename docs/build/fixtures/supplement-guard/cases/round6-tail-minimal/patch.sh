#!/usr/bin/env bash
# Round 6 declares `tail=minimal`; demoting the docs row breaks the contiguous
# capstone, capstone, gate, docs tail → [tail-roles].
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p = f"{sys.argv[1]}/docs/tickets/00_MANIFEST.md"
t = open(p).read()
old = "| `325_DOC6.1__docs.md` | Round 6 | docs |"
assert old in t, "row 325 not in expected docs form"
open(p, "w").write(t.replace(old, "| `325_DOC6.1__docs.md` | Round 6 | ticket |", 1))
PY
