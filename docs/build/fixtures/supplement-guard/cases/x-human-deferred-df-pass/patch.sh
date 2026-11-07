#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/readouts/HUMAN-H16.md"
open(p, "w").write("# HUMAN-H16 readout\nStatus: DEFERRED — see DF-H16-1\n")
p = f"{ROOT}/docs/tickets/DEFERRALS.md"
t = open(p).read()
t += "| DF-H16-1 (D) | HUMAN-H16 flagged-pass review withheld the freeze claim | test | test | test | test | OPEN |\n"
open(p, "w").write(t)
PY
