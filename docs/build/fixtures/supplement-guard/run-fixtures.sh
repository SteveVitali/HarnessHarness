#!/usr/bin/env bash
# run-fixtures.sh — negative-fixture battery for hh-supplement-guard/1 (R5.1).
#
# Each cases/<name>/patch.sh mutates a scratch copy of the guard's real inputs;
# cases/<name>/expected.txt names the fatal diagnostic code the guard MUST emit
# (and exit 1). The guard reads existing records and never writes build state:
# mutations happen only under mktemp sandboxes. The stock checker input is the
# committed stand-in checker-green.json — a fixture, not a live validation run.

set -euo pipefail
cd "$(dirname "$0")/../../../.."

GUARD=docs/build/tools/supplement_guard.py
FIXDIR=docs/build/fixtures/supplement-guard
GREEN="$FIXDIR/checker-green.json"

fail=0; pass=0
for case_dir in "$FIXDIR"/cases/*; do
  name=$(basename "$case_dir")
  expected=$(head -1 "$case_dir/expected.txt" | tr -d '[:space:]')
  root=$(mktemp -d "${TMPDIR:-/tmp}/supguard-${name}.XXXXXX")
  mkdir -p "$root/docs/build/planning" "$root/docs/build/readouts" "$root/docs/tickets"
  cp docs/evidence-and-adoption-spec.md "$root/docs/"
  cp docs/tickets/*.md "$root/docs/tickets/"
  cp docs/build/LEDGER.md docs/build/BUILD_INDEX.md "$root/docs/build/"
  cp docs/build/planning/2026-10-30_evidence-coverage.md \
     docs/build/planning/2026-10-30_evidence-coverage.json \
     docs/build/planning/2026-10-30_evidence-legacy-baseline.json \
     "$root/docs/build/planning/"
  cp docs/build/readouts/*.md "$root/docs/build/readouts/"
  checker="$GREEN"
  [ -f "$case_dir/checker.json" ] && checker="$case_dir/checker.json"
  bash "$case_dir/patch.sh" "$root"
  out=$(python3 "$GUARD" --root "$root" --checker-json "$checker" 2>&1) && rc=0 || rc=$?
  # grep -F: the bracketed code is a literal, never a regex character class
  # (R5.2 repair — "\[$expected\]" matched any listed char, so a wrong
  # diagnostic could satisfy a case).
  if [ "$rc" -eq 1 ] && printf '%s\n' "$out" | grep -qF "[$expected]"; then
    echo "  ✓ $name — rejected with [$expected]"
    pass=$((pass + 1))
  else
    echo "  ✗ $name — expected exit 1 + [$expected], got rc=$rc" >&2
    printf '%s\n' "$out" | tail -15 >&2
    fail=$((fail + 1))
  fi
  rm -rf "$root"
done
echo "run-fixtures: $pass rejected, $fail failures"
[ "$fail" -eq 0 ]
