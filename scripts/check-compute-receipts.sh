#!/usr/bin/env bash
# check-compute-receipts.sh — validate every committed hh-compute-receipt/1
# under docs/build/reports/compute/ and every hh-compute-decl/1 under
# scripts/gce-backend/declarations/ (manifest 266/R6.13; REQ-XB-208/202).
# Runs on a `ci-mode: both` step — a docs-only head that drifts a receipt
# must not take the fast path green.
set -euo pipefail
cd "$(dirname "$0")/.."

B="$PWD/scripts/gce-backend"
n=0
fail=0

for f in docs/build/reports/compute/*.json; do
  [ -f "$f" ] || continue
  if python3 -I "$B/validate_receipt.py" "$f"; then
    n=$((n + 1))
  else
    fail=1
  fi
done
echo "check-compute-receipts: $n receipt(s) valid"

m=0
for f in scripts/gce-backend/declarations/*.json; do
  [ -f "$f" ] || continue
  if python3 -I "$B/validate_decl.py" "$f"; then
    m=$((m + 1))
  else
    fail=1
  fi
done
echo "check-compute-receipts: $m declaration(s) valid"

[ "$fail" -eq 0 ] || { echo "check-compute-receipts: FAIL" >&2; exit 1; }
echo "check-compute-receipts: OK"
