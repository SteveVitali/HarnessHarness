#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/LEDGER.md"
t = open(p).read()
row = '| 2026-11-08 | GATE-G28 | prov wc | disposition | **PASSED — not triggered; basis: FAILED rung record** | decision |'
assert "## GATE DECISIONS" in t
t = t.replace("## GATE DECISIONS\n", "## GATE DECISIONS\n\n" + row + "\n", 1)
open(p, "w").write(t)
PY
