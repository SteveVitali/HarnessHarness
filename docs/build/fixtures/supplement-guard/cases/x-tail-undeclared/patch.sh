#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/tickets/00_MANIFEST.md"
t = open(p).read()
t = t.replace("; tail=minimal)", ")", 1)
assert "tail=minimal" not in t.split("### Round 6")[1].split("\n")[0]
open(p, "w").write(t)
PY
