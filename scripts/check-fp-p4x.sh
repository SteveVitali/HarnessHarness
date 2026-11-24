#!/usr/bin/env bash
# check-fp-p4x.sh — recompute legs for the committed R7.2 phase-4
# exploration pre-registration records (manifest 327): the frozen
# `hh-p4-x-prereg/1` and the `p4-selection-rule/1` rule record
# recompute byte-identical from the committed R7.1 x2 registry /
# levels / W-rule / admissibility records plus the frozen
# `task-selection/1` manifest; sidecar hashes verify; the declared
# envelope, the verbatim stop set, the byte-pinned §8.5 rule, the
# label spellings and the packet's exclusion/hash-ordering members all
# hold. The GATE-G35 leg: the p4x `hh-compute-decl/1` validates, its
# `hh-compute-quote/1` + `pq-` id recompute through receipt_lib's one
# §3.10 path, and the `comparative-quote/1` + `hh-p4-x-quote-inputs/1`
# records recompute byte-identical through `fp_p4x_quote` (C-11 — the
# single quote artifact; the packet authorizes nothing).
# The R7.4 legs (manifest 330): the offline, $0 fold — `p4_fold`
# recomputes the `hh-p4-x-exploration/1` packet, the `hh-p4-selection/1`
# packet and the frozen `hh-p4-c-prereg/1` byte-identical from the
# committed X1 evidence + gen-3 x2 records; the GATE-G36 leg validates
# the p4c `hh-compute-decl/1`, recomputes its `hh-compute-quote/1`
# + `pq-` id through receipt_lib's §3.10 path, and recomputes the
# X1-measured `comparative-quote/1` + `hh-p4-c-quote-inputs/1`
# byte-identical through `fp_p4c_quote`.
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="$(mktemp -d)"
trap 'rm -rf "$REPO"' EXIT
mkdir -p "$REPO/scripts/gce-backend/declarations" \
         "$REPO/docs/build/reports/live"
cp scripts/fp-p4x-inputs.txt "$REPO/scripts/"
cp scripts/fp-p4x-inputs-gen2.txt "$REPO/scripts/"
cp scripts/fp-p4x-inputs-gen3.txt "$REPO/scripts/"
cp scripts/gce-backend/declarations/fp-p4x.json \
   "$REPO/scripts/gce-backend/declarations/"
# The declaration moved after the gen-1/gen-2 mints (the runner.commit
# repoints, 6ece081f): the committed gen-1 and gen-2
# `hh-p4-x-quote-inputs/1` records cite the pre-repoint bytes. The
# pinned copy (`fp-p4x-4b04440e.json`, sha 4b04440e… — the exact bytes
# the records cite) preserves the cited input; the gen-1/gen-2 emit
# legs run over it at the cited path, and the live declaration returns
# for the gen-3 leg.
cp scripts/gce-backend/declarations/fp-p4x-4b04440e.json \
   "$REPO/scripts/gce-backend/declarations/fp-p4x.json.pinned"
cp docs/build/reports/live/pricing.json "$REPO/docs/build/reports/live/"
for f in \
  2026-11-20-fp-p4-sweep-registry.json \
  2026-11-20-fp-p4-levels.json \
  2026-11-20-fp-p4-w-rule.json \
  2026-11-20-fp-p4-admissibility.json \
  2026-11-20-fp-p4x-compute-quote.json \
  2026-11-08-fp-selection.json; do
  cp "docs/build/reports/$f" "$REPO/docs/build/reports/"
done
# The gen-2 legs (R7.3): the superseding inputs + the gen-1 prereg
# the lineage cite recomputes against. The gen-3 legs additionally
# read the committed gen-3 x2 records and cite the gen-2 prereg.
for f in \
  2026-11-21-fp-p4-sweep-registry.json \
  2026-11-21-fp-p4-levels.json \
  2026-11-21-fp-p4-w-rule.json \
  2026-11-21-fp-p4-admissibility.json \
  2026-11-20-fp-p4x-prereg.json \
  2026-11-20-fp-p4x-quote.json \
  2026-11-21-fp-p4-sweep-registry-gen3.json \
  2026-11-21-fp-p4-levels-gen3.json \
  2026-11-21-fp-p4-w-rule-gen3.json \
  2026-11-21-fp-p4-admissibility-gen3.json \
  2026-11-21-fp-p4x-prereg.json; do
  cp "docs/build/reports/$f" "$REPO/docs/build/reports/"
done

echo "== leg 1: emit byte-identical =="
BIN=./target/debug/p4_prereg
[ -x "$BIN" ] || cargo build -q -p hharness-runner --bins
"$BIN" emit "$REPO" >/dev/null
for f in \
  2026-11-20-fp-p4x-prereg.json \
  2026-11-20-fp-p4x-selection-rule.json; do
  cmp -s "docs/build/reports/$f" "$REPO/docs/build/reports/$f" || {
    echo "FAIL: $f does not recompute byte-identical"; exit 1; }
