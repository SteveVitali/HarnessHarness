#!/usr/bin/env bash
# gen-revisit-triggers.sh — regenerate docs/adr/REVISIT_TRIGGERS.md (L4.2,
# ADR-0377): the ADR revisit-trigger catalogue a maintainer triages.
#
# Every docs/adr/ADR-*.md carries a mandatory `## Revisit trigger` section —
# a standing condition under which the decision re-opens. The classification
# authority is the BACKLOG.csv `sources` cell: an ADR homed on BL-47 is a
# quiet/dormant conditional, an ADR homed on BL-48 fired and was answered
# during the build, and an ADR homed on any other row is fired-unanswered —
# its residual rides that row (named in the table). A live ADR placed in no
# sources cell is a generation failure, never a silent drop (check-backlog
# enforces the same exactly-once rule).
#
#   gen-revisit-triggers.sh           regenerate docs/adr/REVISIT_TRIGGERS.md
#   gen-revisit-triggers.sh --check   exit 0 iff the committed file is in sync
#
# Deterministic: ADR ids sort numerically inside each table; trigger text is
# whitespace-folded verbatim; the output carries no timestamps — a re-run on
# unchanged sources is byte-identical.
# Self-test overrides (never set in ci.yml): GEN_TRIGGERS_ROOT (fixture root),
# GEN_TRIGGERS_OUT (output path; default <root>/docs/adr/REVISIT_TRIGGERS.md).
# Pure bash/awk, bash 3.2-compatible — no toolchain dependency, so the CI
# drift step can run on both step-modes (ADR-0362).
set -euo pipefail

MODE=emit
case "${1:-}" in
  --check) MODE=check ;;
  -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
  "") ;;
  *) echo "usage: gen-revisit-triggers.sh [--check]" >&2; exit 2 ;;
esac

ROOT="${GEN_TRIGGERS_ROOT:-$(git rev-parse --show-toplevel)}"
CSV="$ROOT/docs/build/BACKLOG.csv"
ADR_DIR="$ROOT/docs/adr"
OUT="${GEN_TRIGGERS_OUT:-$ADR_DIR/REVISIT_TRIGGERS.md}"

[ -f "$CSV" ] || { echo "gen-revisit-triggers: no backlog at $CSV" >&2; exit 2; }
[ -d "$ADR_DIR" ] || { echo "gen-revisit-triggers: no ADR dir at $ADR_DIR" >&2; exit 2; }

W="$(mktemp -d)"; trap 'rm -rf "$W" 2>/dev/null' EXIT

# ── BACKLOG.csv → adr-id  <TAB>  home-row  <TAB>  row-status ────────────────
# Same quote-aware cell reader check-backlog.sh uses; sources tokens split on
# space/semicolon/plus/comma exactly as it does.
awk '
function csv(line, F,   n, i, c, q, cell) {
  n = 0; cell = ""; q = 0
  for (i = 1; i <= length(line); i++) {
    c = substr(line, i, 1)
    if (q) { if (c == "\"") { if (substr(line, i + 1, 1) == "\"") { cell = cell "\""; i++ } else q = 0 } else cell = cell c }
    else if (c == "\"") q = 1
    else if (c == ",") { F[++n] = cell; cell = "" }
    else cell = cell c
  }
  F[++n] = cell; return n
}
function trim(x) { gsub(/^[ \t\r]+|[ \t\r]+$/, "", x); return x }
NR == 1 { n = csv($0, H); for (i = 1; i <= n; i++) { h = tolower(trim(H[i]));
    if (h == "bl_id") bc = i; if (h == "sources") sc = i; if (h == "status") tc = i }
  if (!bc) bc = 1; if (!sc) sc = 4; next }
/^#/ || /^[[:space:]]*$/ { next }
{ n = csv($0, F); b = trim(F[bc]); if (b == "") next
  st = tolower(trim(tc ? F[tc] : F[n])); src = F[sc]
  m = split(src, T, /[ ;+,]+/)
  for (i = 1; i <= m; i++) if (T[i] ~ /^ADR-[0-9]+$/) print T[i] "\t" b "\t" st
}' "$CSV" | sort > "$W/homes"

# ── each ADR: id, live flag (a superseded ADR's trigger is dead), trigger ───
: > "$W/adrs"
for f in "$ADR_DIR"/ADR-*.md; do
  [ -f "$f" ] || continue
  id="$(basename "$f" | grep -oE '^ADR-[0-9]+')"; [ -n "$id" ] || continue
  if grep -qiE '^[^#]*superseded by ADR-[0-9]+' "$f"; then live=dead; else live=live; fi
  # trigger text: from `## Revisit trigger` to the next `## ` heading,
  # whitespace-folded verbatim; `|` escaped for the table cell.
  trig="$(awk '/^## Revisit trigger/{p=1;next} p&&(/^## /||/^#/||/^---/){exit} p' "$f" \
          | tr '\n' ' ' | sed -E 's/[[:space:]]+/ /g; s/^ //; s/ $//; s/\|/\\|/g')"
  [ -n "$trig" ] || trig="(no \`## Revisit trigger\` section — contract violation)"
  printf '%s\t%s\t%s\n' "$id" "$live" "$trig" >> "$W/adrs"
done
sort -t- -k2,2n "$W/adrs" -o "$W/adrs"

