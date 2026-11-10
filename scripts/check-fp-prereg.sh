#!/usr/bin/env bash
# check-fp-prereg.sh — recompute legs for the hh-fp-prereg/1 packet set
# (manifest 291/R6.33). Every leg is offline, $0, and fails loudly.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"
FAIL=0

leg() { # leg <name> <cmd...>
  local name="$1"; shift
  if "$@" >/tmp/fp-prereg-leg.$$.log 2>&1; then
    echo "fp-prereg: $name OK"
  else
    echo "fp-prereg: $name FAIL"
    cat /tmp/fp-prereg-leg.$$.log
    FAIL=1
  fi
}

# 1 — the packet hash reproduces byte-identically from the committed
#     canonical JSON (the committed recompute command).
leg "packet-hash" shasum -a 256 -c docs/build/reports/2026-11-10-fp-prereg.sha256

# 2 — the emitter recomputes the whole record set byte-identically into
#     a scratch tree (same committed inputs, no clock, no network).
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH" /tmp/fp-prereg-leg.$$.log' EXIT
for rel in \
  docs/build/reports/2026-11-08-fp-select-commit.json \
  docs/build/reports/2026-11-08-fp-selection.json \
  docs/build/reports/2026-11-08-fp-exclusion-record.json \
  docs/build/reports/live/2026-11-10-reproof.json \
  docs/build/reports/compute/harvest/fp-w1/MANIFEST.sha256 \
  docs/build/reports/compute/harvest/fp-w1/run-root/prove-held/reproof-packet.json \
  docs/build/reports/compute/harvest/fp-w1/run-root/prove-held/reproof-register.json \
  docs/build/reports/compute/harvest/fp-w1/run-root/prove-dev/reproof-packet.json \
  docs/build/reports/compute/harvest/fp-w1/run-root/prove-dev/reproof-register.json \
  crates/hh-runner/tests/fixtures/r6_22/mini-swe-agent.json \
  crates/hh-runner/tests/fixtures/r6_22/pi.json \
  crates/hh-runner/tests/fixtures/r6_22/claude-code.json \
  scripts/gce-backend/declarations/fp-w1.json \
  scripts/gce-backend/declarations/fp-w2.json \
  scripts/gce-backend/declarations/fp-w3.json \
  scripts/gce-backend/declarations/fp-w4.json \
  scripts/gce-backend/declarations/fp-wc.json; do
  mkdir -p "$SCRATCH/$(dirname "$rel")"
  cp "$REPO/$rel" "$SCRATCH/$rel"
done
cargo build -p hharness-runner --bin fp_prereg -q
BIN="$REPO/target/debug/fp_prereg"
"$BIN" emit "$SCRATCH" >/dev/null
for out in \
  docs/build/reports/2026-11-10-fp-prereg.json \
  docs/build/reports/2026-11-10-fp-prereg.sha256 \
  docs/build/reports/2026-11-10-fp-w2-comparative-quote.json \
  docs/build/reports/2026-11-10-fp-fps-readiness.json; do
  leg "emit-identical:$(basename "$out")" cmp -s "$REPO/$out" "$SCRATCH/$out"
done

# 3 — the packet is canonical Json::to_canonical_string output: parse
#     and reserialize matches the committed bytes (minus newline).
leg "canonical-roundtrip" python3 - "$REPO" <<'PY'
import json, sys
p = json.load(open(sys.argv[1] + "/docs/build/reports/2026-11-10-fp-prereg.json"))
assert p["schema"] == "hh-fp-prereg/1", p.get("schema")
# canonical JSON is sorted-key compact — a re-dump in that order agrees
s = json.dumps(p, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
raw = open(sys.argv[1] + "/docs/build/reports/2026-11-10-fp-prereg.json", "rb").read().decode()
assert raw == s + "\n" or raw == s, "packet bytes are not canonical JSON"
PY

# 4 — a mutation anywhere in the packet changes the hash (AC-6).
leg "mutation-changes-hash" python3 - "$REPO" <<'PY'
import hashlib, json, sys
raw = open(sys.argv[1] + "/docs/build/reports/2026-11-10-fp-prereg.json", "rb").read()
orig = hashlib.sha256(raw).hexdigest()
recorded = open(sys.argv[1] + "/docs/build/reports/2026-11-10-fp-prereg.sha256").read().split()[0]
assert orig == recorded, "recorded hash does not match file bytes"
p = json.loads(raw)
p["effort"]["value"] = "low"
mut = json.dumps(p, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n"
assert hashlib.sha256(mut.encode()).hexdigest() != orig, "mutation did not change hash"
PY

# 5 — the committed readiness record is in its pending state.
leg "readiness-pending" python3 - "$REPO" <<'PY'
import json, sys
r = json.load(open(sys.argv[1] + "/docs/build/reports/2026-11-10-fp-fps-readiness.json"))
assert r["schema"] == "hh-fp-readiness/1"
assert r["result"]["state"] == "pending{HUMAN-H14 not run}", r["result"]
assert "never an input" in r["e2_t8_rung_record"]
PY

# 6 — the acceptance battery.
leg "tests" cargo test -p hharness-runner --test r6_33 -q

if [ "$FAIL" -ne 0 ]; then
  echo "fp-prereg: FAIL"
  exit 1
fi
echo "fp-prereg: all legs OK"