done
echo "emit-identical OK"

echo "== leg 2: sha256 sidecars =="
for f in \
  2026-11-20-fp-p4x-prereg.sha256 \
  2026-11-20-fp-p4x-selection-rule.sha256 \
  2026-11-20-fp-p4x-compute-quote.sha256 \
  2026-11-20-fp-p4x-quote.sha256 \
  2026-11-20-fp-p4x-quote-inputs.sha256; do
  shasum -a 256 -c "docs/build/reports/$f"
done

echo "== leg 3: pre-registration shape =="
python3 - <<'PY'
import json, hashlib

pre = json.loads(open('docs/build/reports/2026-11-20-fp-p4x-prereg.json').read())
reg = json.loads(open('docs/build/reports/2026-11-20-fp-p4-sweep-registry.json').read())
sel = json.loads(open('docs/build/reports/2026-11-08-fp-selection.json').read())
rule = open('docs/build/reports/2026-11-20-fp-p4x-selection-rule.json','rb').read()

assert pre['schema'] == 'hh-p4-x-prereg/1'
assert pre['ticket'] == '327_R7.2'

# Registry / W / withheld — cited verbatim, never re-typed.
assert pre['registry']['sha256'] == reg['frozen_sha256']
assert pre['registry']['exploration_identity'] == reg['exploration_identity']
assert pre['registry']['exploration_identity'].startswith('x2:sha256:')
assert pre['w']['outcome'] == 'withheld{inert_at_admissible_window}'
w_ids = {w['level_id']: w for w in pre['withheld']}
assert set(w_ids) == {'V8', 'V9'}
for w in w_ids.values():
    assert w['disposition'].startswith('withheld{'), w

# The task manifest: the band-A dev cells of the frozen task-selection
# record, in declared order; X0 is the first dev of each band-A cell.
x1 = []; x0 = []
for c in sel['cells']:
    if c['band'] != 'A':
        continue
    if c['dev']:
        x0.append(c['dev'][0])
        x1.extend(c['dev'])
assert pre['tasks']['x1'] == x1 and len(x1) == 11, pre['tasks']['x1']
assert pre['tasks']['x0'] == x0 and len(x0) == 3, pre['tasks']['x0']

# The declared envelope: X0 = 3 × 1 × all arms; X1 = 11 × 5 × arms,
# seeded slot_schedule_randomization in 5 rounds. The withheld levels
# never run — arms = admitted + baseline.
n_arms = 1 + sum(1 for l in reg['levels'] if 'withheld' not in l)
for leg, tasks, k in [('x0', 3, 1), ('x1', 11, 5)]:
    d = pre['design'][leg]
    exp = d['experiment']
    assert exp['kind'] == 'exploratory'
    assert exp['design']['kind'] == 'one_factor_at_a_time', leg
    assert d['tasks'] == tasks and d['replicates'] == k, leg
    assert d['arms'] == n_arms and d['slots'] == n_arms * tasks * k, leg
    # OFAT arms: base point + single-level perturbations exactly.
    varied = [f['name'] for f in exp['factors']
              if f['kind'] == 'harness' and len(f['levels']) >= 2]
    want = [{f: next(l for l in exp['factors'] if l['name'] == f)['levels'][0]['level_id']
             for f in varied}]
    for f in varied:
        lv = next(l for l in exp['factors'] if l['name'] == f)
        for lvl in lv['levels'][1:]:
            p = dict(want[0]); p[f] = lvl['level_id']; want.append(p)
    got = [a['level_assignment'] for a in exp['arms']]
    assert sorted(map(lambda p: tuple(sorted(p.items())), got)) == \
           sorted(map(lambda p: tuple(sorted(p.items())), want)), leg
x1d = pre['design']['x1']
assert x1d['rounds'] == 5 and 'slot_schedule_randomization' in x1d['schedule']
assert x1d['seed'] == x1d['experiment']['scheduling']['permutation_seed']
assert pre['design']['x1']['experiment']['scheduling']['order'] == 'random_permuted'
assert pre['envelope']['x0_slots'] == n_arms * 3
assert pre['envelope']['x1_slots'] == n_arms * 55 == 440
assert pre['envelope']['nominal_x1_slots'] == 550
# The X1 arms are exactly baseline + the admitted registry levels.
assert x1d['arm_ids'] == ['baseline'] + [l['level_id'] for l in reg['levels']
                                       if 'withheld' not in l]

# The stop set: the four §8.4 classes verbatim, nothing else.
assert pre['stop_rules']['classes'] == ['budget', 'infrastructure',
                                        'integrity', 'catalog_identity']
