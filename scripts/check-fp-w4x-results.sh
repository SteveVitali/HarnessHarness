#!/usr/bin/env bash
# check-fp-w4x-results.sh — recompute legs for the R6.57 executed-W4x
# re-fold record set (manifest 325f). Offline, $0; every leg fails
# loudly. The packet legs recompute byte-identically anywhere; the
# retained legs recompute where the retained W4x/W4xv harvests resolve
# (the manifest's ~/ paths) and otherwise report a recorded skip —
# never a pass.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
FAIL=0

leg() { # leg <name> <cmd...>
  local name="$1"; shift
  if "$@" >/tmp/fp-w4x-leg.$$.log 2>&1; then
    echo "fp-w4x-results: $name OK"
  else
    echo "fp-w4x-results: $name FAIL"
    cat /tmp/fp-w4x-leg.$$.log
    FAIL=1
  fi
}

records=(
  2026-11-20-fp-w4x-outcomes
  2026-11-20-fp-w4x-answer-scan
  2026-11-20-fp-w4x-flagged-review-input
  2026-11-20-fp-w4x-results-p2-fp
  2026-11-20-fp-w4x-results-p4-fp
  2026-11-20-fp-w4x-results-p3-l2
  2026-11-20-fp-w4x-l3
  2026-11-20-fp-w4x-admission-pi-executed
  2026-11-20-fp-w4x-admission-claude-code-executed
)

# 1 — every emitted record's sha256 sidecar verifies.
for r in "${records[@]}"; do
  leg "sha256:$r" shasum -a 256 -c "docs/build/reports/$r.sha256"
done

# 2 — append-only history: the W4 outcomes record, the frozen W4
#     packets and the R6.48 frozen packets are byte-untouched; their
#     committed sidecars still verify.
for r in \
    2026-11-19-fp-w4-outcomes \
    2026-11-19-fp-w4-answer-scan \
    2026-11-19-fp-w4-results-p2-fp \
    2026-11-19-fp-w4-results-p4-fp \
    2026-11-19-fp-w4-results-p2-fp-frozen \
    2026-11-19-fp-w4-results-p4-fp-frozen \
    2026-11-19-fp-w4-results-p3-l2-frozen \
    2026-11-19-fp-w4-l2-audit-frozen \
    2026-11-19-fp-w4-l3-frozen \
    2026-11-19-fp-results-p2-fp-frozen \
    2026-11-19-fp-results-p4-fp-frozen \
    2026-11-19-fp-results-p4-fp-frozen; do
  leg "history-sidecar:$r" shasum -a 256 -c "docs/build/reports/$r.sha256"
done
# the R6.55/R6.56-era records carry .json.sha256 sidecars.
for r in \
    2026-11-20-fp-w4x-window \
    2026-11-20-fp-w4xv-window; do
  leg "history-sidecar:$r" shasum -a 256 -c "docs/build/reports/$r.json.sha256"
done
# the a2 admission records embed a bare filename in their sidecar —
# verify them from inside the reports directory.
for r in \
    2026-11-20-fp-w4x-admission-pi \
    2026-11-20-fp-w4x-admission-claude-code; do
  leg "history-sidecar:$r" bash -c \
    "cd docs/build/reports && shasum -a 256 -c $r.json.sha256"
done
leg "truncation-refusal-retired" bash -c \
  "! grep -rl 'protected_pair_truncated{k<5,R<5}' \
    docs/build/reports/2026-11-20-fp-w4x-results-p2-fp.json \
    docs/build/reports/2026-11-20-fp-w4x-results-p4-fp.json \
    docs/build/reports/2026-11-20-fp-w4x-results-p3-l2.json \
    docs/build/reports/2026-11-20-fp-w4x-l3.json"
# the W4-era typed spellings stay inside typed-history members only —
# never as a landed arm disposition.
leg "no-ag3-landed-disposition" python3 -c \
  "import json; p=json.load(open(\"docs/build/reports/2026-11-20-fp-w4x-results-p2-fp.json\")); bad=[a for a,h in p[\"harnesses\"].items() if h.get(\"disposition\")==\"not_run{admission:AG3}\"]; assert not bad, bad"

