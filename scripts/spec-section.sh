#!/usr/bin/env bash
# spec-section.sh — print ONE spec section or ONE AC bullet (BM-ORIENT-01).
#
#   spec-section.sh R-2.8.4        the '### <sec> R-2.8.4 — …' heading through
#                                  the line before the next '### ' — never more
#   spec-section.sh AC-R-2.8.4-3   the one '- **AC-R-2.8.4-3 (…).**' bullet
#                                  (plus any indented continuation lines)
#   spec-section.sh '§5g.4'        the '### 05g.4 …' section (leading § and
#                                  leading-0 padding both accepted)
#
# spec/CANONICAL_SPEC.md is ~634k tokens — a contract names the section, this
# prints just it. Unknown anchor exits 2 with a message. Pure bash/awk.

set -euo pipefail
cd "$(dirname "$0")/.."

SPEC=spec/CANONICAL_SPEC.md

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  sed -n '2,14p' "$0"; exit 0
fi
if [ $# -ne 1 ]; then
  echo "usage: spec-section.sh <R-id|AC-id|\"§heading\">" >&2
  exit 2
fi
id=$1

case "$id" in
  AC-R-*)
    # One bullet: '- **AC-R-2.8.4-1 (…' — the id as a whole token (the char
    # after it is never [0-9], so AC-…-1 never matches AC-…-10), then any
    # indented continuation lines, stopping at the next bullet/heading.
    awk -v id="$id" '
      index($0, "- **" id) == 1 || index($0, "- **" id) == 2 {
        rest = substr($0, index($0, id) + length(id))
        if (rest ~ /^[0-9]/) next
        print; found = 1; printing = 1; next
      }
      printing == 1 {
        if ($0 ~ /^[ \t]/ && $0 !~ /^- /) { print; next }  # continuation
        exit
      }
      END { if (!found) exit 1 }' "$SPEC" || {
      echo "spec-section: no AC bullet for '$id' in $SPEC" >&2; exit 2; }
    ;;
  §*)
    sec=${id#§}
    # '### <sec> …' or '### 0<sec> …' (requirements pad to two digits: 05g.4)
    start=$(grep -nE "^### (0?${sec})[ .]" "$SPEC" | head -1 | cut -d: -f1 || true)
    if [ -z "$start" ]; then
      echo "spec-section: no '### ${sec} …' heading in $SPEC" >&2; exit 2
    fi
    awk -v s="$start" 'NR >= s { if (NR > s && /^### /) exit; print }' "$SPEC"
    ;;
  R-*)
    # '### <sec> R-2.x.y — …' — the R-id as a whole token in a ### heading.
    start=$(awk -v id="$id" '
      /^### / && index($0, " " id " ") > 0 { print NR; exit }
      /^### / && index($0, " " id) > 0 {
        rest = substr($0, index($0, " " id) + length(id) + 1)
        if (rest !~ /^[0-9a-z]/) { print NR; exit }
      }' "$SPEC")
    if [ -z "$start" ]; then
      echo "spec-section: no '### … $id' heading in $SPEC" >&2; exit 2
    fi
    awk -v s="$start" 'NR >= s { if (NR > s && /^### /) exit; print }' "$SPEC"
    ;;
  *)
    echo "spec-section: unknown anchor '$id' (R-*|AC-R-*|§* expected)" >&2
    exit 2 ;;
esac
