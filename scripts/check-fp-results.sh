#!/usr/bin/env bash
# check-fp-results.sh — recompute legs for the R6.48 FP-RESULTS record
# set (manifest 314). Offline, $0; every leg fails loudly. The packet
# legs recompute byte-identically anywhere; the seal-open and scan legs
# recompute where the retained ws1 store resolves (the manifest's ~/
# paths) and otherwise report a recorded skip — never a pass.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
FAIL=0

leg() { # leg <name> <cmd...>
  local name="$1"; shift
  if "$@" >/tmp/fp-results-leg.$$.log 2>&1; then
    echo "fp-results: $name OK"
  else
    echo "fp-results: $name FAIL"
    cat /tmp/fp-results-leg.$$.log
    FAIL=1
  fi
}

records=(
  2026-11-19-fp-seal-open
  2026-11-19-fp-answer-scan
  2026-11-19-fp-results-p2-fp
  2026-11-19-fp-results-p4-fp
  2026-11-19-fp-results-p3-l2
  2026-11-19-fp-l2-audit
  2026-11-19-fp-l3
)

# 1 — every emitted record's sha256 sidecar verifies.
for r in "${records[@]}"; do
  leg "sha256:$r" shasum -a 256 -c "docs/build/reports/$r.sha256"
done

# 2 — the packet validators pass over the committed records.
leg "r6_48-test-battery" cargo test -p hharness-runner --test r6_48

# 3 — the committed-inputs packets re-emit byte-identically.
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH" /tmp/fp-results-leg.$$.log' EXIT
mkdir -p "$SCRATCH/docs/build/reports/compute" "$SCRATCH/scripts"
for f in scripts/fp-results-inputs.txt \
    docs/build/reports/2026-11-19-fp-ws1-trials.json \
    docs/build/reports/2026-11-19-fp-ws1-pp0-evidence.json \
    docs/build/reports/2026-11-19-fp-e.json \
    docs/build/reports/2026-11-19-fp-ws1-measurement.json \
    docs/build/reports/2026-11-19-fp-w4-quote.json \
    docs/build/reports/compute/2026-11-18-fp-ws1-l1-conformance.json \
    docs/build/reports/2026-11-10-fp-prereg.json; do
  mkdir -p "$SCRATCH/$(dirname "$f")"
  cp "$REPO/$f" "$SCRATCH/$f"
done
leg "reemit-packets" bash -c \
  "cd '$REPO' && ./target/debug/fp_results emit-packets '$SCRATCH'"
for r in \
    2026-11-19-fp-results-p2-fp \
    2026-11-19-fp-results-p4-fp \
    2026-11-19-fp-results-p3-l2 \
    2026-11-19-fp-l2-audit \
    2026-11-19-fp-l3; do
  leg "byte-identical:$r" cmp -s \
    "$REPO/docs/build/reports/$r.json" \
    "$SCRATCH/docs/build/reports/$r.json"
done

# 4 — the retained-store legs recompute where the store resolves.
#     (requires `cargo build --release -p hharness-runner --bin fp_results`
#     — the debug emit is correct but slow over the real corpus.)
if [ -d "$HOME/hh-retained/ws1/harvest" ]; then
  leg "reemit-full" bash -c \
    "cd '$REPO' && ./target/release/fp_results emit '$SCRATCH' >/dev/null 2>&1"
  for r in "${records[@]}"; do
    leg "byte-identical-retained:$r" cmp -s \
      "$REPO/docs/build/reports/$r.json" \
      "$SCRATCH/docs/build/reports/$r.json"
  done
else
  echo "fp-results: retained legs SKIPPED — ~/hh-retained/ws1/harvest absent" \
    "(the seal-open + scan records' recompute requires the retained store; \
     their committed bytes still verify via leg 1 and the test battery)"
fi

exit "$FAIL"
