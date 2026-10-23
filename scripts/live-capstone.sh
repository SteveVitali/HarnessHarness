#!/usr/bin/env bash
# live-capstone.sh — the L4.6 live E2E capstone orchestrator (ADR-0363),
# extended for L4.6a (164w): the surface & plugin sweep.
#
# Drives every capstone leg through `scripts/live-verify.sh` — the ONLY
# path a live leg may take (R4.5a). Each leg is independently
# re-runnable: `live-verify.sh <leg> [--budget USD]` works standalone;
# this script just sequences them and prints the summary.
#
#   scripts/live-capstone.sh                    # the L4.6 set (declared budgets)
#   scripts/live-capstone.sh --sweep l4.6a      # the L4.6a six-leg sweep (USD 0)
#   scripts/live-capstone.sh --only a,b         # a subset (comma list)
#   scripts/live-capstone.sh --list             # the leg order + budgets
#   scripts/live-capstone.sh --harness <id>     # the record's harness field
#
# The L4.6 set totals USD 12 against L4.6's USD 30 allocation (the
# measured legs debit the usage-derived cost, always ≤ the declared
# bound; a leg whose estimate would breach the allocation or the $100
# cap refuses upstream — live-verify exit 3 — and asks the operator).
# The L4.6a sweep is USD 0 by contract — local lane, loopback and
# subprocess surfaces only; hosted-provider legs stay in the L4.6 set.
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

# The L4.6a surface & plugin sweep (164w) — every leg is USD 0 and
# independently runnable. Surface legs provision the local lane
# (HH_LOCAL_BASE_URL/HH_LOCAL_MODEL); the plugin legs need no names —
# they spawn the packaged variants out of process through hh-helper.
legs_l46a() {
  printf '%s\n' \
    "surface-acp 0" \
    "surface-mcp-lab 0" \
    "embed-sdk-live 0" \
    "plugin-memory-store 0" \
    "plugin-compact-evict-oldest 0" \
    "plugin-tracker-fixture 0"
}

only=""
sweep="l4.6"
harness="${HH_LIVE_HARNESS:-devin-desktop/swe-2/subagent}"
while [ $# -gt 0 ]; do
  case "$1" in
    --only)    only="${2:?--only needs a comma list}"; shift 2;;
    --sweep)   sweep="${2:?--sweep needs l4.6|l4.6a}"; shift 2;;
    --list)    legs; legs_l46a; exit 0;;
    --harness) harness="${2:?--harness needs an id}"; shift 2;;
    -h|--help) sed -n '2,20p' "$0"; exit 0;;
    *) printf 'live-capstone: unknown arg %s\n' "$1" >&2; exit 1;;
  esac
done
case "$sweep" in
  l4.6)  sweep_legs() { legs; };;
  l4.6a) sweep_legs() { legs_l46a; };;
  *) printf 'live-capstone: unknown sweep %s (l4.6|l4.6a)\n' "$sweep" >&2; exit 1;;
esac

selected() {
  if [ -z "$only" ]; then sweep_legs; return; fi
  local want
  for want in ${only//,/ }; do
    sweep_legs | grep "^$want " || { printf 'live-capstone: unknown leg %s for sweep %s\n' "$want" "$sweep" >&2; exit 1; }
  done
}

say() { printf 'live-capstone: %s\n' "$*"; }

pass=0; skip=0; fail=0; stop=0
summary=""
while read -r leg budget; do
  [ -z "$leg" ] && continue
  say "── $leg (declared budget \$$budget) ──"
  rc=0
  # `</dev/null` — a leg runner that reads stdin (the SDK `npm install`
  # probes, pty cells) must not eat the leg list this loop is reading.
  bash "$LV" "$leg" --budget "$budget" --harness "$harness" </dev/null || rc=$?
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

say "sweep=$sweep summary: pass=$pass skipped_or_failed=$skip stopped=$stop"
printf '%b\n' "$summary"
[ "$stop" -eq 0 ] && [ "$fail" -eq 0 ] && [ "$skip" -eq 0 ]
