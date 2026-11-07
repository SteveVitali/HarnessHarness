# Independent machine check of the Round-6 partition and coverage register (v3: Phase-4 revision +
# operator re-partition of 2026-11-07 — ~45-row sizing, ≤ 95k focused + 10k reserve, 3-run minimal tail).
# Reads only the four committed-to-be artifacts plus the literal sources they cite:
#   docs/experiments-spec.md (Appendix A, normative rows, <a id> anchors),
#   docs/build/planning/2026-11-07_round6-coverage.json, …-coverage.md (literal owner table, owner index),
#   docs/build/planning/2026-11-07_round6-decomposition.md (row table, totals, shared-decision table).
# It never reads the generator's data. Every check is isolated: a missing id, a parse failure or a
# malformed field is a FAIL line, never a traceback.
# Usage: python3 -I check_round6_coverage.py <repo_root>        (exit 0 = PASS, 1 = FAIL)
import json, re, sys, os, hashlib
from collections import Counter, defaultdict

root = sys.argv[1]
P = lambda p: os.path.join(root, p)
res = []
def ck(name, fn):
    try:
        out = fn()
        ok, d = (out if isinstance(out, tuple) else (bool(out), ''))
    except Exception as e:  # graceful: a crash inside a check is that check's FAIL
        ok, d = False, f'{type(e).__name__}: {e}'
    res.append((name, ok, d))

def read(p):
    with open(P(p), encoding='utf-8') as f: return f.read()
def cells(line): return [c.strip() for c in re.split(r'(?<!\\)\|', line.strip())[1:-1]]
CID = r'(?:GATE-G\d+|HUMAN-H\d+|R6\.\d+[a-z]?|CAP6\.\d+|REC6\.\d+|DOC6\.\d+)'
def short(xs, n=6): xs = list(xs); return str(xs[:n]) + (f' …(+{len(xs)-n})' if len(xs) > n else '')

L = {}
for k, p in [('spec', 'docs/experiments-spec.md'), ('json', 'docs/build/planning/2026-11-07_round6-coverage.json'),
             ('md', 'docs/build/planning/2026-11-07_round6-coverage.md'), ('plan', 'docs/build/planning/2026-11-07_round6-decomposition.md')]:
    try: L[k] = read(p)
    except Exception as e: L[k] = None; res.append((f'read {p}', False, str(e)))
spec, MD, PLAN = L['spec'] or '', L['md'] or '', L['plan'] or ''
try: J = json.loads(L['json'] or '{}')
except Exception as e: J = {}; res.append(('coverage json parses', False, str(e)))
C = {}
for c in J.get('candidates', []):
    if isinstance(c, dict) and 'id' in c: C.setdefault(c['id'], c)

# ---------- 1. spec: Appendix A + normative rows ----------
A, AROW, SROW = {}, {}, {}
m = re.search(r'## Appendix A — Requirement index(.*?)\n<a id="appendix-b">', spec, re.S)
for line in (m.group(1) if m else '').split('\n'):
    mm = re.match(r'^\| (REQ-X[A-H]-\d+) \| ([^|]+?) \| ([^|]+?) \| ([^|]+?) \| ([^|]+?) \|', line)
    if mm:
        A[mm.group(1)] = dict(disp=mm.group(2).strip(), sec=mm.group(3).strip(), hook=mm.group(4).strip().replace(' (target)', '').strip('`'), layer=mm.group(5).strip())
        AROW[mm.group(1)] = line
for line in spec.split('\n'):
    mm = re.match(r'^\| \*\*(REQ-X[A-H]-\d+)\*\* \|', line)
    if mm: SROW[mm.group(1)] = line
anchors = set(re.findall(r'<a id="([^"]+)"', spec))
adopted = {i for i, v in A.items() if v['disp'].startswith('adopted')}
ck('spec sha256 equals register supplement_sha256', lambda: hashlib.sha256(open(P('docs/experiments-spec.md'), 'rb').read()).hexdigest() == J.get('supplement_sha256'))
ck('Appendix A: 456 candidates, 370 adopted; every adopted id has one normative row', lambda: (len(A) == 456 and len(adopted) == 370 and adopted <= set(SROW), f'{len(A)}/{len(adopted)}; missing rows {short(adopted - set(SROW))}'))

