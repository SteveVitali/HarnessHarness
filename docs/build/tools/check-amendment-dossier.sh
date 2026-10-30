#!/usr/bin/env bash
# check-amendment-dossier.sh — structural invariants of the round-5 §9.3
# amendment register (docs/build/planning/2026-10-30_amendment-dossier.md).
#
# Asserts INVARIANTS of a well-formed register (BM-TEST-01), never the living
# status of any row: seven numbered rows exist; each row's status cell names
# a register-vocabulary token; every `prepared` row's packet section exists;
# rows 4–7 additionally require the shared method contract's AC-1 markers and
# each packet's own field set + packet marker; the non-admission ledger and
# the unadmitted-name marker are present; no ADR-number allocation is claimed
# inside the dossier (proposals cite PADR-R5-<row> only).
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
    3) grep -q '^## Packet C ' "$REG" || fail "row 3 prepared but Packet C section absent" ;;
    4) grep -q '^## Packet D ' "$REG" || fail "row 4 prepared but Packet D section absent" ;;
    5) grep -q '^## Packet E ' "$REG" || fail "row 5 prepared but Packet E section absent" ;;
    6) grep -q '^## Packet F ' "$REG" || fail "row 6 prepared but Packet F section absent" ;;
    7) grep -q '^## Packet G ' "$REG" || fail "row 7 prepared but Packet G section absent" ;;
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
# Packet row 3, when prepared, carries the same contract field set and cites
# its proposal id (PADR-R5-3, never an ADR number — the pairing rule below).
# Fields are checked inside the Packet C section — file-wide greps would be
# masked by the sibling packets' same-named fields.
if grep -q '^## Packet C ' "$REG"; then
  grep -q 'PADR-R5-3' "$REG" || fail "PADR-R5-3 proposal id absent"
  pkt_c=$(awk '/^## Packet C /{f=1} f&&/^## /&&!/^## Packet C /{f=0} f' "$REG")
  echo "$pkt_c" | grep -q 'evidence-package/1' || fail "Packet C missing profile name evidence-package/1"
  for field in 'Exact before' 'Owner map' 'Durable payload' 'Authority' 'Recovery' 'Tier / negotiation' 'Compatibility and preserved legacy' 'Dependency edges' 'Acceptance mapping' 'Alternatives rejected'; do
    echo "$pkt_c" | grep -q "$field" || fail "Packet C missing field: $field"
  done
fi

# Rows 4–7 (R5.5's method/detector slice), when prepared: the shared method
# contract carries the R5.5 AC-1 cross-row items and each packet carries the
# same contract field set, its proposal id and a packet-specific marker —
# all checked inside the packet's own section.
if grep -qE '^\| [4-7] \| .*\*\*prepared\*\*' "$REG"; then
  shared=$(awk '/^## Shared method contract/{f=1} f&&/^## /&&!/^## Shared method contract/{f=0} f' "$REG")
  [ -n "$shared" ] || fail "rows 4-7 prepared but Shared method contract section absent"
  if [ -n "$shared" ]; then
    for marker in 'population' 'ppm' 'seed' 'doubling' 'S11' 'routing'; do
      echo "$shared" | grep -q "$marker" || fail "Shared method contract missing marker: $marker"
    done
  fi
fi
packet_section() {  # $1 = packet letter
  awk -v p="^## Packet $1 " '$0 ~ p {f=1} f && /^## / && $0 !~ p {f=0} f' "$REG"
}
if grep -q '^## Packet D ' "$REG"; then
  grep -q 'PADR-R5-4' "$REG" || fail "PADR-R5-4 proposal id absent"
  pkt_d=$(packet_section D)
  echo "$pkt_d" | grep -q 'Beta(1,1)' || fail "Packet D missing marker: Beta(1,1)"
  echo "$pkt_d" | grep -q 'finite-manifest\|finite_manifest' || fail "Packet D missing marker: finite-manifest"
  for field in 'Exact before' 'Owner map' 'Durable payload' 'Authority' 'Recovery' 'Tier / negotiation' 'Compatibility and preserved legacy' 'Dependency edges' 'Acceptance mapping' 'Alternatives rejected'; do
    echo "$pkt_d" | grep -q "$field" || fail "Packet D missing field: $field"
  done
fi
if grep -q '^## Packet E ' "$REG"; then
  grep -q 'PADR-R5-5' "$REG" || fail "PADR-R5-5 proposal id absent"
  pkt_e=$(packet_section E)
  echo "$pkt_e" | grep -q 'pair_id' || fail "Packet E missing marker: pair_id"
  echo "$pkt_e" | grep -q 'independent' || fail "Packet E missing marker: independent"
  for field in 'Exact before' 'Owner map' 'Durable payload' 'Authority' 'Recovery' 'Tier / negotiation' 'Compatibility and preserved legacy' 'Dependency edges' 'Acceptance mapping' 'Alternatives rejected'; do
    echo "$pkt_e" | grep -q "$field" || fail "Packet E missing field: $field"
  done
fi
if grep -q '^## Packet F ' "$REG"; then
  grep -q 'PADR-R5-6' "$REG" || fail "PADR-R5-6 proposal id absent"
  pkt_f=$(packet_section F)
  echo "$pkt_f" | grep -q 'KL' || fail "Packet F missing marker: KL"
  echo "$pkt_f" | grep -q 'TOST' || fail "Packet F missing marker: TOST"
  for field in 'Exact before' 'Owner map' 'Durable payload' 'Authority' 'Recovery' 'Tier / negotiation' 'Compatibility and preserved legacy' 'Dependency edges' 'Acceptance mapping' 'Alternatives rejected'; do
    echo "$pkt_f" | grep -q "$field" || fail "Packet F missing field: $field"
  done
fi
if grep -q '^## Packet G ' "$REG"; then
  grep -q 'PADR-R5-7' "$REG" || fail "PADR-R5-7 proposal id absent"
  pkt_g=$(packet_section G)
  echo "$pkt_g" | grep -q 'no_detector' || fail "Packet G missing marker: no_detector"
  echo "$pkt_g" | grep -q 'estimator_undefined' || fail "Packet G missing marker: estimator_undefined"
  for field in 'Exact before' 'Owner map' 'Durable payload' 'Authority' 'Recovery' 'Tier / negotiation' 'Compatibility and preserved legacy' 'Dependency edges' 'Acceptance mapping' 'Alternatives rejected'; do
    echo "$pkt_g" | grep -q "$field" || fail "Packet G missing field: $field"
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