assert {r['class'] for r in pre['stop_rules']['rules']} == \
       set(pre['stop_rules']['classes'])

# The frozen rule: byte-equal to the committed rule record, and the
# record's declared sha member pins the rule file's bytes.
body = rule[:-1] if rule.endswith(b'\n') else rule
assert hashlib.sha256(body).hexdigest() == pre['selection_rule_sha256']
assert json.loads(rule) == pre['selection_rule']
r = pre['selection_rule']
assert r['m_max'] == 2 and r['one_level_per_factor'] is True
assert r['gates']['c']['floor_ppm'] == 250_000
assert r['gates']['d']['ceiling_ppm'] == 100_000
assert r['gates']['f']['multiple_ppm'] == 2_000_000
assert r['combination']['combine'] is True
assert r['combination']['scope'] == 'confirmation-only'
assert r['no_candidate']['disposition'] == 'withheld{no_candidate}'
assert 'positive exploration estimate' in r['no_candidate']['publishable']

# Labels: exploration / selection / combination spellings.
assert pre['labels']['exploration'].startswith('exploratory')
assert 'selected from the same data' in pre['labels']['selection']
assert '+.03' in pre['labels']['selection'] and '+.10' in pre['labels']['selection']
assert pre['labels']['combination'] == 'combination; interaction not estimated'
assert pre['combine_declaration']['combine'] is True

# The selection packet contract: X1-only inputs, deterministic,
# hash-ordered before the confirmation pre-registration.
pkt = pre['selection_packet']
assert pkt['schema'] == 'hh-p4-selection/1'
assert 'held-out' in pkt['input_manifest'] and 'phase-2/3' in pkt['input_manifest']
assert 'byte-identical' in pkt['determinism']
assert 'predates' in pkt['hash_order']

# Honesty members: no spend, hash-before-subjects, the honest expected
# outcome, the x2 cohort statement.
assert '$0' in pre['spend']
assert 'GATE-G35' in pre['predates']
assert 'x2' in pre['cohort']
print('shape OK: envelope 24 + 440 executable slots, V8/V9 withheld')
PY

echo "== leg 4: GATE-G35 quote — compute + comparative recompute =="
# The gen-1/gen-2 quote-inputs records cite the pre-repoint
# declaration — stage the pinned bytes at the cited path for the
# recompute legs (the priced envelope is identical across repoints;
# only runner.commit moved).
cp "$REPO/scripts/gce-backend/declarations/fp-p4x.json.pinned" \
   "$REPO/scripts/gce-backend/declarations/fp-p4x.json"
# The declaration validates through the closed-schema validator.
python3 -I scripts/gce-backend/validate_decl.py \
  scripts/gce-backend/declarations/fp-p4x.json
# The compute quote recomputes byte-identical through receipt_lib's
# one §3.10 path, and its pq- id recomputes from the declaration.
python3 -I scripts/gce-backend/receipt_lib.py quote \
  "$REPO/scripts/gce-backend/declarations/fp-p4x.json" "$REPO" \
  > "$REPO/recomputed-compute-quote.json"
cmp -s docs/build/reports/2026-11-20-fp-p4x-compute-quote.json \
  "$REPO/recomputed-compute-quote.json" || {
  echo "FAIL: compute quote does not recompute byte-identical"; exit 1; }
PQ="$(python3 -I scripts/gce-backend/receipt_lib.py quoteid \
  "$REPO/scripts/gce-backend/declarations/fp-p4x.json" "$REPO")"
[ "$PQ" = "pq-59a49747bfa5bdf0" ] || {
  echo "FAIL: quoteid recompute $PQ"; exit 1; }
# The comparative-quote/1 + inputs records recompute byte-identical
# through the one quote path (fp_p4x_quote → comparative_quote).
QBIN=./target/debug/fp_p4x_quote
[ -x "$QBIN" ] || cargo build -q -p hharness-runner --bins
"$QBIN" emit "$REPO" >/dev/null
for f in \
  2026-11-20-fp-p4x-quote.json \
  2026-11-20-fp-p4x-quote-inputs.json; do
  cmp -s "docs/build/reports/$f" "$REPO/docs/build/reports/$f" || {
    echo "FAIL: $f does not recompute byte-identical"; exit 1; }
done
python3 - <<'PY'
import json

q = json.loads(open('docs/build/reports/2026-11-20-fp-p4x-quote.json').read())
i = json.loads(open('docs/build/reports/2026-11-20-fp-p4x-quote-inputs.json').read())
pre = json.loads(open('docs/build/reports/2026-11-20-fp-p4x-prereg.json').read())

assert q['schema'] == 'comparative-quote/1'
assert q['envelope_label'] == 'pre-quote'
assert q['t'] == 1 and q['k'] == 1 and q['concurrency_c'] == 4

