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
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="$(mktemp -d)"
trap 'rm -rf "$REPO"' EXIT
mkdir -p "$REPO/scripts/gce-backend/declarations" \
         "$REPO/docs/build/reports/live"
cp scripts/fp-p4x-inputs.txt "$REPO/scripts/"
cp scripts/gce-backend/declarations/fp-p4x.json \
   "$REPO/scripts/gce-backend/declarations/"
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

echo "== leg 5: lab acceptance batteries =="
cargo test -q -p hharness-lab --test r7_2 -- --test-threads=1
cargo test -q -p hharness-lab --test r7_1 -- --test-threads=1
cargo test -q -p hharness-lab --test r6_43

echo "check-fp-p4x OK"