# 3 — the r6_57 regression battery (validators, population fold,
#     withheld{flagged_review} freeze, packet shape, sidecars).
leg "r6_57-test-battery" cargo test -p hharness-runner --test r6_57

# 4 — the committed-inputs packets re-emit byte-identically.
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH" /tmp/fp-w4x-leg.$$.log' EXIT
mkdir -p "$SCRATCH/docs/build/reports" "$SCRATCH/scripts" "$SCRATCH/docs/adr"
for f in \
    scripts/fp-w4x-results-inputs.txt \
    docs/adr/ADR-0453-w4x-extension-unblock-rulings.md; do
  cp "$f" "$SCRATCH/$f"
done
for f in \
    2026-11-20-fp-w4x-window \
    2026-11-20-fp-w4xv-window \
    2026-11-19-fp-w4-window \
    2026-11-19-fp-w4-outcomes \
    2026-11-19-fp-w4-flagged-review \
    2026-11-19-fp-e \
    2026-11-10-fp-prereg \
    2026-11-20-fp-w4x-settlement \
    2026-11-20-fp-w4x-settlement-2 \
    2026-11-20-fp-w4xv-settlement \
    2026-11-20-fp-w4x-admission-pi \
    2026-11-20-fp-w4x-admission-claude-code \
    2026-11-20-fp-w4x-outcomes \
    2026-11-20-fp-w4x-answer-scan \
    2026-11-20-fp-w4x-flagged-review-input \
    2026-11-20-fp-w4x-results-p2-fp \
    2026-11-20-fp-w4x-results-p4-fp \
    2026-11-20-fp-w4x-results-p3-l2 \
    2026-11-20-fp-w4x-l3; do
  cp "docs/build/reports/$f.json" "$SCRATCH/docs/build/reports/$f.json"
done
mkdir -p "$SCRATCH/docs/build/reports/compute"
cp docs/build/reports/compute/2026-11-18-fp-ws1-l1-conformance.json \
   "$SCRATCH/docs/build/reports/compute/2026-11-18-fp-ws1-l1-conformance.json"
leg "emit-packets-reemit" cargo run -q -p hharness-runner --bin fp_w4x_results -- emit-packets "$SCRATCH"
for r in \
    2026-11-20-fp-w4x-results-p2-fp \
    2026-11-20-fp-w4x-results-p4-fp \
    2026-11-20-fp-w4x-results-p3-l2 \
    2026-11-20-fp-w4x-l3; do
  leg "byte-identical:$r" cmp "docs/build/reports/$r.json" "$SCRATCH/docs/build/reports/$r.json"
done

# 5 — the freeze state is the honest withheld{flagged_review}: the
#     freeze leg refuses loudly until HUMAN-H20's analysis record
#     lands; no *-frozen.json exists for the w4x set.
for r in \
    2026-11-20-fp-w4x-results-p2-fp \
    2026-11-20-fp-w4x-results-p4-fp \
    2026-11-20-fp-w4x-results-p3-l2 \
    2026-11-20-fp-w4x-l3; do
  leg "no-frozen-copy:$r" bash -c "test ! -e docs/build/reports/$r-frozen.json"
done
leg "freeze-withholds" bash -c \
  "! cargo run -q -p hharness-runner --bin fp_w4x_results -- freeze . 2>&1 | grep -q 'wrote .*-frozen'"

# 6 — retained-harvest legs recompute where the harvest resolves;
#     otherwise a recorded skip (never a pass).
if [ -d "$HOME/hh-retained/w4x/harvest" ] && [ -d "$HOME/hh-retained/w4xv/harvest" ]; then
  leg "ctrf-fold-pi" python3 - <<'PY'
import json, glob, os
root = os.path.expanduser("~/hh-retained/w4x/harvest/run-root-w4x/w4x-a4/slots")
exp = {"pi": 31, "claude-code": 30}
got = {}
for arm in exp:
    ok = 0
    for f in glob.glob(f"{root}/w4-{arm}-*/collect/ctrf.json"):
        s = json.load(open(f))["results"]["summary"]
        if s["failed"] == 0:
            ok += 1
    got[arm] = ok
assert got == exp, got
PY
else
  echo "fp-w4x-results: retained-harvest legs SKIPPED (~/hh-retained/w4x|w4xv absent) — recorded, not a pass"
fi

exit $FAIL
