#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/tickets/285_HUMAN-H12__pages-enable.md"
t = open(p).read()
assert "**Owner:**" in t
t = t.replace("- **Owner:** the operator\n", "", 1)
open(p, "w").write(t)
PY
