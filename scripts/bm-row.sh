#!/usr/bin/env bash
# bm-row.sh — print exactly ONE build-memory row per id (BM-ORIENT-01).
#
# The registers, DEFERRALS.md, BACKLOG.csv, COVERAGE_MATRIX.csv and the spec
# are far larger than a ticket worker's context budget. This is the row-scope
# reader: name the id, get the one row — never open the file.
#
#   bm-row.sh [--status] <ID>…
#
# Id classes and their homes (the interface every round-4 contract cites):
#   DF-*            docs/tickets/DEFERRALS.md           (the markdown row)
#   BL-*            docs/build/BACKLOG.csv              (header + the CSV line)
#   OQ-*            research/registers/open-questions.md
#   CF-*            research/registers/conflicts.md
#   SD-*, ADR-NNNN  research/registers/spec-debt.md      (spec-debt rows key on ADR ids)
#   RK-*            research/registers/risks.md
#   R-2.x.y[a-z]    docs/build/COVERAGE_MATRIX.csv       (header + the CSV line)
#
# --status prints only `<id> | <lead token> | <last dated marker>` per id
# (<=400 chars per line). --bl prints `<id> | <BL home>` — the last BL-nn
# the row names (a BL-* id answers itself; a row naming none answers `-`).
# Unknown id class or a class with no matching row exits 2 with a message.
# Pure bash/awk; no dependencies.

set -euo pipefail
cd "$(dirname "$0")/.."

STATUS=0 BL=0
while [ $# -gt 0 ]; do
  case "$1" in
    --status) STATUS=1; shift ;;
    --bl)     BL=1; shift ;;
    -h|--help) sed -n '2,21p' "$0"; exit 0 ;;
    *) break ;;
  esac
done
if [ $# -lt 1 ]; then
  echo "usage: bm-row.sh [--status] [--bl] <ID>…" >&2
  exit 2
fi

DEFERRALS=docs/tickets/DEFERRALS.md
BACKLOG=docs/build/BACKLOG.csv
OPENQ=research/registers/open-questions.md
CONFLICTS=research/registers/conflicts.md
SPECDEBT=research/registers/spec-debt.md
RISKS=research/registers/risks.md
COVERAGE=docs/build/COVERAGE_MATRIX.csv

# md_row <file> <id> — the one table row whose FIRST cell's first token is
# the id (first cell never holds a code-span pipe; a '(V)'/'(F)' marker may
# follow the id).
md_row() {
  awk -F'|' -v id="$2" '
    { t = $2; gsub(/^[ \t]+|[ \t]+$/, "", t); sub(/[ \t].*$/, "", t)
      if (t == id) { print; found = 1; exit } }
    END { if (!found) exit 1 }' "$1"
}

# csv_row <file> <id> — the one line whose first field is the id.
csv_row() {
  awk -F',' -v id="$2" '$1 == id { print; found = 1; exit } END { if (!found) exit 1 }' "$1"
}

# status_line <kind> <kind2> — on stdin a row; prints
# '<lead token> | <last dated marker>' (<=340 chars). Lead token = first
# word of the last markdown cell (md tables) or of the status/verdict CSV
# column, '*' markers stripped. The last dated marker runs from the last
# '**' boundary at or before the final YYYY-MM-DD to the end of the
# status-bearing cell.
status_line() {
  awk -v kind="$1" -v kind2="$2" '
  # Split a markdown table row on | outside backtick code spans; cells[i]
  # holds the text between pipe i-1 and pipe i (cells[0] is the leading
  # empty segment, cells[k] the trailing one).
  function split_md(line, cells,   i, n, incode, c, cur, k) {
    n = length(line); incode = 0; cur = ""; k = 0
    for (i = 1; i <= n; i++) {
      c = substr(line, i, 1)
      if (c == "`") { incode = !incode; cur = cur c; continue }
      if (c == "|" && !incode) { cells[k] = cur; k++; cur = ""; continue }
      cur = cur c
    }
    cells[k] = cur
    return k
  }
  function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
  {
    if (kind == "csv") {
      n = split($0, f, ",")
      cell = (kind2 == "cm") ? f[5] : f[n]   # CM verdict is col 5; BL status is last
    } else {
      np = split_md($0, cells)
      # The status column is the rightmost cell that LEADS with a
      # canonical status token — inline pipe fragments to its right are
      # part of it (the DF-S1.17-1 / DF-S1.25-1 inline-pipe class).
      si = 0
      for (i = np - 1; i >= 1; i--) {
        t = trim(cells[i]); sub(/[ \t].*$/, "", t); gsub(/\*/, "", t)
        if (t ~ /^(OPEN|PARTIAL|DONE|WONTFIX|ACCEPTED-SKELETON|CLOSED)$/) { si = i; break }
      }
      if (si == 0) si = np - 1
      cell = cells[si]
      for (i = si + 1; i <= np - 1; i++) cell = cell "|" cells[i]
    }
    cell = trim(cell)
    tok = cell; sub(/[ \t].*$/, "", tok); gsub(/\*/, "", tok)
    # last dated marker: last YYYY-MM-DD — in the status cell for
    # markdown rows, anywhere in the line for CSV rows (the verdict
    # cell itself carries no date).
    hay = (kind == "csv") ? $0 : cell
    pos = 0; dpos = 0
    while (match(substr(hay, pos + 1), /20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]/)) {
      dpos = pos + RSTART; pos = dpos + 9
    }
    marker = ""
    if (dpos > 0) {
      # back up to the last "**" boundary before the date, when one exists
      pre = substr(hay, 1, dpos - 1); bpos = 0; p = 0
      while (match(substr(pre, p + 1), /\*\*/)) { bpos = p + RSTART; p = bpos + 1 }
      marker = (bpos > 0) ? substr(hay, bpos) : substr(hay, dpos)
    }
    print substr(tok " | " marker, 1, 340)
  }'
}