# Slot-level rows: X0 = 24 lines, X1 = 440 executable lines — the
# withheld V8/V9 are never priced.
rows = {r['row_id']: r for r in q['rows']}
assert set(rows) == {'p4x-x0', 'p4x-x1'}
assert rows['p4x-x0']['n_trials'] == 24 and rows['p4x-x1']['n_trials'] == 440
assert rows['p4x-x0']['h'] == 24 and rows['p4x-x1']['h'] == 440
levels = [a['arm_id'] for a in pre['design']['x0']['experiment']['arms']]
assert len(levels) == 8 and 'V8' not in levels and 'V9' not in levels
x0 = rows['p4x-x0']; x1 = rows['p4x-x1']
assert all(a.split(':')[0] in levels for a in x0['arms'])
assert all(a.split(':')[0] in levels for a in x1['arms'])
# Every model figure is the declared §10.4 top-point prior — the
# measure_ref labels it, never a measurement.
for r in (x0, x1):
    assert 'DECLARED planning prior' in r['measure_refs'][0]
    assert r['verdict'] == 'unresolved{sim_absent}'
assert x0['model']['expected_micros'] == 24 * 340_000
assert x1['model']['expected_micros'] == 440 * 340_000
assert x0['model']['ceiling_micros'] == 24 * 510_000 * 12 // 10
assert x1['model']['ceiling_micros'] == 440 * 510_000 * 12 // 10

# The compute line is a verbatim citation — never re-priced.
assert q['compute'] == [{'ceiling_micros': 13_486_340,
                        'quote_id': 'pq-59a49747bfa5bdf0',
                        'window': 'p4x'}]
assert q['compute_ceiling_micros'] == 13_486_340

# The inputs record: committed inputs with hashes, declared priors,
# the honesty members — the packet authorizes nothing.
assert i['schema'] == 'hh-p4-x-quote-inputs/1'
assert i['records']['compute_quote']['quote_id'] == 'pq-59a49747bfa5bdf0'
assert i['declared_model_priors']['p_hat_micros_per_slot'] == 340_000
assert i['declared_model_priors']['p90_micros_per_slot'] == 510_000
assert i['envelope']['x0_slots'] == 24 and i['envelope']['x1_slots'] == 440
assert i['envelope']['nominal_x1_slots'] == 550
assert {w['level_id'] for w in i['envelope']['withheld']} == {'V8', 'V9'}
assert i['honesty']['authorizes_nothing'] is True
print('quote OK: comparative-quote/1', q['quote_id'][:16],
      '— 24 + 440 slots at the declared top point; compute cited')
PY

echo "== leg 5: gen-2 (R7.3) — supersede recomputes byte-identical =="
# The superseding generation-2 mint: the same envelope over the
# repaired transcript_render baseline; the emitted prereg and the
# byte-pinned rule recompute byte-identical in the scratch tree.
HH_FP_P4X_INPUTS=scripts/fp-p4x-inputs-gen2.txt \
  "$BIN" supersede "$REPO" >/dev/null
cmp -s "docs/build/reports/2026-11-21-fp-p4x-prereg.json" \
  "$REPO/docs/build/reports/2026-11-21-fp-p4x-prereg.json" || {
  echo "FAIL: gen-2 prereg does not recompute byte-identical"; exit 1; }
cmp -s "docs/build/reports/2026-11-20-fp-p4x-selection-rule.json" \
  "$REPO/docs/build/reports/2026-11-20-fp-p4x-selection-rule.json" || {
  echo "FAIL: selection rule not byte-identical"; exit 1; }
# The comparative quote re-emits byte-identical at its gen-1 path
# (the priced envelope is unchanged); the inputs record mints gen-2.
HH_FP_P4X_INPUTS=scripts/fp-p4x-inputs-gen2.txt \
  "$QBIN" emit "$REPO" >/dev/null
cmp -s "docs/build/reports/2026-11-20-fp-p4x-quote.json" \
  "$REPO/docs/build/reports/2026-11-20-fp-p4x-quote.json" || {
  echo "FAIL: gen-2 quote not byte-identical"; exit 1; }
cmp -s "docs/build/reports/2026-11-21-fp-p4x-quote-inputs.json" \
  "$REPO/docs/build/reports/2026-11-21-fp-p4x-quote-inputs.json" || {
  echo "FAIL: gen-2 quote-inputs does not recompute byte-identical"; exit 1; }
echo "gen-2 supersede-identical OK"

echo "== leg 6: gen-2 sha256 sidecars =="
for f in \
  2026-11-21-fp-p4x-prereg.sha256 \
  2026-11-21-fp-p4x-quote-inputs.sha256; do
  shasum -a 256 -c "docs/build/reports/$f"
done

echo "== leg 7: gen-2 prereg shape — lineage + envelope parity =="
python3 - <<'PY'
import json, hashlib

