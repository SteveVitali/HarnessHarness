#!/usr/bin/env bash
# Record an active-round gate readout replaying GATE-G6's pre-round verbatim
# "accept, proceed" — old acceptance signatures cannot satisfy new scope.
set -euo pipefail
cat > "$1/docs/build/readouts/GATE-G9.md" <<'MD'
# Readout — GATE-G9
Status: PENDING
Operator decision (verbatim): "accept, proceed"
MD