emit() {
  local id=$1 file kind kind2="" line=""
  case "$id" in
    DF-*)   file=$DEFERRALS; kind=md ;;
    BL-*)   file=$BACKLOG;   kind=csv ;;
    OQ-*)   file=$OPENQ;     kind=md ;;
    CF-*)   file=$CONFLICTS; kind=md ;;
    RK-*)   file=$RISKS;     kind=md ;;
    SD-*|ADR-*)
            file=$SPECDEBT; kind=md
            # spec-debt rows key on the ADR id: '| ADR-0001 | …'. SD-* ids
            # may spell it 'SD-ADR-0001' or 'SD-0001' — try the literal id,
            # the stripped form, and the ADR- prefixed forms.
            for cand in "$id" "${id#SD-}" "ADR-${id#SD-}" "ADR-${id#SD-ADR-}"; do
              [ "$cand" = "$id" ] || [ -n "$cand" ] || continue
              if line=$(md_row "$file" "$cand"); then break; fi
            done
            ;;
    R-2.*)  file=$COVERAGE;  kind=csv; kind2=cm ;;
    *)      echo "bm-row: unknown id class '$id' (DF|BL|OQ|CF|RK|SD|ADR|R-2.* expected)" >&2
            exit 2 ;;
  esac
  if [ -z "$line" ]; then
    case "$kind" in
      csv) line=$(csv_row "$file" "$id" || true) ;;
      md)  line=$(md_row "$file" "$id" || true) ;;
    esac
  fi
  if [ -z "$line" ]; then
    echo "bm-row: no row for '$id' in $file" >&2
    exit 2
  fi
  if [ "$BL" -eq 1 ]; then
    # BL home: the last BL-nn the row names; a BL id answers itself.
    case "$id" in
      BL-*) printf '%s | %s\n' "$id" "$id" ;;
      *)    bl=$(printf '%s\n' "$line" \
               | awk '{ r="-"; while (match($0, /BL-[0-9]+/)) { r=substr($0,RSTART,RLENGTH); $0=substr($0,RSTART+RLENGTH) } print r }')
            printf '%s | %s\n' "$id" "$bl" ;;
    esac
  elif [ "$STATUS" -eq 1 ]; then
    printf '%s | ' "$id"
    printf '%s\n' "$line" | status_line "$kind" "$kind2" | cut -c1-360
  else
    case "$kind" in
      csv) head -1 "$file"; printf '%s\n' "$line" ;;
      *)   printf '%s\n' "$line" ;;
    esac
  fi
}

for id in "$@"; do emit "$id"; done
