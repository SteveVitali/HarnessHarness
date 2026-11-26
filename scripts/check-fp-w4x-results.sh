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
  2026-11-20-fp-w4x-flagged-review
  2026-11-20-fp-w4x-results-p2-fp
  2026-11-20-fp-w4x-results-p4-fp
  2026-11-20-fp-w4x-results-p3-l2
  2026-11-20-fp-w4x-l3
  2026-11-20-fp-w4x-results-p2-fp-post-review
  2026-11-20-fp-w4x-results-p4-fp-post-review
  2026-11-20-fp-w4x-results-p3-l2-post-review
  2026-11-20-fp-w4x-l3-post-review
  2026-11-20-fp-w4x-results-p2-fp-frozen
  2026-11-20-fp-w4x-results-p4-fp-frozen
  2026-11-20-fp-w4x-results-p3-l2-frozen
  2026-11-20-fp-w4x-l3-frozen
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

# 5 — the post-review mint: HUMAN-H20's committed dispositions landed,
#     so `freeze` re-mints the hh-flagged-review/1 record, the
#     `-post-review` restored packets (the committed pre-review packets
#     stay byte-untouched history) and the `-frozen` site-facing copies
#     wrapping the restored fold.
for r in \
    2026-11-20-fp-w4x-results-p2-fp \
    2026-11-20-fp-w4x-results-p4-fp \
    2026-11-20-fp-w4x-results-p3-l2 \
    2026-11-20-fp-w4x-l3; do
  leg "frozen-copy:$r" bash -c "test -e docs/build/reports/$r-frozen.json && test -e docs/build/reports/$r-frozen.sha256"
  leg "post-review-copy:$r" bash -c "test -e docs/build/reports/$r-post-review.json && test -e docs/build/reports/$r-post-review.sha256"
done
# the restored fold: every flag cleared → the flagged passes score as
# passes again (pi 31/50 → θ 620000, claude-code 30/50 → θ 600000,
# mini 29/50 → θ 580000 unchanged); the paired-cell Δ and the
# sign-flip/Holm p-members land as ints; no pending{flagged_review}
# marker survives anywhere in the restored or frozen records.
leg "restored-theta-p2" python3 -c \
  "import json; p=json.load(open('docs/build/reports/2026-11-20-fp-w4x-results-p2-fp-frozen.json')); h=p['harnesses']; assert h['pi']['theta']['ppm']==620000, h['pi']['theta']; assert h['claude-code']['theta']['ppm']==600000; assert h['mini-swe-agent']['theta']['ppm']==580000; assert h['pi']['delta_vs_ref']['ppm']==40000; assert h['claude-code']['delta_vs_ref']['ppm']==20000; assert isinstance(h['pi']['delta_vs_ref']['p_raw_ppm'],int); assert isinstance(h['pi']['delta_vs_ref']['p_holm_ppm'],int); assert isinstance(h['claude-code']['delta_vs_ref']['p_raw_ppm'],int); assert isinstance(h['claude-code']['delta_vs_ref']['p_holm_ppm'],int); assert all(m['status']=='computed' for m in p['family']['members']); assert h['pi']['leader_group']['leader_id']=='hh/pi@0.52.3' and h['pi']['leader_group']['member']==True; assert h['pi']['integrity']['cleared']==35 and h['claude-code']['integrity']['cleared']==34"
leg "restored-theta-p4" python3 -c \
  "import json; p=json.load(open('docs/build/reports/2026-11-20-fp-w4x-results-p4-fp-frozen.json')); h=p['harnesses']; assert h['v5']['theta']['ppm']==400000; assert h['v5']['leader_group']['leader_id']=='hh/v5@corrected-seal'; assert h['v5']['delta_vs_ref']['p_raw_ppm']['state']=='unavailable{contrast_refused}'"
leg "no-pending-markers" bash -c \
  "! grep -l 'pending{flagged_review}\|pending_flagged_review\|unavailable{pending_flagged_review}' \
    docs/build/reports/2026-11-20-fp-w4x-results-*-frozen.json \
    docs/build/reports/2026-11-20-fp-w4x-l3-frozen.json \
    docs/build/reports/2026-11-20-fp-w4x-*-post-review.json"
# the member-wise diff rule: each frozen copy adds exactly
# {freeze, review, seal_open_ref} over its post-review record.
for r in \
    2026-11-20-fp-w4x-results-p2-fp \
    2026-11-20-fp-w4x-results-p4-fp \
    2026-11-20-fp-w4x-results-p3-l2 \
    2026-11-20-fp-w4x-l3; do
  leg "freeze-diff:$r" python3 -c \
    "import json; a=json.load(open('docs/build/reports/$r-post-review.json')); b=json.load(open('docs/build/reports/$r-frozen.json')); added=set(b)-set(a); assert added=={'freeze','review','seal_open_ref'} or added=={'freeze','review'}, added; assert all(a[k]==b[k] for k in a), '$r members changed'; assert b['freeze']['recomputed_from']=='docs/build/reports/$r.json'; assert b['review']['cleared']==69 and b['review']['confirmed']==0"
done
# the review record itself: 69 of 69 cleared, the derived disposition
# cites HUMAN-H20's readout verbatim.
leg "review-record" python3 -c \
  "import json; r=json.load(open('docs/build/reports/2026-11-20-fp-w4x-flagged-review.json')); c=r['coverage']; assert c['flagged']==69 and c['cleared']==69 and c['confirmed']==0, c; assert r['disposition'].startswith('69 of 69 cleared'); assert all(x['disposition']=='cleared' for x in r['records']) and len(r['records'])==69"
# the mint is deterministic: a re-run re-emits byte-identical records.
leg "freeze-remint-identical" bash -c \
  "s=\$(cd docs/build/reports && shasum -a 256 2026-11-20-fp-w4x-*-frozen.json 2026-11-20-fp-w4x-*-post-review.json 2026-11-20-fp-w4x-flagged-review.json | shasum -a 256); \
   cargo run -q -p hharness-runner --bin fp_w4x_results -- freeze . >/dev/null 2>&1; \
   s2=\$(cd docs/build/reports && shasum -a 256 2026-11-20-fp-w4x-*-frozen.json 2026-11-20-fp-w4x-*-post-review.json 2026-11-20-fp-w4x-flagged-review.json | shasum -a 256); \
   [ \"\$s\" = \"\$s2\" ]"

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