home_of() { awk -F'\t' -v id="$1" '$1 == id {print $2 "\t" $3; exit}' "$W/homes"; }

# ── classify + emit ──────────────────────────────────────────────────────────
QUIET="$W/quiet"; FIRED="$W/fired"; UNANS="$W/unans"; DEAD="$W/dead"
: > "$QUIET"; : > "$FIRED"; : > "$UNANS"; : > "$DEAD"
unplaced=0
while IFS="$(printf '\t')" read -r id live trig; do
  [ -n "$id" ] || continue
  h="$(home_of "$id")"; row="${h%%	*}"
  if [ "$live" = dead ]; then
    printf '| %s | — (trigger dead — superseded) |\n' "$id" >> "$DEAD"
  elif [ "$row" = "BL-47" ]; then
    printf '| %s | %s |\n' "$id" "$trig" >> "$QUIET"
  elif [ "$row" = "BL-48" ]; then
    printf '| %s | %s |\n' "$id" "$trig" >> "$FIRED"
  elif [ -n "$row" ]; then
    printf '| %s | %s | %s |\n' "$id" "$row" "$trig" >> "$UNANS"
  else
    echo "gen-revisit-triggers: $id appears in no BACKLOG.csv sources cell — place its trigger before regenerating" >&2
    unplaced=$((unplaced + 1))
  fi
done < "$W/adrs"
[ "$unplaced" -eq 0 ] || { echo "gen-revisit-triggers: $unplaced unplaced live ADR(s)" >&2; exit 1; }

{
cat <<'HDR'
<!-- GENERATED FILE — do not hand-edit.
     Regenerate:  bash scripts/gen-revisit-triggers.sh
     Drift check: bash scripts/gen-revisit-triggers.sh --check   (CI, ci-mode: both)
     Source of truth: each ADR's `## Revisit trigger` section (verbatim below)
     + the BACKLOG.csv `sources` classification — BL-47 quiet-conditional,
     BL-48 fired-answered, any other row fired-unanswered (the home names the
     residual). Conversion to maintainer policy: the 2026-10-20 standing-rows
     ruling ("Convert to maintainer policy"); L4.2; ADR-0377.
     Triage procedure: MAINTAINING.md §"ADR revisit triggers". -->
# ADR revisit triggers — the standing catalogue

Every build ADR carries a `## Revisit trigger` — the concrete condition under
which its decision re-opens. This file is the generated catalogue of those
conditions, produced by `scripts/gen-revisit-triggers.sh` from the ADR texts
and the `docs/build/BACKLOG.csv` `sources` cells. The CSV cells remain the
classification authority; this file is the readable projection. Drift between
the two is a CI failure (`--check`), so a new or re-homed trigger is catalogued
by placing its id in the right `sources` cell and regenerating — never by
editing this file.

Reading the classes:

- **Quiet-conditional** — the named condition has not fired; the decision
  stands. Each re-fires on its own text.
- **Fired-and-answered** — the condition landed during the build and was
  discharged with dated evidence. The trigger stays on the register; it
  re-arms per its own text.
- **Fired-unanswered / row-bound** — the condition met and a residual is
  owed; the named backlog row owns it. Triaging these is reading that row.
- **Superseded** — the ADR is superseded; the trigger is dead (listed for
  completeness — the catalogue accounts for every ADR file).
HDR
nq=$(wc -l < "$QUIET" | tr -d ' ')
nf=$(wc -l < "$FIRED" | tr -d ' ')
nu=$(wc -l < "$UNANS" | tr -d ' ')
nd=$(wc -l < "$DEAD" | tr -d ' ')
printf '\n_%s ADRs: %s quiet-conditional · %s fired-and-answered · %s fired-unanswered · %s superseded._\n' \
  "$((nq + nf + nu + nd))" "$nq" "$nf" "$nu" "$nd"

printf '\n## Quiet-conditional triggers (BL-47)\n\n| ADR | Revisit trigger |\n|---|---|\n'
cat "$QUIET"
printf '\n## Fired-and-answered triggers (BL-48)\n\n| ADR | Revisit trigger |\n|---|---|\n'
cat "$FIRED"
printf '\n## Fired-unanswered / row-bound triggers\n\n| ADR | Home | Revisit trigger |\n|---|---|---|\n'
if [ -s "$UNANS" ]; then cat "$UNANS"; else printf '| — | — | (none) |\n'; fi
printf '\n## Superseded\n\n| ADR | Revisit trigger |\n|---|---|\n'
if [ -s "$DEAD" ]; then cat "$DEAD"; else printf '| — | (none) |\n'; fi
} > "$W/out"

if [ "$MODE" = check ]; then
  if [ -f "$OUT" ] && cmp -s "$W/out" "$OUT"; then
    echo "gen-revisit-triggers: in sync ($(basename "$OUT"))"
    exit 0
  fi
  echo "gen-revisit-triggers: $OUT is stale or missing — run scripts/gen-revisit-triggers.sh" >&2
  if [ -f "$OUT" ]; then diff "$W/out" "$OUT" | head -30 >&2 || true; fi
  exit 1
fi

cat "$W/out" > "$OUT"
echo "gen-revisit-triggers: wrote $OUT ($((nq + nf + nu + nd)) ADRs — $nq quiet · $nf fired-answered · $nu fired-unanswered · $nd superseded)"
