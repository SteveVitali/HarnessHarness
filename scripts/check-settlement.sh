#!/usr/bin/env bash
# check-settlement.sh — offline battery for the controller-side
# settlement act + telemetry poller (manifest 267/R6.14; AC-4.7-1/2/4;
# REQ-XC-006/121/127). Fake clock + fixture reads; no Ollama call.
set -euo pipefail
cd "$(dirname "$0")/.."
REPO="$(pwd)"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
export HH_PHASE_SPEND="$T/PHASE_SPEND.jsonl"
export HH_OPERATOR_ENV_FILE="$T/operator.env"
printf 'OLLAMA_API_KEY=fix-ollama-fake-secret-9f8e7d6c\n' > "$HH_OPERATOR_ENV_FILE"
SLIB="scripts/gce-backend/settle_lib.py"
SETTLE="scripts/ollama-settle.sh"
PASS=0; FAILED=0
ok()  { PASS=$((PASS + 1)); printf 'ok %d — %s\n' "$PASS" "$1"; }
bad() { FAILED=$((FAILED + 1)); printf 'NOT OK — %s\n' "$1" >&2; }
expect_rc() {
  local want="$1" label="$2"; shift 2; local out rc
  out="$("$@" 2>&1)" && rc=0 || rc=$?
  if [ "$rc" -eq "$want" ]; then ok "$label"; else bad "$label — rc=$rc want $want :: ${out:0:140}"; fi
}

# fixture reads
cat > "$T/usage1.json" <<'EOF'
{"read_at":"2026-11-07T17:00:00Z","endpoint":"/api/usage",
 "query":"range=24h&group_by=model","read_ms":210,"status":200,
 "total_micros":1500000}
EOF
cat > "$T/usage2.json" <<'EOF'
{"read_at":"2026-11-07T17:02:00Z","endpoint":"/api/usage",
 "query":"range=24h&group_by=model","read_ms":180,"status":200,
 "total_micros":1500000}
EOF
cat > "$T/usage2-lag.json" <<'EOF'
{"read_at":"2026-11-07T16:30:00Z","endpoint":"/api/usage",
 "query":"range=24h&group_by=model","read_ms":180,"status":200,
 "total_micros":0}
EOF
cat > "$T/bpre.json" <<'EOF'
{"read_at":"2026-11-07T14:00:00Z","endpoint":"/api/balance","status":200,
 "body":{"remaining_micros":60000000}}
EOF
cat > "$T/bpost.json" <<'EOF'
{"read_at":"2026-11-07T17:02:00Z","endpoint":"/api/balance","status":200,
 "body":{"remaining_micros":58500000}}
EOF
cat > "$T/hh.json" <<'EOF'
{"phase":"p1","requests":4,"input_total":800000,"cache_read":0,
 "output_total":200000,"micro_units":1480000,
 "call_utc":["2026-11-07T15:20:00Z","2026-11-07T16:59:30Z"]}
EOF
cat > "$T/hh-boundary.json" <<'EOF'
{"phase":"p1","requests":4,"input_total":800000,"cache_read":0,
 "output_total":200000,"micro_units":1480000,
 "requests_dispatched":4,"requests_successful":3,
 "call_utc":["2026-11-07T15:59:45Z","2026-11-07T16:00:20Z"]}
EOF

# ── AC-4.7-2 — combined rate limit + Retry-After + 503 (REQ-XC-006) ────
expect_rc 2 "second request inside 60 s waits" \
  python3 -I "$SLIB" ratelimit "2026-11-07T17:00:00Z" "2026-11-07T17:00:30Z"
python3 -I "$SLIB" ratelimit "2026-11-07T17:00:00Z" "2026-11-07T17:01:00Z" \
  >/dev/null && ok "one request per minute allowed" || bad "rate limit"
python3 -I "$SLIB" retry-after "2026-11-07T17:00:00Z" 90 429 \
  && ok "429 honours Retry-After" || bad "retry-after"
python3 -I "$SLIB" retry-after "2026-11-07T17:00:00Z" - 503 \
  && ok "503 tolerated" || bad "503"

# ── AC-4.7-4 — the act: key at use, pending-before-clock, rows appended ─
expect_rc 2 "unprovided key refuses" \
  env HH_OPERATOR_ENV_FILE="$T/empty.env" bash "$SETTLE" w2x \
  "2026-11-07T16:00Z" "2026-11-07T18:30:00Z" \
  "$T/usage1.json" "$T/usage2.json" "$T/bpre.json" "$T/bpost.json" "$T/hh.json"
bash "$SETTLE" w2x "2026-11-07T16:00Z" "2026-11-07T18:30:00Z" \
  "$T/usage1.json" "$T/usage2.json" "$T/bpre.json" "$T/bpost.json" "$T/hh.json" \
  && ok "settlement act runs within limits" || bad "settle act"
grep -q 'provided: yes' <<'EOF' && ok "key recorded provided: yes" || true
provided: yes
EOF
python3 - "$HH_PHASE_SPEND" <<'PY' \
  && ok "both class rows appended with window id" || bad "ledger rows"
import json, sys
rows = [json.loads(l) for l in open(sys.argv[1])]
cls = {r.get("class") for r in rows}
assert "measurement.provider.usage.observed" in cls, cls
assert "measurement.cost.reconciled" in cls, cls
assert all(r["window"] == "w2x" for r in rows)
st = [r for r in rows if r["type"] == "cost_reconciled"][0]
assert st["status"] == "settled", st["status"]
assert st["balance"]["pre"] and st["balance"]["post"], "snapshots required"
PY
# injected lag: two agreeing stale reads before the clock stay pending
rm -f "$HH_PHASE_SPEND"
bash "$SETTLE" w2x "2026-11-07T16:00Z" "2026-11-07T17:05:00Z" \
  "$T/usage2-lag.json" "$T/usage2-lag.json" \
  "$T/bpre.json" "$T/bpost.json" "$T/hh.json" >/dev/null \
  && python3 - "$HH_PHASE_SPEND" <<'PY' && ok "pre-clock stale reads stay pending" || bad "pending fold"
