#!/usr/bin/env bash
# run-fixtures.sh — negative-fixture battery for hh-p1-records-check/1 (R6.2,
# REQ-XA-106 / AC-2.2-6).
#
# Each cases/<name>/patch.sh mutates a scratch copy of the verifier's real
# committed inputs; cases/<name>/expected.txt names the check id the verifier
# MUST fail (exit 1) or `missing-input` (exit 2). The verifier reads existing
# records and never writes build state: mutations happen only under mktemp
# sandboxes.

set -euo pipefail
cd "$(dirname "$0")/../../../.."

TOOL=docs/build/tools/p1_records.py
FIXDIR=docs/build/fixtures/p1-records
PROV=docs/build/reports/provenance/2026-11-06-scratch-drivers
PACKETS="2026-11-05-s4-confirmation.json 2026-11-05-s4-egress-cohort.json \
2026-11-06-s4-completion.json 2026-11-06-s4-30slot-repairs.json \
2026-11-05-parity-tranche-3.json"

fail=0; pass=0
for case_dir in "$FIXDIR"/cases/*; do
  name=$(basename "$case_dir")
  read -r expected _ <<<"$(head -1 "$case_dir/expected.txt")"
  root=$(mktemp -d "${TMPDIR:-/tmp}/p1rec-${name}.XXXXXX")
  mkdir -p "$root/docs/build/reports" "$root/docs/build/tools" \
         "$root/docs/build/reports/provenance" "$root/crates/hh-env/tests/fixtures"
  cp "$TOOL" "$root/docs/build/tools/"
  cp -R "$PROV" "$root/docs/build/reports/provenance/"
  for p in $PACKETS; do cp "docs/build/reports/$p" "$root/docs/build/reports/"; done
  for p in docs/build/reports/*-p1-surface-xcheck.json docs/build/reports/*-p1-estimators.json; do
    [ -f "$p" ] && cp "$p" "$root/docs/build/reports/" || true
  done
  [ -d crates/hh-env/tests/fixtures/r5_49 ] && cp -R crates/hh-env/tests/fixtures/r5_49 "$root/crates/hh-env/tests/fixtures/"
  [ -d crates/hh-env/tests/fixtures/p1 ] && cp -R crates/hh-env/tests/fixtures/p1 "$root/crates/hh-env/tests/fixtures/"
  bash "$case_dir/patch.sh" "$root"
  if [ "$expected" = "missing-input" ]; then
    out=$(cd "$root" && python3 -I docs/build/tools/p1_records.py --check 2>&1) && rc=0 || rc=$?
  else
    # each mutation case targets one check — isolate it so unrelated
    # pending stage-2 inputs cannot mask the mutation
    out=$(cd "$root" && python3 -I docs/build/tools/p1_records.py --check --only "$expected" 2>&1) && rc=0 || rc=$?
  fi
  if [ "$expected" = "missing-input" ]; then
    if [ "$rc" -eq 2 ]; then
      echo "  ✓ $name — exit 2 (missing input)"; pass=$((pass + 1))
    else
      echo "  ✗ $name — expected exit 2, got rc=$rc" >&2
      printf '%s\n' "$out" | tail -12 >&2; fail=$((fail + 1))
    fi
  elif [ "$rc" -eq 1 ] && printf '%s\n' "$out" | python3 -c \
      "import json,sys; r=json.load(sys.stdin); sys.exit(0 if any(c['check']=='$expected' and c['verdict']=='fail' for c in r['checks']) else 1)"; then
    echo "  ✓ $name — rejected with $expected fail"; pass=$((pass + 1))
  else
    echo "  ✗ $name — expected exit 1 + $expected fail, got rc=$rc" >&2
    printf '%s\n' "$out" | tail -12 >&2; fail=$((fail + 1))
  fi
  rm -rf "$root"
done
echo "run-fixtures: $pass accepted/rejected as expected, $fail failures"
[ "$fail" -eq 0 ]
