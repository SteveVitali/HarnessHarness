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
c=0
fail=0

# Scope repair 2026-11-19 (DF-CAP6.1-5 / CAP6.2): the reports dir carries
# non-compute window records (ws1-era hh-l1-conformance/1, hh-l1-corpus/1, an
# empty-schema coverage record and a top-level-array refold-diffs record) —
# this checker owns hh-compute-* schemas only. Non-dict JSON and non-compute
# schemas are skipped with a note (they belong to other checkers); a record
# claiming an hh-compute-* schema of an unknown variant still fails.
skip=0
for f in docs/build/reports/compute/*.json; do
  [ -f "$f" ] || continue
  schema=$(python3 -c "
import json,sys
d=json.load(open(sys.argv[1]))
print(d.get('schema','') if isinstance(d,dict) else '')" "$f")
  case "$schema" in
    hh-compute-receipt/1)
      if python3 -I "$B/validate_receipt.py" "$f"; then
        n=$((n + 1))
      else
        fail=1
      fi
      ;;
    hh-compute-cost/1)
      td=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['receipts']['teardown']['path'])" "$f")
      if python3 "$B/cost_lib.py" costvalidate "$f" "$td" "$PWD"; then
        c=$((c + 1))
      else
        fail=1
      fi
      ;;
    hh-compute-*)
      echo "check-compute-receipts: $f unknown schema '$schema'" >&2
      fail=1
      ;;
    *)
      echo "check-compute-receipts: $f non-compute record (schema '${schema:-none}') — out of scope, skipped" >&2
      skip=$((skip + 1))
      ;;
  esac
done
echo "check-compute-receipts: $n receipt(s) + $c cost record(s) valid ($skip non-compute record(s) skipped)"

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