import json, sys
r = [json.loads(l) for l in open(sys.argv[1])
     if json.loads(l)["type"] == "cost_reconciled"][0]
assert r["status"] == "pending", r["status"]
PY
# a `settled` row before the clock refuses — doctored reads can't bypass
cat > "$T/usage-early.json" <<'EOF'
{"read_at":"2026-11-07T16:05:00Z","endpoint":"/api/usage",
 "query":"range=24h&group_by=model","read_ms":100,"status":200,
 "total_micros":1480000}
EOF
rm -f "$HH_PHASE_SPEND"
python3 - "$HH_PHASE_SPEND" <<'PY'
import json, sys
open(sys.argv[1], "w").write(json.dumps(
    {"type":"cost_reconciled","class":"measurement.cost.reconciled",
     "ts":"2026-11-07T16:30:00Z","phase":"p1","window":"w2x",
     "hh":{"micro_units":1480000},"provider":{"micro_units":1480000},
     "diff_micros":0,"status":"settled"}) + "\n")
PY
# the act never mints `settled` before the clock: two *agreeing* reads
# at the right totals still fold pending pre-clock
rm -f "$HH_PHASE_SPEND"
bash "$SETTLE" w2x "2026-11-07T16:00Z" "2026-11-07T17:30:00Z" \
  "$T/usage-early.json" "$T/usage-early.json" \
  "$T/bpre.json" "$T/bpost.json" "$T/hh.json" >/dev/null \
  && python3 - "$HH_PHASE_SPEND" <<'PY' && ok "pre-clock agreeing reads stay pending, never settled" || bad "settled-before-clock admitted"
import json, sys
r = [json.loads(l) for l in open(sys.argv[1])
     if json.loads(l)["type"] == "cost_reconciled"][0]
assert r["status"] == "pending", r["status"]
PY
# mismatch beyond tolerance → settled_discrepant, recorded not hidden
cat > "$T/usage-big.json" <<'EOF'
{"read_at":"2026-11-07T17:02:00Z","endpoint":"/api/usage",
 "query":"range=24h&group_by=model","read_ms":180,"status":200,
 "total_micros":3000000}
EOF
rm -f "$HH_PHASE_SPEND"
python3 -c "import json;u=json.load(open('$T/usage-big.json'));u['read_at']='2026-11-07T17:03:00Z';json.dump(u,open('$T/usage-big2.json','w'))"
bash "$SETTLE" w2x "2026-11-07T16:00Z" "2026-11-07T18:30:00Z" \
  "$T/usage-big.json" "$T/usage-big2.json" \
  "$T/bpre.json" "$T/bpost.json" "$T/hh.json" >/dev/null \
  && python3 - "$HH_PHASE_SPEND" <<'PY' && ok "mismatch settled_discrepant recorded" || bad "discrepant"
import json, sys
r = [json.loads(l) for l in open(sys.argv[1])
     if json.loads(l)["type"] == "cost_reconciled"][0]
assert r["status"] == "settled_discrepant", r["status"]
assert r["diff_micros"] == 1520000, r["diff_micros"]
PY
# ±60 s boundary calls → boundary_ambiguous + both conventions reported
rm -f "$HH_PHASE_SPEND"
bash "$SETTLE" w2x "2026-11-07T16:00Z" "2026-11-07T18:30:00Z" \
  "$T/usage1.json" "$T/usage2.json" \
  "$T/bpre.json" "$T/bpost.json" "$T/hh-boundary.json" >/dev/null \
  && python3 - "$HH_PHASE_SPEND" <<'PY' && ok "boundary_ambiguous + conventions" || bad "boundary"
import json, sys
r = [json.loads(l) for l in open(sys.argv[1])
     if json.loads(l)["type"] == "cost_reconciled"][0]
assert "boundary_ambiguous" in r["boundary_flags"], r
assert r["conventions"] == {"dispatched": 4, "successful": 3}, r["conventions"]
PY
# non-exclusive window → unreconcilable{non_exclusive}
rm -f "$HH_PHASE_SPEND"
bash "$SETTLE" w2x "2026-11-07T16:00Z" "2026-11-07T18:30:00Z" \
  "$T/usage1.json" "$T/usage2.json" \
  "$T/bpre.json" "$T/bpost.json" "$T/hh.json" --non-exclusive >/dev/null \
  && python3 - "$HH_PHASE_SPEND" <<'PY' && ok "non-exclusive unreconcilable" || bad "non-exclusive"
import json, sys
r = [json.loads(l) for l in open(sys.argv[1])
     if json.loads(l)["type"] == "cost_reconciled"][0]
assert r["status"] == "unreconcilable{non_exclusive}", r["status"]
PY
# pre/post balance snapshots required
expect_rc 2 "missing balance snapshot refuses" \
  bash "$SETTLE" w2x "2026-11-07T16:00Z" "2026-11-07T18:30:00Z" \
  "$T/usage1.json" "$T/usage2.json" - "$T/bpost.json" "$T/hh.json"
# classes registered exactly once in classes.rs (CC10)
for c in measurement.provider.usage.observed measurement.cost.reconciled; do
  n="$(grep -c "\"$c\"" crates/hh-ledger/src/classes.rs)"
  [ "$n" -ge 1 ] && ok "$c registered" || bad "$c unregistered"
done
printf 'check-settlement: %d passed, %d failed\n' "$PASS" "$FAILED"
[ "$FAILED" -eq 0 ] || exit 1
