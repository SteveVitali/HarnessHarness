#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, re
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/planning/2026-11-07_round6-coverage.md"
t = open(p).read()
m = re.search(r"## Owner index[^\n]*\n(.*?)(\n## |\Z)", t, re.S)
assert m, "owner index"
seg = m.group(1)
assert "XH-001" in seg
seg2 = seg.replace("XH-001", "XH-999", 1)
t = t.replace(seg, seg2, 1)
open(p, "w").write(t)
PY
