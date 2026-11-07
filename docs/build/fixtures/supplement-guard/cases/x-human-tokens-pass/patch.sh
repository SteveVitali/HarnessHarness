#!/usr/bin/env bash
set -euo pipefail
python3 - "$1" <<'PY'
import sys, json, re, os
ROOT = sys.argv[1]
for rid, st in (("HUMAN-H15", "NOT_NEEDED"), ("HUMAN-H17", "N/A"), ("HUMAN-H14", "DECLINED")):
    open(f"{ROOT}/docs/build/readouts/{rid}.md", "w").write(f"# {rid} readout\nStatus: {st}\n")
PY