# ---------- 2. plan row table ----------
rows, order, rowtable_owned, parse_err = {}, [], {}, []
for line in PLAN.split('\n'):
    if not re.match(r'^\| \d{3} \| `', line): continue
    c = cells(line)
    if len(c) != 13: parse_err.append(line[:60]); continue
    seq, f, cid, kind, deps, size = int(c[0]), c[1].strip('`'), c[2], c[3], c[8], c[9]
    dl = [d.strip() for d in deps.split(',')]
    rows[cid] = dict(seq=seq, file=f, kind=kind, deps=dl, size=size, scope=c[5], alias=c[12])
    order.append(cid)
    oc = c[7]
    mm = re.match(r'^(\d+)(?: \([^)]*\))?: (.*)$', oc)
    rowtable_owned[cid] = (int(mm.group(1)), ['REQ-' + x.strip() for x in mm.group(2).split(',')]) if mm else (0, []) if oc == '—' else (-1, [oc])
order.sort(key=lambda x: rows[x]['seq'])
N = len(order)
ck('row table parses (13 cells per row)', lambda: (not parse_err and N > 0, f'{N} rows; bad {short(parse_err)}'))
ck(f'sequences contiguous from 252', lambda: ([rows[x]['seq'] for x in order] == list(range(252, 252 + N)), f'{N} rows'))
tot = re.search(r'\*\*total rows\*\* \| \*\*(\d+)\*\*', PLAN)
ck('Totals table total equals the row-table count', lambda: (tot is not None and int(tot.group(1)) == N, f'{tot.group(1) if tot else None} vs {N}'))
def totals_kinds():
    kc = Counter(r['kind'] for r in rows.values()); bad = []
    for k in ('ticket', 'skeleton', 'gate', 'human'):
        mm = re.search(r'^\| ' + k + r' \| (\d+)', PLAN, re.M)
        if not mm or int(mm.group(1)) != kc[k]: bad.append((k, mm.group(1) if mm else None, kc[k]))
    mm = re.search(r'^\| capstone / reconcile / docs \| (\d+) / (\d+) / (\d+)', PLAN, re.M)
    if not mm or (int(mm.group(1)), int(mm.group(2)), int(mm.group(3))) != (kc['capstone'], kc['reconcile'], kc['docs']): bad.append('tail kinds')
    return not bad, str(bad)
ck('Totals table kind counts equal the row table', totals_kinds)
gram = re.compile(r'^(GATE|HUMAN)-[A-Z0-9]+$|^[A-Z]+\d*(\.\d+[a-z0-9]*)?$')
ck('chain ids match guard grammar; filenames open with seq and id; kinds in the closed set', lambda: all(
    gram.match(c) and r['file'].startswith(f"{r['seq']:03d}_{c}__") and r['kind'] in {'ticket','gate','human','skeleton','capstone','reconcile','docs'} for c, r in rows.items()))
def deps_ok():
    bad = []
    for c, r in rows.items():
        if r['deps'] == ['nothing']: continue
        for d in r['deps']:
            if d not in rows or rows[d]['seq'] >= r['seq']: bad.append((c, d))
    return not bad, short(bad)
ck('depends-on: "nothing" or chain rows only (never "—"), backward only', deps_ok)
ck('only the bootstrap gate depends on nothing', lambda: [c for c, r in rows.items() if r['deps'] == ['nothing']] == [order[0]])
ck('tail (operator Qs 2026-11-07: minimal tail, then "Split into 4-run tail") contiguous at end: capstone, capstone, capstone, gate, docs; its gate filename contains "accept"; no reconcile row', lambda: (
    [rows[x]['kind'] for x in order[-5:]] == ['capstone', 'capstone', 'capstone', 'gate', 'docs'] and 'accept' in rows[order[-2]]['file']
    and not any(r['kind'] == 'reconcile' for r in rows.values()) and all(rows[x]['kind'] not in ('capstone', 'docs') for x in order[:-5])))
ck('row 1 bootstrap gate; row 2 is the single guard row R6.1, before any product row', lambda: (
    rows[order[0]]['kind'] == 'gate' and order[1] == 'R6.1' and rows['R6.1']['kind'] == 'ticket' and all(rows[x]['seq'] > rows['R6.1']['seq'] for x in order[2:])))
def skel():
    bad = []
    for c, r in rows.items():
        if r['kind'] != 'skeleton': continue
        g = [d for d in r['deps'] if rows.get(d, {}).get('kind') == 'gate']
        if not g or 'fleshed after gate' not in r['size']: bad.append(c)
    return len([1 for r in rows.values() if r['kind'] == 'skeleton']) == 5 and not bad, short(bad)
