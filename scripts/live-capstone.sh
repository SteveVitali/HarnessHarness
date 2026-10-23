#!/usr/bin/env bash
# live-capstone.sh — the L4.6 live E2E capstone orchestrator (ADR-0363).
#
# Drives every capstone leg through `scripts/live-verify.sh` — the ONLY
# path a live leg may take (R4.5a). Each leg is independently
# re-runnable: `live-verify.sh <leg> [--budget USD]` works standalone;
# this script just sequences them and prints the summary.
#
#   scripts/live-capstone.sh                 # all legs, declared budgets
#   scripts/live-capstone.sh --only a,b      # a subset (comma list)
#   scripts/live-capstone.sh --list          # the leg order + budgets
#   scripts/live-capstone.sh --harness <id>  # the record's harness field
#
# Declared budgets total USD 12 against L4.6's USD 30 allocation (the
# measured legs debit the usage-derived cost, always ≤ the declared
# bound; a leg whose estimate would breach the allocation or the $100
# cap refuses upstream — live-verify exit 3 — and asks the operator).
# Exit: 0 when every leg passed; 2 when any leg skipped/failed honestly —
# the summary names each outcome, never silently.

set -euo pipefail
set +x

REPO="$(cd "$(dirname "$0")/.." && pwd)"
LV="$REPO/scripts/live-verify.sh"

# Ordered legs + declared budgets (USD). The zero-cost legs run the
# provisioned local provider / loopback / sandbox surfaces only.
legs() {
  printf '%s\n' \
    "capstone-run-local 0" \
    "capstone-run-anthropic 2" \
    "capstone-run-openai 2" \
    "capstone-sandboxed-exec 0" \
    "capstone-judge 8" \
    "capstone-ledger-replay 0" \
    "capstone-web 0" \
    "capstone-mcp 0" \
    "capstone-fleet-webhook 0" \
    "capstone-bundle-repro 0"
}

only=""
harness="${HH_LIVE_HARNESS:-devin-desktop/swe-2/subagent}"
while [ $# -gt 0 ]; do
  case "$1" in
    --only)    only="${2:?--only needs a comma list}"; shift 2;;
    --list)    legs; exit 0;;
    --harness) harness="${2:?--harness needs an id}"; shift 2;;
    -h|--help) sed -n '2,20p' "$0"; exit 0;;
    *) printf 'live-capstone: unknown arg %s\n' "$1" >&2; exit 1;;
  esac
done

selected() {
  if [ -z "$only" ]; then legs; return; fi
  local want
  for want in ${only//,/ }; do
    legs | grep "^$want " || { printf 'live-capstone: unknown leg %s\n' "$want" >&2; exit 1; }
  done
}

say() { printf 'live-capstone: %s\n' "$*"; }

pass=0; skip=0; fail=0; stop=0
summary=""
while read -r leg budget; do
  [ -z "$leg" ] && continue
  say "── $leg (declared budget \$$budget) ──"
  rc=0
  bash "$LV" "$leg" --budget "$budget" --harness "$harness" || rc=$?
  case "$rc" in
    0) pass=$((pass+1)); summary="$summary\n  $leg: pass";;
    2) skip=$((skip+1)); summary="$summary\n  $leg: skipped/failed (record written honestly)";;
    3) stop=$((stop+1)); summary="$summary\n  $leg: SPEND CAP — STOP and ask the operator"; break;;
    4) stop=$((stop+1)); summary="$summary\n  $leg: NAME unprovided — STOP and ask (HUMAN-H6)"; break;;
    5) stop=$((stop+1)); summary="$summary\n  $leg: leak-scan hit — STOP and ask"; break;;
    6) stop=$((stop+1)); summary="$summary\n  $leg: gitleaks staged-diff finding — STOP and ask"; break;;
    7) stop=$((stop+1)); summary="$summary\n  $leg: required tool absent — STOP and ask"; break;;
    *) fail=$((fail+1)); summary="$summary\n  $leg: rc=$rc";;
  esac
done <<EOF
$(selected)
EOF

say "summary: pass=$pass skipped_or_failed=$skip stopped=$stop"
printf '%b\n' "$summary"
[ "$stop" -eq 0 ] && [ "$fail" -eq 0 ] && [ "$skip" -eq 0 ]
