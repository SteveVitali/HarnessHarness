#!/usr/bin/env bash
# check-fp-p4c.sh — recompute legs for the committed R7.6 confirmation
# fold (manifest 333): `p4c_fold` recomputes the `hh-p4-c-confirmation/1`
# packet byte-identical from the committed P4-C records (the frozen
# `hh-p4-c-prereg/1`, the `hh-p4-selection/1` packet it cites, and the
# executed `hh-fp-window/1` r4 window record whose slot_results member
# embeds the 370-slot matrix verbatim — plus the two earlier terminal
# window records); the sidecar verifies; the packet's honest shape
# holds (367/370 resolved, the 3 unresolved members typed, V1
# inconclusive{unresolved_cell} retained in the Holm family at
# worst-case mass); the §8.8 claims leg scans the packet under the
# banned/conditional phrase set; the r7_6 battery runs the AC-1..AC-3
# legs.
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="$(mktemp -d)"
trap 'rm -rf "$REPO"' EXIT
mkdir -p "$REPO/scripts" "$REPO/docs/build/reports"
cp scripts/fp-p4c-fold-inputs.txt "$REPO/scripts/"
for f in \
  2026-11-24-fp-p4c-prereg.json \
  2026-11-24-fp-p4x-selection.json \
  2026-11-26-fp-p4c-window-r4.json \
  2026-11-26-fp-p4c-window.json \
  2026-11-26-fp-p4c-window-r2-attempt1-native_lane_hold.json; do
  cp "docs/build/reports/$f" "$REPO/docs/build/reports/"
done

echo "== leg 1: R7.6 — confirmation fold emit recomputes byte-identical =="
FBIN=./target/debug/p4c_fold
[ -x "$FBIN" ] || cargo build -q -p hharness-runner --bin p4c_fold
"$FBIN" emit "$REPO" >/dev/null
cmp -s "docs/build/reports/2026-11-26-fp-p4c-confirmation.json" \
       "$REPO/docs/build/reports/2026-11-26-fp-p4c-confirmation.json" || {
  echo "FAIL: confirmation packet does not recompute byte-identical"; exit 1; }
# Deterministic twice.
"$FBIN" emit "$REPO" >/dev/null
cmp -s "docs/build/reports/2026-11-26-fp-p4c-confirmation.json" \
       "$REPO/docs/build/reports/2026-11-26-fp-p4c-confirmation.json" || {
  echo "FAIL: confirmation packet is not deterministic"; exit 1; }
echo "fold emit-identical OK"

echo "== leg 2: R7.6 sha256 sidecar =="
shasum -a 256 -c docs/build/reports/2026-11-26-fp-p4c-confirmation.sha256

echo "== leg 3: R7.6 packet shape — population, estimators, dispositions =="
python3 - <<'PY'
import json

j = json.loads(open('docs/build/reports/2026-11-26-fp-p4c-confirmation.json').read())

assert j['schema'] == 'hh-p4-c-confirmation/1' and j['ticket'] == '333_R7.6'
assert j['packet_disposition'] == \
    'executed — no selected member was confirmed different from the ' \
    'baseline at Holm α = .05 on 37 held-out tasks'
# The population — 367/370 resolved; the 3 unresolved members stay typed.
p = j['population']
assert p['kind'] == 'joint_task_pass_without_observed_veto'
assert (p['tasks'], p['slots'], p['resolved'], p['unresolved']) == (37, 370, 367, 3)
assert p['complete'] is False
assert sorted(m['slot_id'] for m in p['unresolved_members']) == [
    'p4c-V1-model-extraction-relu-logits-r4',
    'p4c-baseline-headless-terminal-r2',
    'p4c-baseline-kv-store-grpc-r5',
]
assert sorted(p['unresolved_tasks']) == [
    'headless-terminal', 'kv-store-grpc', 'model-extraction-relu-logits']
# V1's member row — every confirmatory member refused typed; the
# all-planned bound and the labelled conditional bootstrap publish.
v1 = j['estimand']['contrast']['members'][0]
assert v1['member'] == 'V1'
assert v1['disposition'] == 'inconclusive{unresolved_cell}'
assert v1['delta_ppm']['state'] == 'refused'
assert 'UnresolvedCell' in v1['delta_ppm']['reason']
assert v1['fm_interval']['state'] == 'refused'
assert v1['status'] == 'unavailable'
assert v1['adjusted_p_ppm'] == 1_000_000
assert v1['holm_rejected_at_alpha'] is False
assert v1['bound_ppm']['lo_ppm'] <= 0 <= v1['bound_ppm']['hi_ppm']
b = v1['task_bootstrap_interval']
assert b['eligible_tasks'] == 34 and len(b['excluded_tasks']) == 3
assert 'conditional secondary' in b['label']
assert b['lo_ppm'] <= 0 <= b['hi_ppm']
# The Holm family — the unavailable member stays at worst-case mass.
f = j['family']
assert f['id'] == 'P4-C1' and f['method'] == 'holm' and f['alpha_ppm'] == 50_000
assert len(f['members']) == 1
assert f['members'][0]['status'] == 'unavailable'
assert f['members'][0]['unavailable'].startswith('UnresolvedCell')
assert f['members'][0]['adjusted_p_ppm'] == 1_000_000
# Per-arm rows — θ̂ refused typed; envelopes + bounds publish.
arms = {a['arm']: a for a in j['estimand']['arms']}
assert arms['baseline']['completion_envelope'] == {
    'planned': 185, 'first': 183, 'recovered': 0, 'unresolved': 2}
