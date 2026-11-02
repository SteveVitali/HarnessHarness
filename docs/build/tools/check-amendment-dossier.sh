#!/usr/bin/env bash
# check-amendment-dossier.sh — structural invariants of the round-5 §9.3
# amendment register (docs/build/planning/2026-10-30_amendment-dossier.md).
#
# Asserts INVARIANTS of a well-formed register (BM-TEST-01), never the living
# status of any row: seven numbered rows exist; each row's status cell names
# a register-vocabulary token; every `prepared` row's packet section exists;
# the non-admission ledger and the unadmitted-name marker are present; no
# ADR-number allocation is claimed inside the dossier (proposals cite
# PADR-R5-<row> only).
#
# Usage: bash docs/build/tools/check-amendment-dossier.sh [path-to-register]
set -u
REG="${1:-docs/build/planning/2026-10-30_amendment-dossier.md}"
fails=0
fail() { echo "check-amendment-dossier: FAIL — $1"; fails=$((fails+1)); }

[ -f "$REG" ] || { echo "check-amendment-dossier: FAIL — register missing: $REG"; exit 1; }

# Seven numbered register rows (| 1 | ... | 7 |) in order.
rows=$(grep -cE '^\| [1-7] \| ' "$REG")
[ "$rows" -eq 7 ] || fail "expected 7 register rows, found $rows"
for n in 1 2 3 4 5 6 7; do
  grep -qE "^\| $n \| " "$REG" || fail "register row $n missing"
done

# Every row carries a status token from the register vocabulary.
for n in 1 2 3 4 5 6 7; do
  line=$(grep -E "^\| $n \| " "$REG")
  echo "$line" | grep -qE '\*\*(prepared|owed)\*\*' \
    || fail "row $n has no register-vocabulary status token"
done

# Each `prepared` row must name an existing packet section anchor.
for n in $(grep -oE '^\| [1-7] \| .*\*\*prepared\*\*' "$REG" | grep -oE '^\| [1-7]' | tr -d '| '); do
  case "$n" in
    1) grep -q '^## Packet A ' "$REG" || fail "row 1 prepared but Packet A section absent" ;;
    2) grep -q '^## Packet B ' "$REG" || fail "row 2 prepared but Packet B section absent" ;;
    *) : ;;  # sibling packets named by their own owners' sections
  esac
done

# Packet rows 1–2, when prepared, carry the contract field set.
if grep -q '^## Packet A ' "$REG"; then
  for field in 'Exact before' 'Owner map' 'Durable payload' 'Authority proof' 'Recovery' 'Tier / negotiation' 'Compatibility and preserved legacy' 'Dependency edges' 'Acceptance mapping' 'Alternatives rejected'; do
    grep -q "$field" "$REG" || fail "Packet A missing field: $field"
  done
fi
if grep -q '^## Packet B ' "$REG"; then
  for field in 'Exact before' 'Owner map' 'Durable payload' 'Authority' 'Recovery' 'Tier / negotiation' 'Compatibility and preserved legacy' 'Dependency edges' 'Acceptance mapping' 'Alternatives rejected'; do
    grep -q "$field" "$REG" || fail "Packet B missing field: $field"
  done
fi

# Non-admission ledger + unadmitted-name marker + no ADR-number claim.
# A proposal id (PADR-R5-<row>) may never be paired with a decided ADR
# number on the same line; citing decided research/build ADRs as sources is
# legitimate and untouched.
grep -q '^## Non-admission ledger' "$REG" || fail "non-admission ledger section absent"
grep -q '∗' "$REG" || fail "unadmitted-candidate marker absent"
grep -q 'PADR-R5-1' "$REG" || fail "PADR-R5-1 proposal id absent"
grep -q 'PADR-R5-2' "$REG" || fail "PADR-R5-2 proposal id absent"
grep -nE 'PADR-R5-[0-9].*ADR-0[0-9]{3}|ADR-0[0-9]{3}.*PADR-R5-[0-9]' "$REG" \
  && fail "a proposal id is paired with an ADR number (admission implied)"

# No agent signature block — the negation sentence stating the rule is fine;
# a real signature artifact (signed/approved/signature field) is not.
grep -qiE 'signature:|signed off|signed-off|signed by|^> approved|^approved by' "$REG" \
  && fail "agent acceptance/signature artifact present"

if [ "$fails" -gt 0 ]; then
  echo "check-amendment-dossier: $fails failure(s)"
  exit 1
fi
echo "check-amendment-dossier: OK — $REG"
