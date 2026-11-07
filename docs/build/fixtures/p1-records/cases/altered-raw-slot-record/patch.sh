#!/usr/bin/env bash
set -euo pipefail
RAW="$1/docs/build/reports/provenance/2026-11-06-scratch-drivers/raw"
victim="$RAW/s4-confirm/slots/$(ls "$RAW/s4-confirm/slots" | head -1)"
python3 - "$victim" <<'PY'
import json, sys
p = sys.argv[1]
d = json.load(open(p))
d["calls"] = d.get("calls", 0) + 99
json.dump(d, open(p, "w"), indent=1)
PY
(cd "$RAW" && shasum -a 256 "s4-confirm/slots/$(basename "$victim")" > /tmp/p1newsum.$$)
newsum=$(cut -d' ' -f1 /tmp/p1newsum.$$); rm -f /tmp/p1newsum.$$
python3 - "$RAW/SHA256SUMS" "s4-confirm/slots/$(basename "$victim")" "$newsum" <<'PY'
import sys
path, rel, h = sys.argv[1:4]
lines = [l if rel not in l else l.split()[0] + "  " + rel for l in open(path)]
lines = [f"{h}  {rel}" if rel in l else l for l in open(path)]
open(path, "w").write("".join(l if l.endswith("\n") else l + "\n" for l in lines))
PY
