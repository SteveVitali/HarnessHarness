#!/usr/bin/env bash
# check-fp-p4.sh — recompute legs for the committed R7.1 phase-4
# admissibility records (manifest 326): the frozen `sweep-registry/1`
# (x2 identity), the sealed levels, the W-rule derivation and the
# admissibility record recompute byte-identical from the committed
# token-corpus record; sidecar hashes verify; the registry validates
# clean; the corpus record's input manifest is non-outcome.
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="$(mktemp -d)"
trap 'rm -rf "$REPO"' EXIT
mkdir -p "$REPO/scripts" "$REPO/docs/build/reports"
cp scripts/fp-p4-inputs.txt "$REPO/scripts/"
cp docs/build/reports/2026-11-20-fp-p4-token-corpus.json "$REPO/docs/build/reports/"

echo "== leg 1: emit byte-identical =="
BIN=./target/debug/p4_admissibility
[ -x "$BIN" ] || cargo build -q -p hharness-runner --bins
"$BIN" emit "$REPO" >/dev/null
for f in \
  2026-11-20-fp-p4-sweep-registry.json \
  2026-11-20-fp-p4-levels.json \
  2026-11-20-fp-p4-w-rule.json \
  2026-11-20-fp-p4-admissibility.json; do
  cmp -s "docs/build/reports/$f" "$REPO/docs/build/reports/$f" || {
    echo "FAIL: $f does not recompute byte-identical"; exit 1; }
done
echo "emit-identical OK"

echo "== leg 2: sha256 sidecars =="
for f in \
  2026-11-20-fp-p4-sweep-registry.sha256 \
  2026-11-20-fp-p4-levels.sha256 \
  2026-11-20-fp-p4-w-rule.sha256 \
  2026-11-20-fp-p4-admissibility.sha256 \
  2026-11-20-fp-p4-token-corpus.sha256; do
  shasum -a 256 -c "docs/build/reports/$f"
done

echo "== leg 3: registry + derivation shape =="
python3 - <<'PY'
import json, hashlib

reg = json.loads(open('docs/build/reports/2026-11-20-fp-p4-sweep-registry.json').read())
assert reg['schema'] == 'sweep-registry/1'
assert reg['exploration_identity'].startswith('x2:sha256:'), 'the pass freezes a NEW identity'
assert reg['frozen_sha256'] in reg['exploration_identity']
levels = {l['level_id']: l for l in reg['levels']}
assert set(levels) == {f'V{i}' for i in range(1, 10)}
for lid in [f'V{i}' for i in range(1, 8)]:
    assert levels[lid].get('sealed_ref'), f'{lid} unsealed'
    assert levels[lid].get('override_spellings'), f'{lid} missing spellings'
for lid in ['V8', 'V9']:
    assert 'sealed_ref' not in levels[lid], f'{lid} must stay unsealed'
    assert levels[lid]['withheld'] == 'withheld{inert_at_admissible_window}'
assert [t['level_id'] for t in reg['tier2']] == [
    'step-limit', 'format-error-cap', 'truncation-off', 'context-policy',
    'interaction-mode-shim', 'explicit-temperature', 'cost-limit']
assert all(t.get('reason') for t in reg['tier2']), 'tier-2 reasons required'

lvl = json.loads(open('docs/build/reports/2026-11-20-fp-p4-levels.json').read())
assert [l['level_id'] for l in lvl['levels']] == ['V1','V2','V3','V4','V5','V6','V7']
for l in lvl['levels']:
    assert l['level_ref'] == levels[l['level_id']]['sealed_ref'], 'sealed ref mismatch'
    assert l['delta'], f"{l['level_id']} sealed an inert document"

w = json.loads(open('docs/build/reports/2026-11-20-fp-p4-w-rule.json').read())
assert w['schema'] == 'hh-p4-w-rule/1'
outcome = w['derivation']['outcome']
assert outcome == 'withheld{inert_at_admissible_window}', outcome
# The F table is exact: recompute from the trial rows it read.
peaks = [t['peak_input_tokens'] for t in w['trials'] if t.get('measured')]
for row in w['derivation']['f']:
    assert row['exceed'] == sum(1 for p in peaks if p > row['w'])
    assert row['total'] == len(peaks)
# The input manifest selects only non-outcome fields.
OUTCOME = {'reward','verdict','outcome','resolved','resolve','score','pass',
           'passed','success','solved','answer','grade','result','label','winner'}
for f in w['input_manifest']['selected_fields']:
    last = f.strip().lower().rsplit('.', 1)[-1]
    assert f.strip().lower() not in OUTCOME and last not in OUTCOME, f

adm = json.loads(open('docs/build/reports/2026-11-20-fp-p4-admissibility.json').read())
assert adm['schema'] == 'hh-p4-admissibility/1'
assert adm['w_rule']['outcome'] == outcome
assert len(adm['tier2_admissions']) == 7
print('shape OK:', outcome)
PY

echo "== leg 4: lab acceptance battery =="
cargo test -q -p hharness-lab --test r7_1 -- --test-threads=1
cargo test -q -p hharness-lab --test r6_43

echo "check-fp-p4 OK"