ck('five window skeletons, each depending on a preceding gate, no size/Run budget', skel)
def human_place():
    bad = []
    for i, c in enumerate(order):
        if rows[c]['kind'] != 'human': continue
        j = i + 1
        while j < N and rows[order[j]]['kind'] == 'human': j += 1
        nx = order[j] if j < N else None
        if nx is None or not (rows[nx]['kind'] in ('gate', 'capstone') or c in rows[nx]['deps']): bad.append((c, nx))
    return not bad, short(bad)
ck('HUMAN rows sit only before a gate, the tail, or a row that depends on them (SR-15/16)', human_place)
def sizes():
    bad = []
    for c, r in rows.items():
        mm = re.match(r'(\d+)k \+ 10k', r['size'])
        if r['kind'] in ('ticket', 'capstone', 'reconcile', 'docs') and not mm: bad.append((c, 'no size')); continue
        if mm and int(mm.group(1)) + 10 > 105 and 'operator-ruled' not in r['size']: bad.append(c)
    return not bad, short(bad)
ck('every implement-spec row sized; ≤ 95k focused + 10k reserve (105k) unless marked operator-ruled', sizes)
ck('operator-ruled oversize rows are tail capstones only (the closure capstone)', lambda: (lambda x: (all(rows[c]['kind'] == 'capstone' and c in order[-4:] for c in x), short(x)))(
    [c for c, r in rows.items() if 'operator-ruled' in r['size']]))
def impl_total():
    mm = re.search(r'\*\*implement-spec rows\*\* \| \*\*(\d+)\*\*', PLAN)
    n = sum(1 for r in rows.values() if r['kind'] in ('ticket', 'capstone', 'reconcile', 'docs'))
    return mm is not None and int(mm.group(1)) == n and 40 <= n <= 50, f'{mm.group(1) if mm else None} vs {n} (band 40–50)'
ck('Totals implement-spec count equals the row table and sits in the operator band 40–50', impl_total)

ck('record-only window rows folded: ARCHIVE-UPLOAD / SETTLE acts live only in window skeletons', lambda: (lambda b: (not b, short(b)))(
    [c for c, r in rows.items() if re.search(r'ARCHIVE-UPLOAD|SETTLE-W', r['alias']) and r['kind'] != 'skeleton']))
def lin_b():
    mm = re.search(r'<!-- linearization-B-prime: ([^>]*?) -->', PLAN)
    if not mm: return False, 'marker missing'
    mv = mm.group(1).split()
    if any(x not in rows for x in mv) or 'GATE-G29' not in rows: return False, 'unknown id'
    ob = [x for x in order if x not in mv]; i = ob.index('GATE-G29') + 1; ob = ob[:i] + mv + ob[i:]
    pos = {x: j for j, x in enumerate(ob)}
    bad = [(c, d) for c, r in rows.items() if r['deps'] != ['nothing'] for d in r['deps'] if d in pos and pos[d] >= pos[c]]
    return not bad, short(bad)
ck("linearization B′ (marked rows moved to just after GATE-G29) stays backward-only", lin_b)

# ---------- 3. register json ----------
ck('json candidate ids unique and == Appendix A', lambda: (len(J.get('candidates', [])) == len(C) and set(C) == set(A), short(set(A) ^ set(C))))
ck('json dispositions/layers/sections match Appendix A', lambda: (not (b := [i for i in A if i not in C or C[i].get('layer') != A[i]['layer'] or C[i].get('section') != A[i]['sec'] or not A[i]['disp'].startswith(C[i].get('disposition', '\0'))]), short(b)))
ck('json: no null or empty-string field in any candidate', lambda: (not (b := [(i, k) for i, c in C.items() for k, v in c.items() if v is None or v == '']), short(b)))
def lit_checks():
    bad, cache = [], {}
    for i, c in C.items():
        src = c.get('literal_source', '')
        if src not in cache:
            try: cache[src] = open(P(src), 'rb').read()
            except Exception: cache[src] = None
        raw = cache[src]
        lr = c.get('literal_row') or ''
        if raw is None: bad.append((i, 'source missing')); continue
        if c.get('source_sha256') != hashlib.sha256(lr.encode()).hexdigest(): bad.append((i, 'source_sha256'))
        if c.get('literal_source_sha256') != hashlib.sha256(raw).hexdigest(): bad.append((i, 'literal_source_sha256'))
        if not lr or lr.encode() not in raw: bad.append((i, 'literal_row not verbatim in source'))
        if i not in lr: bad.append((i, 'id absent from literal_row'))
    return not bad, short(bad)