def sha(p):
    return 'sha256:' + hashlib.sha256(open(p,'rb').read()).hexdigest()

g2 = json.loads(open('docs/build/reports/2026-11-21-fp-p4x-prereg.json').read())
g1 = json.loads(open('docs/build/reports/2026-11-20-fp-p4x-prereg.json').read())
reg = json.loads(open('docs/build/reports/2026-11-21-fp-p4-sweep-registry.json').read())

assert g2['schema'] == 'hh-p4-x-prereg/1'
assert g2['ticket'] == '329_R7.3'
# The lineage cite binds the gen-1 record's actual bytes — the same
# pin the .sha256 sidecar reports.
s = g2['supersedes']
assert s['prior'] == 'docs/build/reports/2026-11-20-fp-p4x-prereg.json'
assert s['prior_sha256'] == sha(s['prior'])
assert s['byte_untouched'] is True
assert 'transcript_render' in s['reason']
assert 'supersedes' not in g1, 'the gen-1 record is byte-untouched'
# The gen-2 mint honestly postdates the gate — never borrows gen-1's
# "predates GATE-G35" wording.
assert 'postdates GATE-G35' in g2['predates']
# The registry citation binds the new generation's registry.
assert g2['registry']['sha256'] == reg['frozen_sha256']
assert g2['registry']['exploration_identity'] == reg['exploration_identity']
# The baseline ref moved; the envelope is verbatim modulo digests.
assert g2['baseline']['baseline_ref'] != g1['baseline']['baseline_ref']

def strip(o):
    if isinstance(o, dict):
        return {k: strip(v) for k, v in o.items()
                if k not in ('ref', 'version_id', 'artifact_ref', 'experiment_id')}
    if isinstance(o, list):
        return [strip(v) for v in o]
    return o
assert strip(g2['design']) == strip(g1['design']), 'design verbatim modulo digests'
for member in ['envelope', 'tasks', 'withheld', 'stop_rules',
               'selection_rule', 'selection_rule_sha256', 'estimators',
               'labels', 'combine_declaration', 'selection_packet',
               'integrity', 'expected_outcome', 'spend', 'cohort']:
    assert g2[member] == g1[member], member
assert g2['w']['outcome'] == g1['w']['outcome']
assert g2['w']['verbatim'] == g1['w']['verbatim']
# The quote-inputs record cites the gen-2 prereg by hash.
qi = json.loads(open('docs/build/reports/2026-11-21-fp-p4x-quote-inputs.json').read())
assert qi['records']['prereg']['path'] == \
    'docs/build/reports/2026-11-21-fp-p4x-prereg.json'
assert qi['records']['prereg']['sha256'] == \
    hashlib.sha256(open(qi['records']['prereg']['path'], 'rb').read()).hexdigest()
assert qi['records']['compute_quote']['quote_id'] == 'pq-59a49747bfa5bdf0'
assert qi['envelope']['x0_slots'] == 24 and qi['envelope']['x1_slots'] == 440
assert {w['level_id'] for w in qi['envelope']['withheld']} == {'V8', 'V9'}
print('gen-2 prereg OK:', sha('docs/build/reports/2026-11-21-fp-p4x-prereg.json')[:24],
      '— envelope verbatim, baseline moved')
PY

echo "== leg 8: gen-3 (R7.3) — supersede recomputes byte-identical =="
# The superseding generation-3 mint: the same envelope over the
# baseline carrying the control_strategy submission contract; the
# emitted prereg and the byte-pinned rule recompute byte-identical.
HH_FP_P4X_INPUTS=scripts/fp-p4x-inputs-gen3.txt \
  "$BIN" supersede "$REPO" >/dev/null
cmp -s "docs/build/reports/2026-11-21-fp-p4x-prereg-gen3.json" \
  "$REPO/docs/build/reports/2026-11-21-fp-p4x-prereg-gen3.json" || {
  echo "FAIL: gen-3 prereg does not recompute byte-identical"; exit 1; }
cmp -s "docs/build/reports/2026-11-20-fp-p4x-selection-rule.json" \
  "$REPO/docs/build/reports/2026-11-20-fp-p4x-selection-rule.json" || {
  echo "FAIL: selection rule not byte-identical"; exit 1; }
# The comparative quote re-emits byte-identical at its gen-1 path
# (the priced envelope is unchanged); the inputs record mints gen-3.
# The gen-3 record cites the live declaration — restore it at the
# cited path first.
cp scripts/gce-backend/declarations/fp-p4x.json \
   "$REPO/scripts/gce-backend/declarations/fp-p4x.json"
HH_FP_P4X_INPUTS=scripts/fp-p4x-inputs-gen3.txt \
  "$QBIN" emit "$REPO" >/dev/null
