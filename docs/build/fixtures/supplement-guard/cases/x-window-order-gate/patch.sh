#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/tickets/00_MANIFEST.md"
t = open(p).read()
mg = re.search(r"^\| 305 \| `305_GATE-G27[^\n]*$", t, re.M)
m44 = re.search(r"^\| 306 \| `306_R6.44[^\n]*$", t, re.M)
assert mg and m44
t = t.replace(mg.group(0) + "\n", "")
t = t.replace(m44.group(0), m44.group(0) + "\n" + mg.group(0), 1)
open(p, "w").write(t)
PY