ck('json: literal_row verbatim in literal_source; source_sha256 = sha256(literal_row); file digest pinned (all 456)', lit_checks)
ck('json: synthesis-allocated ids carry the spec\'s own row as literal_row', lambda: (not (b := [i for i, c in C.items() if c.get('synthesis_allocated') and (c.get('literal_source') != 'docs/experiments-spec.md' or c.get('literal_row') != SROW.get(i))]), short(b)))
def req_checks():
    bad = []
    for i, c in C.items():
        if c.get('disposition') == 'adopted':
            sc = cells(SROW.get(i, '|||'))
            if len(sc) < 3 or c.get('requirement') != sc[1] or c.get('acceptance') != sc[2]: bad.append(i)
        elif not c.get('requirement') or c['requirement'].split()[0] not in c.get('literal_row', ''): bad.append(i)
    return not bad, short(bad)
ck('json: adopted requirement/acceptance = spec cells; alias requirement drawn from its literal row', req_checks)
def anchor_checks():
    bad = []
    for i, c in C.items():
        if c.get('appendix_a_row') != AROW.get(i) or c.get('appendix_a_row_sha256') != hashlib.sha256((AROW.get(i) or '').encode()).hexdigest(): bad.append((i, 'appendix_a_row'))
        if c.get('ac_hook') != A.get(i, {}).get('hook') or not re.fullmatch(r'AC-\d+(\.\d+)*-\d+', c.get('ac_hook', '')): bad.append((i, 'ac_hook'))
        if c.get('ac_hook_resolution') != 'appendix-a-row': bad.append((i, 'resolution'))
        if c.get('spec_anchor') not in anchors: bad.append((i, 'spec_anchor'))
        t = c.get('target', '')
        if c.get('target_spec_row_sha256') != hashlib.sha256((SROW.get(t) or '').encode()).hexdigest(): bad.append((i, 'target_spec_row_sha256'))
        if c.get('disposition') == 'adopted' and c.get('ac_hook', '\0') not in SROW.get(i, ''): bad.append((i, 'hook not in normative row'))
    return not bad, short(bad)
ck('json: anchors explicit — spec_anchor exists, ac_hook = Appendix A hook (in the normative row), row digests match', anchor_checks)
ck('every adopted id owned exactly once (as its own target)', lambda: (lambda own: (set(own) == adopted and all(v == 1 for v in own.values()), short([k for k, v in own.items() if v != 1] + list(adopted - set(own)))))(Counter(c.get('target') for c in C.values() if c.get('disposition') == 'adopted')))
def owner_rows():
    bad = []
    for i, c in C.items():
        p = c.get('primary'); r = rows.get(p)
        if r is None: bad.append((i, 'primary not a row')); continue
        if c.get('primary_file') != 'docs/tickets/' + r['file'] or c.get('primary_kind') != r['kind']: bad.append((i, 'file/kind'))
        if c.get('acceptance_anchor') != 'ac-' + p.lower().replace('.', '-') + '-' + c.get('target', '').lower(): bad.append((i, 'anchor'))
    return not bad, short(bad)
ck('every owner is a table row; primary_file, primary_kind and acceptance_anchor agree with it', owner_rows)
ck('aliases: target is adopted; alias inherits target owner', lambda: (not (b := [i for i, c in C.items() if c.get('disposition') != 'adopted' and (c.get('target') not in adopted or c.get('primary') != C.get(c.get('target'), {}).get('primary'))]), short(b)))
def rowtable_vs_json():
    js = defaultdict(set)
    for c in C.values():
        if c.get('disposition') == 'adopted': js[c.get('primary')].add(c['id'])
    bad, seen = [], Counter()
    for cid, (n, ids) in rowtable_owned.items():
        if n == -1: bad.append((cid, 'unparseable cell')); continue
        if n != len(ids) and ids: bad.append((cid, f'count {n} != {len(ids)}'))
        for x in ids: seen[x] += 1
        if set(ids) != js.get(cid, set()): bad.append((cid, 'table≠json', short(set(ids) ^ js.get(cid, set()), 4)))
    dup = [x for x, v in seen.items() if v > 1]
    if dup: bad.append(('duplicated in table', short(dup)))
    missing = adopted - set(seen)
    if missing: bad.append(('dropped from table', short(missing)))
    extra = set(js) - set(rowtable_owned)
    if extra: bad.append(('json owner not in table', short(extra)))
    return not bad, short(bad, 4)
