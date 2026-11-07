#!/usr/bin/env bash
set -euo pipefail
P="$1/docs/build/reports/2026-11-05-s4-confirmation.json"
python3 - "$P" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["slots"][0]["work_exit"] = "1"
json.dump(d, open(p, "w"), indent=1)
PY
