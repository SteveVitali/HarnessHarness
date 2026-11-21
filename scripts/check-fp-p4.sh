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
cp scripts/fp-p4-inputs-gen2.txt "$REPO/scripts/"
cp scripts/fp-p4-inputs-gen3.txt "$REPO/scripts/"
cp docs/build/reports/2026-11-20-fp-p4-token-corpus.json "$REPO/docs/build/reports/"
# The gen-2 supersede leg recomputes the lineage cites from the
# committed gen-1 records, and the gen-3 leg cites the committed
# gen-2 records — stage both generations at their committed paths.
for f in \
  2026-11-20-fp-p4-sweep-registry.json \
  2026-11-20-fp-p4-levels.json \
  2026-11-20-fp-p4-w-rule.json \
  2026-11-20-fp-p4-admissibility.json \
  2026-11-21-fp-p4-sweep-registry.json \
  2026-11-21-fp-p4-levels.json \
  2026-11-21-fp-p4-w-rule.json \
  2026-11-21-fp-p4-admissibility.json; do
  cp "docs/build/reports/$f" "$REPO/docs/build/reports/"
done

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

echo "== leg 4: gen-2 (R7.3) — supersede recomputes byte-identical =="
# The generation-2 mint: the same inputs manifest shape over the
# repaired transcript_render authoring; every emitted record
# recomputes byte-identical in the scratch tree.
HH_FP_P4_INPUTS=scripts/fp-p4-inputs-gen2.txt \
  "$BIN" supersede "$REPO" >/dev/null
for f in \
  2026-11-21-fp-p4-sweep-registry.json \
  2026-11-21-fp-p4-levels.json \
  2026-11-21-fp-p4-w-rule.json \
  2026-11-21-fp-p4-admissibility.json; do
  cmp -s "docs/build/reports/$f" "$REPO/docs/build/reports/$f" || {
    echo "FAIL: gen-2 $f does not recompute byte-identical"; exit 1; }
done
echo "gen-2 supersede-identical OK"

echo "== leg 5: gen-2 sha256 sidecars =="
for f in \
  2026-11-21-fp-p4-sweep-registry.sha256 \
  2026-11-21-fp-p4-levels.sha256 \
  2026-11-21-fp-p4-w-rule.sha256 \
  2026-11-21-fp-p4-admissibility.sha256; do
  shasum -a 256 -c "docs/build/reports/$f"
done

echo "== leg 6: gen-2 shape — fidelity set + lineage cites =="
python3 - <<'PY'
import json, hashlib

def sha(p):
    return 'sha256:' + hashlib.sha256(open(p,'rb').read()).hexdigest()

reg = json.loads(open('docs/build/reports/2026-11-21-fp-p4-sweep-registry.json').read())
assert reg['schema'] == 'sweep-registry/1'
assert reg['exploration_identity'].startswith('x2:sha256:')
assert reg['frozen_sha256'] in reg['exploration_identity']
# The new generation has a new identity.
g1 = json.loads(open('docs/build/reports/2026-11-20-fp-p4-sweep-registry.json').read())
assert reg['frozen_sha256'] != g1['frozen_sha256']
# The lineage cite binds the gen-1 file's actual bytes.
s = reg['supersedes']
assert s['prior'] == 'docs/build/reports/2026-11-20-fp-p4-sweep-registry.json'
assert s['prior_sha256'] == sha(s['prior'])
assert s['byte_untouched'] is True
assert 'transcript_render' in s['reason']
assert 'supersedes' not in g1, 'the gen-1 record is byte-untouched'

lvl = json.loads(open('docs/build/reports/2026-11-21-fp-p4-levels.json').read())
assert lvl['baseline_ref'].startswith('sha256:')
assert lvl['baseline_ref'] != json.loads(
    open('docs/build/reports/2026-11-20-fp-p4-levels.json').read())['baseline_ref']
sl = lvl['supersedes']
assert sl['prior_sha256'] == sha(sl['prior'])
# Every sealed level's bound transcript_render carries the complete
# ws1 fidelity set (the rerun-4 repair).
WANT = {'id_normalisation': 'provider_verbatim',
        'format_error_turn': 'omit',
        'text_only_turn': 'drop',
        'tool_arguments': 'verbatim'}
for l in lvl['levels']:
    tr = l['sealed']['document']['assembly']['slots']['model_port']['one']['params']['transcript_render']
    for k, v in WANT.items():
        assert tr[k] == v, f"{l['level_id']} transcript_render.{k}"
    ar = tr['assistant_replay']
    assert ar['content'] == 'verbatim', l['level_id']
    want_reason = 'drop' if l['level_id'] == 'V5' else 'verbatim'
    assert ar['reasoning'] == want_reason, (l['level_id'], ar['reasoning'])
assert [l['level_id'] for l in lvl['levels']] == ['V1','V2','V3','V4','V5','V6','V7']
assert {w['level_id'] for w in lvl['withheld']} == {'V8','V9'}

w = json.loads(open('docs/build/reports/2026-11-21-fp-p4-w-rule.json').read())
assert w['derivation']['outcome'] == 'withheld{inert_at_admissible_window}'
assert w['supersedes']['prior_sha256'] == sha(w['supersedes']['prior'])

adm = json.loads(open('docs/build/reports/2026-11-21-fp-p4-admissibility.json').read())
assert adm['schema'] == 'hh-p4-admissibility/1'
assert adm['supersedes']['prior_sha256'] == sha(adm['supersedes']['prior'])
print('gen-2 shape OK: baseline', lvl['baseline_ref'][:24],
      '— full fidelity set on every sealed level')