cmp -s "docs/build/reports/2026-11-20-fp-p4x-quote.json" \
  "$REPO/docs/build/reports/2026-11-20-fp-p4x-quote.json" || {
  echo "FAIL: gen-3 quote not byte-identical"; exit 1; }
cmp -s "docs/build/reports/2026-11-21-fp-p4x-quote-inputs-gen3.json" \
  "$REPO/docs/build/reports/2026-11-21-fp-p4x-quote-inputs-gen3.json" || {
  echo "FAIL: gen-3 quote-inputs does not recompute byte-identical"; exit 1; }
echo "gen-3 supersede-identical OK"

echo "== leg 9: gen-3 sha256 sidecars =="
for f in \
  2026-11-21-fp-p4x-prereg-gen3.sha256 \
  2026-11-21-fp-p4x-quote-inputs-gen3.sha256; do
  shasum -a 256 -c "docs/build/reports/$f"
done

echo "== leg 10: gen-3 prereg shape — lineage + envelope parity =="
python3 - <<'PY'
import json, hashlib

def sha(p):
    return 'sha256:' + hashlib.sha256(open(p,'rb').read()).hexdigest()

g3 = json.loads(open('docs/build/reports/2026-11-21-fp-p4x-prereg-gen3.json').read())
g2 = json.loads(open('docs/build/reports/2026-11-21-fp-p4x-prereg.json').read())
reg = json.loads(open('docs/build/reports/2026-11-21-fp-p4-sweep-registry-gen3.json').read())

assert g3['schema'] == 'hh-p4-x-prereg/1'
assert g3['ticket'] == '329_R7.3'
# The lineage cite binds the gen-2 record's actual bytes.
s = g3['supersedes']
assert s['prior'] == 'docs/build/reports/2026-11-21-fp-p4x-prereg.json'
assert s['prior_sha256'] == sha(s['prior'])
assert s['byte_untouched'] is True
assert 'control_strategy' in s['reason']
assert 'supersedes' in g2, 'the gen-2 record keeps its lineage cite'
assert 'postdates GATE-G35' in g3['predates']
# The registry citation binds the gen-3 registry.
assert g3['registry']['sha256'] == reg['frozen_sha256']
assert g3['registry']['exploration_identity'] == reg['exploration_identity']
assert g3['baseline']['baseline_ref'] != g2['baseline']['baseline_ref']

def strip(o):
    if isinstance(o, dict):
        return {k: strip(v) for k, v in o.items()
                if k not in ('ref', 'version_id', 'artifact_ref', 'experiment_id')}
    if isinstance(o, list):
        return [strip(v) for v in o]
    return o
assert strip(g3['design']) == strip(g2['design']), 'design verbatim modulo digests'
for member in ['envelope', 'tasks', 'withheld', 'stop_rules',
               'selection_rule', 'selection_rule_sha256', 'estimators',
               'labels', 'combine_declaration', 'selection_packet',
               'integrity', 'expected_outcome', 'spend', 'cohort']:
    assert g3[member] == g2[member], member
assert g3['w']['outcome'] == g2['w']['outcome']
assert g3['w']['verbatim'] == g2['w']['verbatim']
# The quote-inputs record cites the gen-3 prereg by hash.
qi = json.loads(open('docs/build/reports/2026-11-21-fp-p4x-quote-inputs-gen3.json').read())
assert qi['records']['prereg']['path'] == \
    'docs/build/reports/2026-11-21-fp-p4x-prereg-gen3.json'
assert qi['records']['prereg']['sha256'] == \
    hashlib.sha256(open(qi['records']['prereg']['path'], 'rb').read()).hexdigest()
assert qi['records']['compute_quote']['quote_id'] == 'pq-59a49747bfa5bdf0'
assert qi['envelope']['x0_slots'] == 24 and qi['envelope']['x1_slots'] == 440
assert {w['level_id'] for w in qi['envelope']['withheld']} == {'V8', 'V9'}
print('gen-3 prereg OK:', sha('docs/build/reports/2026-11-21-fp-p4x-prereg-gen3.json')[:24],
      '— envelope verbatim, baseline moved')
PY

echo "== leg 11: R7.4 — fold emit recomputes byte-identical =="
# The fold inputs: the committed X1 evidence + the gen-3 x2 records +
# the frozen task-selection record.
cp scripts/fp-p4x-fold-inputs.txt scripts/fp-p4c-inputs.txt "$REPO/scripts/"
for f in \
  2026-11-24-fp-p4x1-slot-matrix.jsonl \
  2026-11-24-fp-p4x1-slots-lost.json \
  2026-11-24-fp-p4x1-wal-slot-costs.json \
  2026-11-24-fp-p4x1-wal-model-events.jsonl \
  2026-11-24-fp-p4x1-session-wal.tar.gz \
  2026-11-24-fp-p4x1-window-fragment.txt \
  2026-11-21-fp-p4-sweep-registry-gen3.json \
  2026-11-21-fp-p4-levels-gen3.json \
  2026-11-21-fp-p4x-prereg-gen3.json \
  2026-11-08-fp-selection.json; do
  cp "docs/build/reports/$f" "$REPO/docs/build/reports/"
