#!/usr/bin/env bash
# check-fp-w4-results.sh — recompute legs for the R6.53 executed-W4
# re-fold record set (manifest 325b). Offline, $0; every leg fails
# loudly. The packet legs recompute byte-identically anywhere; the
# retained legs recompute where the retained W4 harvest resolves (the
# manifest's ~/ path) and otherwise report a recorded skip — never a
# pass.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
FAIL=0

leg() { # leg <name> <cmd...>
  local name="$1"; shift
  if "$@" >/tmp/fp-w4-leg.$$.log 2>&1; then
    echo "fp-w4-results: $name OK"
  else
    echo "fp-w4-results: $name FAIL"
    cat /tmp/fp-w4-leg.$$.log
    FAIL=1
  fi
}

records=(
  2026-11-19-fp-w4-outcomes
  2026-11-19-fp-w4-answer-scan
  2026-11-19-fp-w4-flagged-review-input
  2026-11-19-fp-w4-results-p2-fp
  2026-11-19-fp-w4-results-p4-fp
  2026-11-19-fp-w4-results-p3-l2
  2026-11-19-fp-w4-l2-audit
  2026-11-19-fp-w4-l3
)

# 1 — every emitted record's sha256 sidecar verifies.
for r in "${records[@]}"; do
  leg "sha256:$r" shasum -a 256 -c "docs/build/reports/$r.sha256"
done

# 2 — the superseded R6.48 frozen packets are untouched (append-only —
#     the new packet set supersedes, never rewrites). Their committed
#     sidecars still verify.
for r in \
    2026-11-19-fp-results-p2-fp-frozen \
    2026-11-19-fp-results-p4-fp-frozen \
    2026-11-19-fp-results-p3-l2-frozen \
    2026-11-19-fp-l2-audit-frozen \
    2026-11-19-fp-l3-frozen; do
  leg "frozen-sidecar:$r" shasum -a 256 -c "docs/build/reports/$r.sha256"
done
leg "truncation-refusal-retired" bash -c \
  "! grep -rl 'protected_pair_truncated{k<5,R<5}' \
    docs/build/reports/2026-11-19-fp-w4-results-p2-fp.json \
    docs/build/reports/2026-11-19-fp-w4-results-p4-fp.json \
    docs/build/reports/2026-11-19-fp-w4-results-p3-l2.json \
    docs/build/reports/2026-11-19-fp-w4-l2-audit.json \
    docs/build/reports/2026-11-19-fp-w4-l3.json"

# 3 — the r6_53 regression battery (validators, population fold,
#     freeze veto, packet shape).
leg "r6_53-test-battery" cargo test -p hharness-runner --test r6_53

# 4 — the committed-inputs packets re-emit byte-identically.
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH" /tmp/fp-w4-leg.$$.log' EXIT
for f in \
    scripts/fp-w4-results-inputs.txt \
    docs/adr/ADR-0452-w4-scope-amendment.md \
    docs/build/reports/2026-11-19-fp-w4-window.json \
    docs/build/reports/2026-11-19-fp-e.json \
    docs/build/reports/compute/2026-11-18-fp-ws1-l1-conformance.json \
    docs/build/reports/2026-11-10-fp-prereg.json \
    docs/build/reports/2026-11-19-fp-w4-resurrect-window.json \
    docs/build/reports/2026-11-19-fp-w4-r652c-quote.json \
    docs/build/reports/2026-11-19-fp-w4-r652c-spend-bound.json \
    docs/build/reports/2026-11-19-fp-w4-settlement-2.json \
    docs/build/reports/2026-11-19-fp-w4-outcomes.json \
    docs/build/reports/2026-11-19-fp-w4-answer-scan.json \
    docs/build/reports/2026-11-19-fp-w4-flagged-review-input.json; do
  mkdir -p "$SCRATCH/$(dirname "$f")"
  cp "$REPO/$f" "$SCRATCH/$f"
done
leg "reemit-packets" bash -c \
  "cd '$REPO' && ./target/debug/fp_w4_results emit-packets '$SCRATCH'"
for r in \
    2026-11-19-fp-w4-results-p2-fp \
    2026-11-19-fp-w4-results-p4-fp \
    2026-11-19-fp-w4-results-p3-l2 \
    2026-11-19-fp-w4-l2-audit \
    2026-11-19-fp-w4-l3; do
  leg "byte-identical:$r" cmp -s \
    "$REPO/docs/build/reports/$r.json" \
    "$SCRATCH/docs/build/reports/$r.json"
done

# 5 — the freeze veto refuses while flagged passes stay unreviewed:
#     no *-frozen record may materialize and the refusal names the
#     pending review.
if "$REPO/target/debug/fp_w4_results" freeze "$SCRATCH" \
    >/tmp/fp-w4-freeze.$$.log 2>&1; then :; fi
if grep -q "refused — unreviewed flags" /tmp/fp-w4-freeze.$$.log \
    && [ -z "$(find "$SCRATCH" -name '*-frozen.json')" ]; then
  echo "fp-w4-results: freeze-veto OK (32 unreviewed flags → pending{human})"
else
  echo "fp-w4-results: freeze-veto FAIL"
  cat /tmp/fp-w4-freeze.$$.log
  FAIL=1
fi
rm -f /tmp/fp-w4-freeze.$$.log

# 6 — the retained legs recompute where the W4 harvest resolves.
if [ -d "$HOME/hh-retained/w4/harvest/run-root-w4" ]; then
  leg "reemit-full" bash -c \
    "cd '$REPO' && ./target/debug/fp_w4_results emit '$SCRATCH' >/dev/null 2>&1"
  for r in "${records[@]:0:3}"; do
    leg "byte-identical-retained:$r" cmp -s \
      "$REPO/docs/build/reports/$r.json" \
      "$SCRATCH/docs/build/reports/$r.json"
  done
else
  echo "fp-w4-results: retained legs SKIPPED — ~/hh-retained/w4/harvest absent" \
    "(the outcomes + scan + review-input records' recompute requires the" \
    "retained store; their committed bytes still verify via leg 1 and" \
    "the packet legs)"
fi

exit "$FAIL"