PY

echo "== leg 7: gen-3 (R7.3) — supersede recomputes byte-identical =="
# The generation-3 mint: the same inputs manifest shape over the
# gen-2 fidelity repair plus the declared control_strategy
# submission contract; every emitted record recomputes
# byte-identical in the scratch tree.
HH_FP_P4_INPUTS=scripts/fp-p4-inputs-gen3.txt \
  "$BIN" supersede "$REPO" >/dev/null
for f in \
  2026-11-21-fp-p4-sweep-registry-gen3.json \
  2026-11-21-fp-p4-levels-gen3.json \
  2026-11-21-fp-p4-w-rule-gen3.json \
  2026-11-21-fp-p4-admissibility-gen3.json; do
  cmp -s "docs/build/reports/$f" "$REPO/docs/build/reports/$f" || {
    echo "FAIL: gen-3 $f does not recompute byte-identical"; exit 1; }
done
echo "gen-3 supersede-identical OK"

echo "== leg 8: gen-3 sha256 sidecars =="
for f in \
  2026-11-21-fp-p4-sweep-registry-gen3.sha256 \
  2026-11-21-fp-p4-levels-gen3.sha256 \
  2026-11-21-fp-p4-w-rule-gen3.sha256 \
  2026-11-21-fp-p4-admissibility-gen3.sha256; do
  shasum -a 256 -c "docs/build/reports/$f"
done

echo "== leg 9: gen-3 shape — control contract + lineage cites =="
python3 - <<'PY'
import json, hashlib

def sha(p):
    return 'sha256:' + hashlib.sha256(open(p,'rb').read()).hexdigest()

reg = json.loads(open('docs/build/reports/2026-11-21-fp-p4-sweep-registry-gen3.json').read())
assert reg['schema'] == 'sweep-registry/1'
g2 = json.loads(open('docs/build/reports/2026-11-21-fp-p4-sweep-registry.json').read())
assert reg['frozen_sha256'] != g2['frozen_sha256'], 'gen-3 mints a new identity'
# The lineage cite binds the gen-2 file's actual bytes.
s = reg['supersedes']
assert s['prior'] == 'docs/build/reports/2026-11-21-fp-p4-sweep-registry.json'
assert s['prior_sha256'] == sha(s['prior'])
assert s['byte_untouched'] is True
assert 'control_strategy' in s['reason']
assert 'supersedes' in g2, 'the gen-2 record keeps its own lineage cite'

lvl = json.loads(open('docs/build/reports/2026-11-21-fp-p4-levels-gen3.json').read())
assert lvl['baseline_ref'] != json.loads(
    open('docs/build/reports/2026-11-21-fp-p4-levels.json').read())['baseline_ref']
sl = lvl['supersedes']
assert sl['prior_sha256'] == sha(sl['prior'])
# Every sealed level binds both repairs: the full ws1
# transcript_render fidelity set AND the control_strategy
# submission-marker contract (the gen-2 trajectory-termination fix).
WANT_TR = {'id_normalisation': 'provider_verbatim',
           'format_error_turn': 'omit',
           'text_only_turn': 'drop',
           'tool_arguments': 'verbatim'}
WANT_CS = {'completion_surface': 'none',
           'format_failure_scope': 'batch',
           'max_consecutive_format_errors': 3,
           'tool_batch_mode': 'sequential'}
WANT_STOP = {'line': 'first',
             'marker': 'COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT',
             'require_returncode': 0}
for l in lvl['levels']:
    slots = l['sealed']['document']['assembly']['slots']
    tr = slots['model_port']['one']['params']['transcript_render']
    for k, v in WANT_TR.items():
        assert tr[k] == v, f"{l['level_id']} transcript_render.{k}"
    ar = tr['assistant_replay']
    assert ar['content'] == 'verbatim', l['level_id']
    want_reason = 'drop' if l['level_id'] == 'V5' else 'verbatim'
    assert ar['reasoning'] == want_reason, (l['level_id'], ar['reasoning'])
    cs = slots['control_strategy']['one']['params']
    for k, v in WANT_CS.items():
        assert cs[k] == v, f"{l['level_id']} control_strategy.{k}"
    assert cs['stop_rule']['submit'] == WANT_STOP, l['level_id']
assert [l['level_id'] for l in lvl['levels']] == ['V1','V2','V3','V4','V5','V6','V7']
assert {w['level_id'] for w in lvl['withheld']} == {'V8','V9'}

w = json.loads(open('docs/build/reports/2026-11-21-fp-p4-w-rule-gen3.json').read())
assert w['derivation']['outcome'] == 'withheld{inert_at_admissible_window}'
assert w['supersedes']['prior_sha256'] == sha(w['supersedes']['prior'])

adm = json.loads(open('docs/build/reports/2026-11-21-fp-p4-admissibility-gen3.json').read())
assert adm['schema'] == 'hh-p4-admissibility/1'
assert adm['supersedes']['prior_sha256'] == sha(adm['supersedes']['prior'])
print('gen-3 shape OK: baseline', lvl['baseline_ref'][:24],
      '— fidelity set + submit marker on every sealed level')
PY

echo "== leg 10: lab acceptance batteries =="
cargo test -q -p hharness-lab --test r7_1 -- --test-threads=1
cargo test -q -p hharness-lab --test r7_3 -- --test-threads=1
cargo test -q -p hharness-lab --test r6_43

echo "check-fp-p4 OK"
