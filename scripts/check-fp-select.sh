#!/usr/bin/env bash
# check-fp-select.sh — the R6.30 SELECT-commit verification battery
# (manifest 284; AC-3…AC-6). Every committed record under
# docs/build/reports/ recomputes byte-identically from its declared
# inputs through the one `task-selection/1` materializer and the one
# gce-backend quote path — never a second engine. Offline; USD $0.
#
# Legs:
#   emit       the committed exclusion record → selection + commit
#              re-emitted into a scratch repo root and byte-diffed
#   decls      validate_decl.py × 5 windows
#   fpcheck    REQ-XH-118 set check passes; an export estimate above
#              W1's egress line refuses; a measured overrun absorbs
#              into Wc's D
#   quotes     receipt_lib.py quote per decl == committed quote docs
#   comparative fp_select comparative-quote == committed packet
#
# Exit 0 green; nonzero a leg failed.
set -euo pipefail
cd "$(dirname "$0")/.."
REPO="$(pwd)"

T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
fails=0
ok()  { echo "check-fp-select: ok [$1]${2:+ $2}"; }
bad() { echo "check-fp-select: FAIL [$1] $2" >&2; fails=$((fails + 1)); }

EXCL="docs/build/reports/2026-11-08-fp-exclusion-record.json"
SEL="docs/build/reports/2026-11-08-fp-selection.json"
COM="docs/build/reports/2026-11-08-fp-select-commit.json"
CQ="docs/build/reports/2026-11-08-fp-w1-comparative-quote.json"
DECLS=""
for w in w1 w2 w3 w4 wc; do DECLS="$DECLS scripts/gce-backend/declarations/fp-$w.json"; done

cargo build -p hharness-eval --bin fp_select >/dev/null 2>&1 \
    || { echo "check-fp-select: fp_select build failed" >&2; exit 2; }
BIN="$REPO/target/debug/fp_select"

# ── emit: committed records recompute byte-identically ───────────────
SCRATCH="$T/repo"; mkdir -p "$SCRATCH/docs/build/reports"
cp "$REPO/$EXCL" "$SCRATCH/$EXCL"
"$BIN" emit "$SCRATCH" >"$T/emit.log" 2>&1 \
    || { bad "emit" "fp_select emit failed"; sed -n '1,20p' "$T/emit.log" >&2; }
if [ -f "$SCRATCH/$SEL" ]; then
    diff -q "$REPO/$SEL" "$SCRATCH/$SEL" >/dev/null \
        && ok "emit" "task-selection/1 recomputes byte-identically" \
        || bad "emit" "task-selection/1 drifted"
else
    bad "emit" "no selection emitted"
fi
if [ -f "$SCRATCH/$COM" ]; then
    diff -q "$REPO/$COM" "$SCRATCH/$COM" >/dev/null \
        && ok "emit" "hh-fp-select/1 recomputes byte-identically" \
        || bad "emit" "hh-fp-select/1 drifted"
else
    bad "emit" "no select commit emitted"
fi

# ── decls: closed-schema validation on all five windows ──────────────
for d in $DECLS; do
    if python3 -I scripts/gce-backend/validate_decl.py "$d" >"$T/v.log" 2>&1; then
        ok "decl" "$(basename "$d") validates"
    else
        bad "decl" "$(basename "$d"): $(cat "$T/v.log")"
    fi
done

# ── fpcheck: the REQ-XH-118 set check ────────────────────────────────
if python3 -I scripts/gce-backend/cost_lib.py fpcheck "$REPO" $DECLS >"$T/f.log" 2>&1; then
    ok "fpcheck" 'Σ ceilings ≤ $20 with every term'
else
    bad "fpcheck" "$(cat "$T/f.log")"
fi
# the export-overrun refusal — the quote refuses above W1's egress line
python3 - "$REPO" >"$T/mk.py.log" 2>&1 <<'PY'
import json, sys
d = json.load(open(sys.argv[1] + "/scripts/gce-backend/declarations/fp-w1.json"))
d["quote_inputs"]["export_size_gib"] = 7
import os
os.makedirs(sys.argv[1] + "/target/tmp-fpcheck", exist_ok=True)
json.dump(d, open(sys.argv[1] + "/target/tmp-fpcheck/fp-w1-overexport.json", "w"))
PY
if python3 -I scripts/gce-backend/cost_lib.py fpcheck "$REPO" \
    "$REPO/target/tmp-fpcheck/fp-w1-overexport.json" \
    scripts/gce-backend/declarations/fp-w2.json \
    scripts/gce-backend/declarations/fp-w3.json \
    scripts/gce-backend/declarations/fp-w4.json \
    scripts/gce-backend/declarations/fp-wc.json >"$T/r.log" 2>&1; then
    bad "refusal" "export above the egress line did not refuse"
else
    grep -q "exceeds egress line" "$T/r.log" \
        && ok "refusal" "export above the egress line refuses typed" \
        || bad "refusal" "unexpected refusal: $(cat "$T/r.log")"
fi
rm -rf "$REPO/target/tmp-fpcheck"
# a measured overrun absorbs into Wc's D (REQ-XH-118)
if python3 -I scripts/gce-backend/cost_lib.py fpcheck "$REPO" \
    --export-measured-gib 7 $DECLS >"$T/o.log" 2>&1; then
    ok "overrun" "Wc absorbs the measured export overrun"
else
    bad "overrun" "$(cat "$T/o.log")"
fi

# ── quotes: the five committed quote docs recompute ──────────────────
for w in w1 w2 w3 w4 wc; do
    q="$REPO/docs/build/reports/2026-11-08-fp-$w-quote.json"
    python3 -I scripts/gce-backend/receipt_lib.py quote \
        "scripts/gce-backend/declarations/fp-$w.json" "$REPO" >"$T/q.json"
    if python3 -c "
import json,sys
a=json.load(open('$q')); b=json.load(open('$T/q.json'))
sys.exit(0 if a==b else 1)"; then
        ok "quote" "fp-$w quote recomputes byte-identically"
    else
        bad "quote" "fp-$w quote drifted"
    fi
done

# ── comparative: the committed comparative-quote/1 recomputes ────────
"$BIN" comparative-quote "$SCRATCH" $DECLS >"$T/cq.log" 2>&1 \
    || { bad "comparative" "emit failed"; sed -n '1,20p' "$T/cq.log" >&2; }
if [ -f "$SCRATCH/docs/build/reports/2026-11-08-fp-w1-comparative-quote.json" ]; then
    python3 -c "
import json,sys
a=json.load(open('$REPO/$CQ'))
b=json.load(open('$SCRATCH/docs/build/reports/2026-11-08-fp-w1-comparative-quote.json'))
sys.exit(0 if a==b else 1)" \
        && ok "comparative" "comparative-quote/1 recomputes byte-identically" \
        || bad "comparative" "comparative-quote/1 drifted"
fi

if [ "$fails" -gt 0 ]; then
    echo "check-fp-select: $fails leg(s) failed" >&2
    exit 1
fi
echo "check-fp-select: all legs green"
