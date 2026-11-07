#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
p = f"{ROOT}/docs/build/LEDGER.md"
t = open(p).read()
rows = "\n".join([
 "| 2026-11-08 | GATE-G27 | prov w3 | disposition | **PASSED — disposition run; basis: quote met** | decision |",
 "| 2026-11-08 | GATE-G28 | prov wc | disposition | **PASSED — not triggered; Wc unused** | decision |",
 "| 2026-11-08 | GATE-G29 | g-l1 | disposition | **PASSED — disposition fallback; basis: trigger fired** | decision |",
 "| 2026-11-08 | GATE-G30 | fp-e | disposition | **PASSED — disposition run{hosted-only}; basis: fallback** | decision |"])
assert "## GATE DECISIONS" in t
t = t.replace("## GATE DECISIONS\n", "## GATE DECISIONS\n\n" + rows + "\n", 1)
open(p, "w").write(t)
PY