assert arms['V1']['completion_envelope'] == {
    'planned': 185, 'first': 184, 'recovered': 0, 'unresolved': 1}
assert arms['baseline']['theta_ppm']['state'] == 'refused'
assert arms['baseline']['conditional_secondary']['tasks'] == 35
assert arms['V1']['conditional_secondary']['tasks'] == 36
# The dispositions member is the pre-registered typed set.
assert j['dispositions'] == [
    {'member': 'V1', 'disposition': 'inconclusive{unresolved_cell}'}]
# The exchangeability record seals the emitted slot order.
e = j['exchangeability']
assert e['kind'] == 'k_round_interleave'
assert e['exchangeability'] == 'slot_schedule_randomization'
assert e['rounds'] == 5 and e['seed'] == 'p4-c-schedule-r7.4-2026-11-24'
assert len(e['digest']) == 64
# The terminal-state record — CAP7.1's input.
t = j['terminal_state']
assert t['run_id'] == 'p4c-c-r4' and t['slots_recorded'] == 370
assert t['terminal_disposition'] == 'recorded' and t['lane_probe'] == 'pass'
assert [a['terminal'] for a in t['earlier_attempts']] == [
    'stop{budget}', 'native_lane_hold']
# The input manifest cites every consumed record at cohort c.
rels = [i['relpath'] for i in j['input_manifest']]
assert rels == [
    'docs/build/reports/2026-11-24-fp-p4c-prereg.json',
    'docs/build/reports/2026-11-24-fp-p4x-selection.json',
    'docs/build/reports/2026-11-26-fp-p4c-window-r4.json',
    'docs/build/reports/2026-11-26-fp-p4c-window.json',
    'docs/build/reports/2026-11-26-fp-p4c-window-r2-attempt1-native_lane_hold.json',
]
assert all(i['sha256'].startswith('sha256:') for i in j['input_manifest'])
print('packet shape OK: 367/370 resolved; V1 inconclusive{unresolved_cell}, Holm-retained')
PY

echo "== leg 4: R7.6 §8.8 claims leg — banned/conditional over the packet =="
python3 - <<'PY'
import json, re

j = json.loads(open('docs/build/reports/2026-11-26-fp-p4c-confirmation.json').read())
lex = json.loads(open('scripts/claims-lexicon.json').read())

def strings(o):
    if isinstance(o, str):
        yield o
    elif isinstance(o, list):
        for v in o: yield from strings(v)
    elif isinstance(o, dict):
        for v in o.values(): yield from strings(v)

def phrase_set(key):
    out = []
    for e in lex.get(key, []):
        out.append(e if isinstance(e, str) else e['phrase'])
    return out

banned = phrase_set('banned') + phrase_set('record_banned')
site_banned = phrase_set('site_banned')
corpus = list(strings(j))
for s in corpus:
    low = s.lower()
    for b in banned:
        assert b.lower() not in low, f'packet carries banned phrase {b!r}: {s!r}'
# The site-bound statements carry none of the site set either.
c = j['claims']
assert c['form'] == 'two-statement (§8.8)'
for m in ('statement_exploratory', 'statement_confirmatory'):
    low = c[m].lower()
    for b in banned + site_banned:
        assert b.lower() not in low, f'{m} carries banned phrase {b!r}'
# Conditional phrases never stand outside their allow patterns.
for e in lex['conditional']:
    pat = re.compile(e['allow_pattern'], re.IGNORECASE)
    for s in corpus:
        if e['phrase'].lower() in s.lower():
            assert pat.search(s) or 'not equivalence' in s \
                or 'interactions not estimated' in s \
                or 'never a claim' in s, \
                f"conditional {e['phrase']!r} outside its allow context: {s!r}"
# The mandatory labels are present verbatim.
lab = c['labels']
assert 'finite-manifest' in lab['finite_manifest_scope']
assert lab['ofat_caveat'] == \
    'one-factor-at-a-time: effects are local to the baseline configuration; ' \
    'interactions not estimated'
assert 'catalog window' in lab['catalog_window']
assert 'baseline' in lab['baseline_label']
# Selection-labelled estimates never stand as claims.
for m in c['exploratory_estimates']['members']:
    assert 'exploratory' in m['label']
# Cost is descriptive-only.
assert 'descriptive' in c['cost']['label'] and 'no confirmatory' in c['cost']['label']
print('claims leg OK: two-statement form; banned/conditional clean; labels verbatim')
PY

echo "== leg 5: R7.6 acceptance battery =="
cargo test -q -p hharness-runner --test r7_6 -- --test-threads=1

echo "check-fp-p4c OK"
