#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/tickets/00_MANIFEST.md"
t = open(p).read()
m18 = re.search(r"^\| 320 \| `320_HUMAN-H18[^\n]*$", t, re.M)
m44 = re.search(r"^\| 306 \| `306_R6.44[^\n]*$", t, re.M)
assert m18 and m44
t = t.replace(m18.group(0) + "\n", "")
t = t.replace(m44.group(0), m18.group(0) + "\n" + m44.group(0), 1)
open(p, "w").write(t)
PY
