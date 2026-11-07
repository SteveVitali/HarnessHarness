#!/usr/bin/env bash
set -euo pipefail
IDX="$1/docs/build/reports/provenance/2026-11-06-scratch-drivers/raw/index.json"
python3 - "$IDX" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["fold_expectations"]["slot_multiplicity"]["1"] = 159
json.dump(d, open(p, "w"), indent=1)
PY
