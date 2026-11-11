#!/usr/bin/env bash
# check-fp-sweep.sh — recompute legs for the committed sweep records
# (manifest 304/R6.43): the frozen `sweep-registry/1` and the sealed
# V5 level document recompute byte-identical from committed inputs;
# sidecar hashes verify; the registry validates clean.
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="$(mktemp -d)"
trap 'rm -rf "$REPO"' EXIT
mkdir -p "$REPO/scripts" "$REPO/docs/build/reports"
cp scripts/fp-sweep-inputs.txt "$REPO/scripts/"
cp docs/build/reports/2026-11-10-fp-prereg.json "$REPO/docs/build/reports/"

echo "== leg 1: emit byte-identical =="
BIN=./target/debug/fp_sweep
[ -x "$BIN" ] || cargo build -q -p hharness-runner --bins
"$BIN" emit "$REPO" >/dev/null
for f in 2026-11-11-fp-sweep-registry.json 2026-11-11-fp-sweep-v5.json; do
  cmp -s "docs/build/reports/$f" "$REPO/docs/build/reports/$f" || {
    echo "FAIL: $f does not recompute byte-identical"; exit 1; }
done
echo "emit-identical OK"

echo "== leg 2: sha256 sidecars =="
shasum -a 256 -c docs/build/reports/2026-11-11-fp-sweep-registry.sha256
shasum -a 256 -c docs/build/reports/2026-11-11-fp-sweep-v5.sha256

echo "== leg 3: canonical round-trip + registry validate =="
python3 - <<'PY'
import json, hashlib, sys
reg = open('docs/build/reports/2026-11-11-fp-sweep-registry.json').read()
doc = json.loads(reg)
assert doc['schema'] == 'sweep-registry/1'
assert doc['frozen_sha256']
v5 = [l for l in doc['levels'] if l['level_id'] == 'V5'][0]
assert v5['declaration_sha256'] == 'e950043fb986461e939839c8a1c892c512813e4aff7933fe106033cd6ab1ef0a'
lvl = json.loads(open('docs/build/reports/2026-11-11-fp-sweep-v5.json').read())
assert lvl['level_ref'] == v5['sealed_ref'], 'slot ref must equal the sealed level ref'
assert lvl['parity'] == 'none'
print('canonical + declaration + sealed-ref legs OK')
PY

echo "== leg 4: acceptance batteries =="
cargo test -q -p hharness-lab --test r6_43
cargo test -q -p hharness-runner --test r6_43
echo "check-fp-sweep: all legs OK"