done
FBIN=./target/debug/p4_fold
[ -x "$FBIN" ] || cargo build -q -p hharness-runner --bins
"$FBIN" emit "$REPO" >/dev/null
for f in \
  2026-11-24-fp-p4x-exploration.json \
  2026-11-24-fp-p4x-selection.json \
  2026-11-24-fp-p4c-prereg.json; do
  cmp -s "docs/build/reports/$f" "$REPO/docs/build/reports/$f" || {
    echo "FAIL: $f does not recompute byte-identical"; exit 1; }
done
echo "fold emit-identical OK"

echo "== leg 12: R7.4 sha256 sidecars =="
for f in \
  2026-11-24-fp-p4x-exploration.sha256 \
  2026-11-24-fp-p4x-selection.sha256 \
  2026-11-24-fp-p4c-prereg.sha256 \
  2026-11-24-fp-p4c-quote.sha256 \
  2026-11-24-fp-p4c-quote-inputs.sha256; do
  shasum -a 256 -c "docs/build/reports/$f"
done

echo "== leg 13: R7.4 fold + selection + c-prereg shape =="
python3 - <<'PY'
import json, hashlib

e = json.loads(open('docs/build/reports/2026-11-24-fp-p4x-exploration.json').read())
s = json.loads(open('docs/build/reports/2026-11-24-fp-p4x-selection.json').read())
c = json.loads(open('docs/build/reports/2026-11-24-fp-p4c-prereg.json').read())

assert e['schema'] == 'hh-p4-x-exploration/1' and e['ticket'] == '330_R7.4'
# WAL authoritative + the verbatim residual; detail_lost recorded,
# never imputed; the fidelity census covers every served call.
assert e['total_cost_micros'] == 19_250_389
assert e['non_slot_attribution_micros'] == 8
assert e['detail_lost']['count'] == 46
assert 'never imputed' in e['detail_lost']['disposition']
assert e['served_model_census'] == ['glm-5.3-flash×10602']
assert e['completed_calls_census'] == 10_602
b = e['baseline']
assert b['completion_envelope'] == {'planned': 55, 'recorded': 55,
                                    'resolved': 50, 'success': 25,
                                    'unresolved': 5}
assert b['cost']['per_executed_slot_micros'] == 46_165
lv = {l['level_id']: l for l in e['levels']}
assert len(lv) == 7
assert lv['V1']['delta_ppm'] == 45_455
assert lv['V1']['activation']['ppm'] == 1_000_000
assert 'unmeasured{evidence_absent}' in lv['V5']['activation']['detail']

# The selection packet: content-hash self-consistency + the outcome.
assert s['schema'] == 'hh-p4-selection/1'
# The content-hash member recomputes inside the r7_4 battery (its
# canonical-minus-member convention); here the committed value is
# pinned — the value the c-prereg cites.
assert s['packet_sha256'] == \
    'sha256:9c3c314c2e1db2872dae24c350f93a508c7e08de90c99658957d541ea3ccfe23'
out = s['outcome']
assert out['disposition'] == 'selected{m=1}'
assert [m['member'] for m in out['members']] == ['V1']
gt = {r['level_id']: r for r in out['gate_table']}
assert gt['V1']['qualified'] is True
assert all(gt['V1']['gates'][g]['pass'] for g in 'abcdef')
for i in ('V2', 'V5', 'V6'):
    assert gt[i]['gates']['d']['pass'] is False and not gt[i]['qualified']
for i in ('V3', 'V4', 'V7'):
    assert not gt[i]['gates']['c']['pass'] and not gt[i]['gates']['d']['pass']

# The frozen c-prereg: members verbatim, T=37 × k=5, hash-ordered.
assert c['schema'] == 'hh-p4-c-prereg/1'
assert c['disposition'] == 'selected{m=1}'
assert [m['member'] for m in c['members']] == ['baseline', 'V1']
assert c['members'][0]['rank'] == 0 and 'control re-run' in c['members'][0]['role']
assert c['members'][1]['rank'] == 1
assert len(c['held_out']) == 37
assert c['design']['k'] == 5 and c['design']['slots_per_member'] == 185
assert c['design']['total_slots'] == 370
assert 'Holm' in c['design']['multiplicity']
assert c['selection_packet_sha256'] == s['packet_sha256']
assert 'predates' in c['hash_order'] and '$0' in c['spend']
assert 'never run in X0/X1' in c['cohort']
print('fold shape OK: selected{m=1} → baseline + V1 over T=37 × k=5')
PY

