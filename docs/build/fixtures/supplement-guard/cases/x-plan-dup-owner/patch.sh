#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, re
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-decomposition.md"
t = open(p).read()
m = re.search(r'^\| 255 \| `[^`]+` \| R6\.3 \|[^\n]*$', t, re.M)
assert m, "R6.3 row"
line = m.group(0); cells = line.split("|")
assert "XA-" in cells[8], cells[8]
cells[8] = cells[8].rstrip() + ", XA-101 "
t = t.replace(line, "|".join(cells), 1)
open(p, "w").write(t)
PY
