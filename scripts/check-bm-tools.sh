#!/usr/bin/env bash
# check-bm-tools.sh — invariant self-test for the BM-ORIENT-01 row readers
# (R4.1; runs in CI next to check-drift.sh).
#
# Asserts invariants only (BM-TEST-01 — never a living count or row value):
#   1. every id `deferrals-open.sh` lists resolves through `bm-row.sh` to
#      exactly one DEFERRALS.md row;
#   2. a discharged (`**DONE** — <date>`) row is absent from the index —
#      the id is derived from the file at run time, never pinned;
#   3. `spec-section.sh R-2.8.4` output starts with its '### ' heading and
#      holds no second '### ' line;
#   4. unknown ids exit 2 (bm-row.sh, spec-section.sh);
#   5. `deferrals-open.sh --bl` lists only BL-* ids.

set -euo pipefail
cd "$(dirname "$0")/.."

fail() { echo "check-bm-tools: FAIL — $*" >&2; exit 1; }

# (1) every listed id resolves to exactly one row.
n=0
while IFS= read -r line; do
  [ -n "$line" ] || continue
  id=${line%% *}
  out=$(bash scripts/bm-row.sh "$id") || fail "bm-row.sh $id exited $?"
  [ "$(printf '%s\n' "$out" | grep -c .)" -eq 1 ] \
    || fail "bm-row.sh $id printed $(printf '%s\n' "$out" | grep -c .) lines, expected 1"
  case "$out" in
    "| ${id} "*) : ;;
    *) fail "bm-row.sh $id returned a row that does not open with '| $id '" ;;
  esac
  n=$((n + 1))
done < <(bash scripts/deferrals-open.sh)
if [ "$n" -gt 0 ]; then
  echo "check-bm-tools: $n open deferral(s), each resolving to exactly one row"
else
  # Zero open rows is a legal index — GATE-G6's zero-deferral threshold,
  # first reached at R4.16a. In that state (2)'s resolved discharged id
  # below is the guard against a silently-broken reader.
  echo "check-bm-tools: 0 open deferrals — the zero-deferral index is legal (GATE-G6 threshold; first reached R4.16a)"
fi

# (2) a discharged row still resolves through bm-row.sh yet is absent from
# the index (id derived, not pinned) — also the zero-open reader guard.
done_id=$(grep -m1 '\*\*DONE\*\* — 20' docs/tickets/DEFERRALS.md \
          | grep -o 'DF-[A-Za-z0-9.-]*' | head -1 || true)
[ -n "$done_id" ] || fail "could not derive a discharged DF id from DEFERRALS.md"
out=$(bash scripts/bm-row.sh "$done_id") || fail "bm-row.sh $done_id exited $?"
[ "$(printf '%s\n' "$out" | grep -c .)" -eq 1 ] \
  || fail "bm-row.sh discharged $done_id did not resolve to exactly one row"
# grep -q on a pipe would early-close deferrals-open.sh (SIGPIPE under
# pipefail); read the index into a string first.
open_index=$(bash scripts/deferrals-open.sh)
if grep -qF "$done_id" <<<"$open_index"; then
  fail "$done_id is discharged yet listed by deferrals-open.sh"
fi
echo "check-bm-tools: discharged $done_id is absent from the open index"

# (3) spec-section.sh R-2.8.4 — one section, heading first.
out=$(bash scripts/spec-section.sh R-2.8.4)
first=${out%%$'\n'*}
case "$first" in
  "### "*) : ;;
  *) fail "spec-section.sh R-2.8.4 does not start with a ### heading" ;;
esac
case "$first" in
  *R-2.8.4*) : ;;
  *) fail "spec-section.sh R-2.8.4 heading does not name R-2.8.4" ;;
esac
[ "$(grep -c '^### ' <<<"$out")" -eq 1 ] \
  || fail "spec-section.sh R-2.8.4 printed more than one ### heading"
echo "check-bm-tools: spec-section.sh R-2.8.4 prints exactly one section"

# (4) unknown ids exit 2.
st=0; bash scripts/bm-row.sh DF-NOPE-000 >/dev/null 2>&1 || st=$?
[ "$st" -eq 2 ] || fail "bm-row.sh unknown id exited $st, expected 2"
st=0; bash scripts/spec-section.sh R-9.9.9 >/dev/null 2>&1 || st=$?
[ "$st" -eq 2 ] || fail "spec-section.sh unknown R-id exited $st, expected 2"
echo "check-bm-tools: unknown ids exit 2"

# (5) --bl lines carry only BL-* ids.
while IFS= read -r line; do
  [ -n "$line" ] || continue
  case "$line" in BL-*) : ;; *) fail "--bl line does not open with a BL id: ${line:0:60}" ;; esac
done < <(bash scripts/deferrals-open.sh --bl)
echo "check-bm-tools: --bl lists BL ids only"

echo "check-bm-tools: OK"
