#!/usr/bin/env bash
# deferrals-open.sh — the owed-work index (BM-ORIENT-01; AGENTS.md gotcha 1).
#
#   deferrals-open.sh        one line per DF row whose status is not
#                            DONE/WONTFIX: 'id | lead token | last dated
#                            marker (<=200 chars) | BL home'
#   deferrals-open.sh --bl   one line per open BL row: 'id | status | title
#                            (<=120 chars)'
#
# Status detection follows gotcha 8: the status cell is token-led and
# marker-governed — a row is discharged iff its status cell carries a dated
# discharge marker ('**DONE** — YYYY-MM-DD', '**DONE — YYYY-MM-DD', bare
# 'DONE — YYYY-MM-DD'/'DONE — progress YYYY-MM-DD', or WONTFIX) or leads
# with a DONE/WONTFIX token. '**machine cells DONE**' inside a PARTIAL row
# is not a discharge marker. Cells are split on '|' OUTSIDE backtick code
# spans (the inline-'|' miss class from the G4-4 sweep). BL home = the last
# 'BL-nn' the row names. Pure bash/awk; no dependencies.

set -euo pipefail
cd "$(dirname "$0")/.."

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  sed -n '2,16p' "$0"; exit 0
fi

if [ "${1:-}" = "--bl" ]; then
  # Open BL rows from BACKLOG.csv — status is the last CSV field; the title
  # may be quoted and hold commas, so split fields CSV-aware.
  awk '
  function csvline(line, out,   i, n, c, cur, k, inq) {
    n = length(line); inq = 0; cur = ""; k = 0
    for (i = 1; i <= n; i++) {
      c = substr(line, i, 1)
      if (inq) {
        if (c == "\"") {
          if (substr(line, i + 1, 1) == "\"") { cur = cur "\""; i++; continue }
          inq = 0; continue
        }
        cur = cur c; continue
      }
      if (c == "\"" && cur == "") { inq = 1; continue }
      if (c == ",") { out[++k] = cur; cur = ""; continue }
      cur = cur c
    }
    out[++k] = cur
    return k
  }
  NR == 1 { next }
  {
    n = csvline($0, f)
    gsub(/^[ \t\r]+|[ \t\r]+$/, "", f[n])  # CRLF-tolerant (the CSV may carry \r\n)
    if (f[n] == "open") {
      line = f[1] " | " f[n] " | " f[2]
      printf "%s\n", substr(line, 1, 120)
    }
  }' docs/build/BACKLOG.csv
  exit 0
fi
if [ $# -gt 0 ]; then
  echo "usage: deferrals-open.sh [--bl]" >&2
  exit 2
fi

# The DF index — one awk pass over DEFERRALS.md rows.
awk '
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
function lead_token(cell,   t) {
  t = cell; sub(/[ \t].*$/, "", t); gsub(/\*/, "", t); return t
}
function last_marker(cell,   pos, dpos, pre, bpos, p) {
  pos = 0; dpos = 0
  while (match(substr(cell, pos + 1), /20[0-9][0-9]-[0-9][0-9]-[0-9][0-9]/)) {
    dpos = pos + RSTART; pos = dpos + 9
  }
  if (dpos == 0) return ""
  pre = substr(cell, 1, dpos - 1); bpos = 0; p = 0
  while (match(substr(pre, p + 1), /\*\*/)) { bpos = p + RSTART; p = bpos + 1 }
  return (bpos > 0) ? substr(cell, bpos) : substr(cell, dpos)
}
function discharged(cell, tok) {
  # dated discharge markers (gotcha 8)
  if (cell ~ /\*\*DONE\*\*[ —]+20[0-9][0-9]-/) return 1
  if (cell ~ /\*\*DONE — 20[0-9][0-9]-/) return 1
  if (cell ~ /DONE — (progress )?20[0-9][0-9]-/) return 1
  if (cell ~ /\*\*WONTFIX\*\*/) return 1
  # lead-token discharge (bare DONE — progress … / WONTFIX leads)
  if (tok == "DONE" || tok == "WONTFIX" || tok == "CLOSED") return 1
  return 0
}
/^\| DF-/ {
  np = split_md($0, cells)
  idcell = trim(cells[1]); sub(/[ \t].*$/, "", idcell)
  # The status column is the rightmost cell that LEADS with a canonical
  # status token — inline pipe fragments to its right are part of it
  # (the DF-S1.17-1 / DF-S1.25-1 inline-pipe class).
  si = 0
  for (i = np - 1; i >= 1; i--) {
    t = trim(cells[i]); sub(/[ \t].*$/, "", t); gsub(/\*/, "", t)
    if (t ~ /^(OPEN|PARTIAL|DONE|WONTFIX|ACCEPTED-SKELETON|CLOSED)$/) { si = i; break }
  }
  if (si == 0) next
  cell = cells[si]
  for (i = si + 1; i <= np - 1; i++) cell = cell "|" cells[i]
  cell = trim(cell)
  tok = lead_token(cell)
  if (discharged(cell, tok)) next
  # BL home: last BL-nn named anywhere in the row
  bl = "-"; rest = $0
  while (match(rest, /BL-[0-9]+/)) { bl = substr(rest, RSTART, RLENGTH); rest = substr(rest, RSTART + RLENGTH) }
  printf "%s | %s | %s | %s\n", idcell, tok, substr(last_marker(cell), 1, 200), bl
}' docs/tickets/DEFERRALS.md
