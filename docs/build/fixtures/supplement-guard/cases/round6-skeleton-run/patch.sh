#!/usr/bin/env bash
# A Round-6 window skeleton must never carry a Run line → [skeleton].
set -euo pipefail
python3 - "$1" <<'PY'
import sys
p = f"{sys.argv[1]}/docs/tickets/290_R6.32__w1-run.md"
t = open(p).read()
anchor = "- **Depends on:**"
assert anchor in t, "skeleton header absent"
i = t.index("\n", t.index(anchor))
t = t[:i] + "\n- **Run:** `implement-spec spec=docs/tickets/290_R6.32__w1-run.md`" + t[i:]
open(p, "w").write(t)
PY