ck('row-table owned ids == JSON primaries row for row (no duplicate, no dropped owner)', rowtable_vs_json)
def owner_kind():
    bad = []
    for c in C.values():
        if c.get('disposition') != 'adopted' or c.get('primary') not in rows: continue
        k = rows[c['primary']]['kind']
        if k in ('gate', 'human') and c['primary'] not in ('GATE-G26', 'HUMAN-H16'): bad.append(c['id'])
        if k == 'skeleton' and c.get('layer') not in ('R', 'P'): bad.append(c['id'])
    return not bad, short(bad)
ck('owner-kind rule: gate/human only literal gate/HUMAN obligations; skeleton only R/P window evidence', owner_kind)
CLOSURE = {'REQ-XH-015', 'REQ-XH-021', 'REQ-XH-029', 'REQ-XH-106', 'REQ-XH-110'}
ck('routed ids: owned by CAP6.2 (closure capstone, before GATE-ACCEPT) only; CAP6.2 first-pass ids = the five closure ids; CAP6.1 owns none', lambda: all(
    (c.get('round_scope', '').startswith('routed') and c.get('primary') == 'CAP6.2') or
    (c.get('round_scope') == 'first-pass' and (c.get('primary') != 'CAP6.2' or c['id'] in CLOSURE) and c.get('primary') != 'CAP6.1')
    for c in C.values() if c.get('disposition') == 'adopted'))
ck('contributors are table rows, never the primary', lambda: (not (b := [(i, x) for i, c in C.items() for x in c.get('contributors', []) if x not in rows or x == c.get('primary')]), short(b)))

# ---------- 4. coverage md agrees with json ----------
def md_owner():
    m = re.search(r'## Literal owner table\s*(.*?)(\n## |\Z)', MD, re.S)
    md = {}
    for l in (m.group(1) if m else '').split('\n'):
        c = cells(l) if l.startswith('|') else []
        if len(c) >= 7 and re.match(r'REQ-X[A-H]-\d+$', c[0]):
            md[c[0]] = re.findall(r'\(([^)]*tickets/[^)#]+?\.md)#([^)]+)\)', c[2])
    bad = [i for i in A if i not in md or len(md[i]) != 1 or os.path.basename(md[i][0][0]) != os.path.basename(C.get(i, {}).get('primary_file', '')) or md[i][0][1] != C.get(i, {}).get('acceptance_anchor')]
    return len(md) == 456 and not bad, f'{len(md)} ids; bad {short(bad)}'
ck('md literal owner table: all 456 ids, one contract + anchor each, equal to json', md_owner)
def md_index():
    m = re.search(r'## Owner index[^\n]*\n(.*?)(\n## |\Z)', MD, re.S)
    got = {}
    for l in (m.group(1) if m else '').split('\n'):
        c = cells(l) if l.startswith('|') else []
        mm = re.match(r'\[(' + CID + r')\]', c[0]) if c else None
        if mm:
            body = re.sub(r'^\d+: (routed: )?', '', c[3])
            got[mm.group(1)] = {'REQ-' + x.strip() for x in body.split(',')}
    js = defaultdict(set)
    for c in C.values():
        if c.get('disposition') == 'adopted': js[c.get('primary')].add(c['id'])
    bad = [k for k in set(got) | set(js) if got.get(k) != js.get(k)]
    return not bad, short(bad)
ck('md owner index agrees with json row for row', md_index)

# ---------- 5. shared-decision ownership: owner row precedes every consumer row ----------
def shared():
    m = re.search(r'### Shared-decision ownership[^\n]*\n(.*?)(\n### |\Z)', PLAN, re.S)
    bad, n = [], 0
    for l in (m.group(1) if m else '').split('\n'):
        c = cells(l) if l.startswith('|') else []
        if len(c) != 3 or c[0] in ('shared decision / seam', '---') or set(c[0]) <= {'-'}: continue
        own = re.findall(CID, c[1])
        if not own: continue
        n += 1
        o = own[0]
        if o not in rows: bad.append((c[0][:30], 'owner not a row')); continue
        for x in re.findall(CID, c[2]):
            if x in rows and x != o and rows[x]['seq'] <= rows[o]['seq']: bad.append((o, x))
    return n >= 20 and not bad, f'{n} decisions; {short(bad)}'
ck('shared-decision table: each owner is a row that precedes every consumer row it names', shared)

ok = all(o for _, o, _ in res)
for n, o, d in res:
    print(('PASS' if o else 'FAIL') + '  ' + n + (f'  [{d}]' if (d and not o) else ''))
print(f"RESULT: {'PASS' if ok else 'FAIL'} ({sum(o for _, o, _ in res)}/{len(res)})")
sys.exit(0 if ok else 1)
