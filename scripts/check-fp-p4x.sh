#!/usr/bin/env bash
# check-fp-p4x.sh — recompute legs for the committed R7.2 phase-4
# exploration pre-registration records (manifest 327): the frozen
# `hh-p4-x-prereg/1` and the `p4-selection-rule/1` rule record
# recompute byte-identical from the committed R7.1 x2 registry /
# levels / W-rule / admissibility records plus the frozen
# `task-selection/1` manifest; sidecar hashes verify; the declared
# envelope, the verbatim stop set, the byte-pinned §8.5 rule, the
# label spellings and the packet's exclusion/hash-ordering members all
# hold.
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="$(mktemp -d)"
trap 'rm -rf "$REPO"' EXIT
mkdir -p "$REPO/scripts" "$REPO/docs/build/reports"
cp scripts/fp-p4x-inputs.txt "$REPO/scripts/"
for f in \
  2026-11-20-fp-p4-sweep-registry.json \
  2026-11-20-fp-p4-levels.json \
  2026-11-20-fp-p4-w-rule.json \
  2026-11-20-fp-p4-admissibility.json \
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
  2026-11-20-fp-p4x-selection-rule.sha256; do
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

echo "== leg 4: lab acceptance batteries =="
cargo test -q -p hharness-lab --test r7_2 -- --test-threads=1
cargo test -q -p hharness-lab --test r7_1 -- --test-threads=1
cargo test -q -p hharness-lab --test r6_43

echo "check-fp-p4x OK"