echo "== leg 14: GATE-G36 quote — compute + comparative recompute =="
cp scripts/gce-backend/declarations/fp-p4c.json \
   "$REPO/scripts/gce-backend/declarations/"
python3 -I scripts/gce-backend/validate_decl.py \
  scripts/gce-backend/declarations/fp-p4c.json
python3 -I scripts/gce-backend/receipt_lib.py quote \
  "$REPO/scripts/gce-backend/declarations/fp-p4c.json" "$REPO" \
  > "$REPO/recomputed-p4c-compute-quote.json"
cmp -s docs/build/reports/2026-11-24-fp-p4c-compute-quote.json \
  "$REPO/recomputed-p4c-compute-quote.json" || {
  echo "FAIL: p4c compute quote does not recompute byte-identical"; exit 1; }
PQ="$(python3 -I scripts/gce-backend/receipt_lib.py quoteid \
  "$REPO/scripts/gce-backend/declarations/fp-p4c.json" "$REPO")"
[ "$PQ" = "pq-3a4187759b2d8411" ] || {
  echo "FAIL: p4c quoteid recompute $PQ"; exit 1; }
# The committed quote inputs the emitter cites.
for f in \
  2026-11-24-fp-p4x-exploration.json \
  2026-11-24-fp-p4c-prereg.json \
  2026-11-24-fp-p4c-compute-quote.json; do
  cp "docs/build/reports/$f" "$REPO/docs/build/reports/"
done
CBIN=./target/debug/fp_p4c_quote
[ -x "$CBIN" ] || cargo build -q -p hharness-runner --bins
"$CBIN" emit "$REPO" >/dev/null
for f in \
  2026-11-24-fp-p4c-quote.json \
  2026-11-24-fp-p4c-quote-inputs.json; do
  cmp -s "docs/build/reports/$f" "$REPO/docs/build/reports/$f" || {
    echo "FAIL: $f does not recompute byte-identical"; exit 1; }
done
python3 - <<'PY'
import json

q = json.loads(open('docs/build/reports/2026-11-24-fp-p4c-quote.json').read())
i = json.loads(open('docs/build/reports/2026-11-24-fp-p4c-quote-inputs.json').read())

assert q['schema'] == 'comparative-quote/1'
assert q['envelope_label'] == 'pre-quote'
assert q['t'] == 1 and q['k'] == 1 and q['concurrency_c'] == 4
rows = {r['row_id']: r for r in q['rows']}
assert set(rows) == {'p4c-baseline', 'p4c-V1'}
# One row per c-prereg member; 185 priced slots; every figure
# X1-MEASURED — never the §10.4 declared priors.
for rid, r in rows.items():
    assert r['n_trials'] == 185 and r['h'] == 185, rid
    assert 'X1-measured' in r['measure_refs'][0], rid
    assert 'never a declared planning prior' in r['measure_refs'][0]
    assert r['verdict'] == 'unresolved{sim_absent}'
assert rows['p4c-baseline']['model'] == {'ceiling_micros': 36_302_550,
                                         'expected_micros': 8_540_525}
assert rows['p4c-V1']['model'] == {'ceiling_micros': 10_051_050,
                                   'expected_micros': 3_317_790}
assert all(a.startswith('baseline:') for a in rows['p4c-baseline']['arms'])
assert all(a.startswith('V1:') for a in rows['p4c-V1']['arms'])
assert q['compute'] == [{'ceiling_micros': 12_229_780,
                        'quote_id': 'pq-3a4187759b2d8411',
                        'window': 'p4c'}]
assert q['compute_ceiling_micros'] == 12_229_780

assert i['schema'] == 'hh-p4-c-quote-inputs/1'
assert i['records']['compute_quote']['quote_id'] == 'pq-3a4187759b2d8411'
assert 'X1-measured' in i['model_line_basis']['basis']
assert i['scenarios']['disposition'] == 'selected{m=1}'
assert i['scenarios']['selection_conditioned'] is True
assert i['envelope']['held_out_tasks'] == 37
assert i['envelope']['slots_per_member'] == 185
assert i['honesty']['authorizes_nothing'] is True
print('quote OK: comparative-quote/1', q['quote_id'][:16],
      '— baseline + V1, 370 slots at X1-measured rates; compute cited')
PY

echo "== leg 15: lab acceptance batteries =="
cargo test -q -p hharness-lab --test r7_2 -- --test-threads=1
cargo test -q -p hharness-lab --test r7_3 -- --test-threads=1
cargo test -q -p hharness-lab --test r7_4 -- --test-threads=1
cargo test -q -p hharness-lab --test r7_1 -- --test-threads=1
cargo test -q -p hharness-lab --test r6_43
cargo test -q -p hharness-embed --test r7_3

echo "check-fp-p4x OK"
