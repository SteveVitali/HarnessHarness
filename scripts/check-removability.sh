#!/usr/bin/env bash
# check-removability.sh — the CC6 removability gate for the helper boundary
# (S2.1; R-2.5.5¹; ticket 032's "removability(0…3)" row).
#
# Tiers map to cargo features: `tier-c1` is the only tiered feature at S2.1
# (durable executor-side dedup + preserve_until). removability(0) = the
# workspace with tier-c1 absent still builds, refuses the C1 halves honestly
# (`unsupported`, never silent degrade), and passes the tier-0 acceptance
# surface unchanged. Tiers 1–3 have no feature surface yet — the check is
# the honest gate: each absent tier is rebuilt+refusal-verified the moment
# its feature lands.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== removability(0): hh-helper without tier-c1 =="
cargo build -p hh-helper --no-default-features
HH_REM0=1 HH_HELPER_BIN="$PWD/target/debug/hh-helper" \
    cargo test -p hh-env --test helper_live ac_s2_removability0 -- --nocapture

echo "== tier-c1 restore + the unchanged tier-1 suite =="
cargo build -p hh-helper
cargo test -p hh-env --test helper_live

echo "check-removability: OK (tier-0 honest refusals verified; tier-1 suite green)"

echo "== removability(0): the S2.3 C1 surface (hh-ledger + hh-env) =="
# S2.3 (§5a.3 C1): scoped leases, `suspend`, wakeup subscriptions and
# `compensate_run` are `tier-c1` on hh-ledger; `HealingPolicy` healing is
# `tier-c1` on hh-env (which forwards the feature). Absent => typed
# `UnsupportedTier{tier:"c1"}` / `Unsupported{heal}` refusals (CC6).
cargo build -p hh-ledger --no-default-features
cargo build -p hh-env --no-default-features
cargo test -p hh-ledger --no-default-features --test durable_execution removability0

echo "== removability(extension): no base crate depends on the variant host =="
# S2.2 (§8.4 removability tiers): hh-varhost, hh-plugin-fixture and the
# packaged plugin binaries (hh-compact-evict-oldest; S4.16b's hh-memory-store;
# S5.6's hh-tracker-fixture — the packaged `work_source_adapter` reference
# variant) are the extension tier — removable without breaking the base class
# contracts. The check: no *other* workspace crate names them as a normal
# dependency (dev-dependency edges don't ship).
EXT="hh-varhost hh-plugin-fixture hh-compact-evict-oldest hh-memory-store hh-tracker-fixture"
cargo metadata --format-version 1 --no-deps | python3 -c "
import json,sys
ext=set(sys.argv[1].split())
meta=json.load(sys.stdin)
bad=[]
for pkg in meta['packages']:
    if pkg['name'] in ext:
        continue
    for d in pkg['dependencies']:
        if d['name'] in ext and d.get('kind') in (None, 'normal'):
            bad.append(pkg['name'] + ' -> ' + d['name'])
if bad:
    print('extension-tier edges into the base:')
    for b in bad: print('  ' + b)
    sys.exit(1)
print('extension tier is edge-free into the base (removable)')
" "$EXT"

echo "check-removability: extension tier verified removable"

echo "== removability(hosting): no crate depends on hh-hosting =="
# S3.4d (§6.6; AC-R-2.10.6-5): the Hosting ABI schema crate is a removable
# tier — `hosting_edges = []`: no HIR entity, C0 contract or other workspace
# crate names hh-hosting as a normal dependency (the projection's class data
# lives in hh-ledger's `hosted_lowering` column — data, not a dependency —
# so removing the crate removes the whole tier; T-LCD-06).
cargo metadata --format-version 1 --no-deps | python3 -c "
import json,sys
meta=json.load(sys.stdin)
bad=[]
for pkg in meta['packages']:
    if pkg['name'] == 'hh-hosting':
        continue
    for d in pkg['dependencies']:
        if d['name'] == 'hh-hosting' and d.get('kind') in (None, 'normal'):
            bad.append(pkg['name'] + ' -> ' + d['name'])
if bad:
    print('hosting-tier edges into the base:')
    for b in bad: print('  ' + b)
    sys.exit(1)
print('hosting tier is edge-free (hosting_edges = [], removable)')
"

echo "check-removability: hosting tier verified removable"

echo "== removability(S4.6): C1 subagent slice independent; C3 is its only consumer =="
# §5e.3 CC6 note: the C1 `spawn` slice must survive removability(1); the
# C3 orchestrator is the removable consumer. `cargo metadata` reports a
# normal dependency's `kind` as null — accept both null and "normal" here.
cargo build -p hh-subagent
cargo test -p hh-subagent
cargo metadata --format-version 1 --no-deps | python3 -c "
import json,sys
meta=json.load(sys.stdin)
normal=lambda d: d.get('kind') in (None,'normal')
bad=[]
consumers=[]
for pkg in meta['packages']:
    name=pkg['name']
    for d in pkg['dependencies']:
        if not normal(d):
            continue
        if name == 'hh-subagent' and d['name'] == 'hh-orchestrator':
            bad.append('hh-subagent -> hh-orchestrator (C1 depends on C3)')
        if d['name'] == 'hh-subagent' and name != 'hh-subagent':
            consumers.append(name)
        if d['name'] == 'hh-orchestrator' and name != 'hh-orchestrator':
            bad.append(name + ' -> hh-orchestrator (C3 is not removable)')
extra=sorted(set(consumers)-{'hh-orchestrator'})
for name in extra:
    bad.append(name + ' -> hh-subagent (C1 consumer outside C3)')
if bad:
    print('subagent removability violations:')
    for b in bad: print('  ' + b)
    sys.exit(1)
print('hh-subagent is edge-free upward; hh-orchestrator is its only consumer')
"

echo "check-removability: S4.6 subagent/orchestrator boundary verified"

echo "== removability(S4.9): C4 fleet tier — hh-fleet optional; hh-embed is its only consumer =="
# §5i.1 CC6: the organizational layer is a removable tier. `hh-embed`'s
# `tier-c4` feature and S5.6's `hh-fleet-adapter` (the L5 plugin boundary —
# itself C4-tier and leaf-consumed only by the packaged fixture binary) are
# the consumers (the embed edge an *optional* dependency); absent ⇒ the
# `fleet.*` ops stay in the schema (CC7 single source) and answer
# `Unsupported{by: "tier-c4"}` — a typed refusal, never silent degrade.
# Lower tiers are untouched: hh-fleet consumes hh-ledger / hh-budget /
# hh-monitor / hh-embed-schema and nothing below C4 depends on hh-fleet.
cargo build -p hh-embed --no-default-features
cargo build -p hh-embed
cargo build -p hh-fleet
cargo metadata --format-version 1 --no-deps | python3 -c "
import json,sys
meta=json.load(sys.stdin)
normal=lambda d: d.get('kind') in (None,'normal')
bad=[]
consumers=[]
for pkg in meta['packages']:
    name=pkg['name']
    for d in pkg['dependencies']:
        if not normal(d):
            continue  # dev-deps don't ship (hh-varhost's fixture test consumes hh-fleet dev-only)
        if name == 'hh-fleet' and d['name'] == 'hh-embed':
            bad.append('hh-fleet -> hh-embed (C4 depends on its consumer)')
        if d['name'] == 'hh-fleet' and name != 'hh-fleet':
            consumers.append((name, 'optional' if d.get('optional') else 'REQUIRED'))
for name, kind in consumers:
    if name == 'hh-embed' and kind == 'REQUIRED':
        bad.append('hh-embed -> hh-fleet must be OPTIONAL (tier-c4 feature)')
    elif name not in ('hh-embed', 'hh-fleet-adapter'):
        bad.append(name + ' -> hh-fleet (consumer outside the C4 boundary)')
# hh-fleet-adapter is the C4 L5 boundary — leaf-consumed only by the
# packaged fixture binary (itself extension-tier). Anything else depending
# on it is an upward edge.
for pkg in meta['packages']:
    name = pkg['name']
    for d in pkg['dependencies']:
        if not normal(d):
            continue
        if d['name'] == 'hh-fleet-adapter' and name not in ('hh-fleet-adapter', 'hh-tracker-fixture'):
            bad.append(name + ' -> hh-fleet-adapter (consumer outside the C4/extension tier)')
for pkg in meta['packages']:
    if pkg['name'] != 'hh-embed':
        continue
    feats = pkg.get('features', {})
    if 'tier-c4' not in feats:
        bad.append('hh-embed missing the tier-c4 feature gate')
if bad:
    print('fleet removability violations:')
    for b in bad: print('  ' + b)
    sys.exit(1)
print('hh-fleet is edge-free upward; hh-embed (optional) + hh-fleet-adapter are its only consumers')
"

echo "check-removability: S4.9 fleet boundary verified"

echo "== removability(S6.1a): C4 evolution tier — hh-evolution optional; hh-embed is its only consumer =="
# §05h CC6: the evolution pipeline is a removable C4 slice. `hh-embed`'s
# `tier-c4` feature is the only consumer (an *optional* dependency);
# absent ⇒ the `lab.evolution.*` ops stay in the schema (CC7 single
# source) and answer `Unsupported{by: "tier-c4"}` — a typed refusal,
# never silent degrade (the `cargo build -p hh-embed
# --no-default-features` leg above already proves the absent-tier
# build). hh-evolution consumes only kernel-side crates — never a
# surface, never its consumer.
cargo metadata --format-version 1 --no-deps | python3 -c "
import json,sys
meta=json.load(sys.stdin)
normal=lambda d: d.get('kind') in (None,'normal')
bad=[]
consumers=[]
allowed_deps={
    'hh-wire','hh-identity','hh-ontology','hh-provenance','hh-hir',
    'hh-ledger','hh-budget','hh-lab','hh-experiment',
    # S6.2 — hh-registry is kernel-side; the proposer conformance
    # driver reports the registry's ConformanceVerdict (CC7 — the
    # verdict sum has one owner).
    'hh-registry',
}
for pkg in meta['packages']:
    name=pkg['name']
    for d in pkg['dependencies']:
        if not normal(d):
            continue
        if name == 'hh-evolution':
            if d['name'] == 'hh-embed':
                bad.append('hh-evolution -> hh-embed (C4 depends on its consumer)')
            if d['name'] not in allowed_deps:
                bad.append('hh-evolution -> ' + d['name'] + ' (non-kernel dependency)')
        if d['name'] == 'hh-evolution' and name != 'hh-evolution':
            consumers.append((name, 'optional' if d.get('optional') else 'REQUIRED'))
for name, kind in consumers:
    if name != 'hh-embed':
        bad.append(name + ' -> hh-evolution (consumer outside the C4 boundary)')
    elif kind == 'REQUIRED':
        bad.append('hh-embed -> hh-evolution must be OPTIONAL (tier-c4 feature)')
if bad:
    print('evolution removability violations:')
    for b in bad: print('  ' + b)
    sys.exit(1)
print('hh-evolution is edge-free upward; hh-embed (optional) is its only consumer')
"

echo "check-removability: S6.1a evolution boundary verified"

echo "== removability(S6.1b): C4 debt-manager tier — hh-debt optional; hh-embed is its only consumer =="
# §5h.6 CC6: the assumption-debt manager is a removable C4 slice.
# `hh-embed`'s `tier-c4` feature is the only consumer (an *optional*
# dependency); absent ⇒ the `lab.debt.{register,sweep,settle,retire,
# propose,manager_open}` ops stay in the schema (CC7) and answer
# `Unsupported{by: "tier-c4"}` — a typed refusal, never silent degrade.
# hh-debt consumes only kernel-side crates — never a surface, never its
# consumer; it holds no authority handle over subject runs (D-2: it
# authors specs and proposals; `commit_kernel_row_for` is the one write
# path).
cargo metadata --format-version 1 --no-deps | python3 -c "
import json,sys
meta=json.load(sys.stdin)
normal=lambda d: d.get('kind') in (None,'normal')
bad=[]
consumers=[]
allowed_deps={
    'hh-wire','hh-identity','hh-ontology','hh-provenance','hh-hir',
    'hh-ledger','hh-budget','hh-lab','hh-experiment','hh-compiler',
}
for pkg in meta['packages']:
    name=pkg['name']
    for d in pkg['dependencies']:
        if not normal(d):
            continue
        if name == 'hh-debt':
            if d['name'] == 'hh-embed':
                bad.append('hh-debt -> hh-embed (C4 depends on its consumer)')
            if d['name'] not in allowed_deps:
                bad.append('hh-debt -> ' + d['name'] + ' (non-kernel dependency)')
        if d['name'] == 'hh-debt' and name != 'hh-debt':
            consumers.append((name, 'optional' if d.get('optional') else 'REQUIRED'))
for name, kind in consumers:
    if name != 'hh-embed':
        bad.append(name + ' -> hh-debt (consumer outside the C4 boundary)')
    elif kind == 'REQUIRED':
        bad.append('hh-embed -> hh-debt must be OPTIONAL (tier-c4 feature)')
if bad:
    print('debt-manager removability violations:')
    for b in bad: print('  ' + b)
    sys.exit(1)
print('hh-debt is edge-free upward; hh-embed (optional) is its only consumer')
"

echo "check-removability: S6.1b debt-manager boundary verified"

echo "== removability(S6.4): co-evolution interface — hh-embed tier-c4 ops; absent tier answers Unsupported =="
# §5h.8 CC6: the co-evolution interface adds no new crate — the records
# and drivers live inside kernel-side hh-lab and the already-verified C4
# slices (hh-evolution's consolidation/cycle, hh-debt's post-import
# sweep, hh-fleet's org-policy defaults). The surface is hh-embed's
# `coevolution_ops`, gated by `tier-c4`: absent ⇒ the
# `lab.coevolution.*` / `lab.org_policy.*` / `lab.debt.post_import_sweep`
# ops stay in the schema (CC7 single source) and answer
# `Unsupported{by: "tier-c4"}` — a typed refusal, never silent degrade.
# The no-default-features test leg exercises the honest refusal; the
# schema grep proves the ops remain declared regardless of the tier.
cargo test -p hh-embed --no-default-features --test s6_4
python3 - <<'EOF'
import json, sys
schema = json.load(open('schema/hh-embed-1.schema.json'))
ops = set()
def walk(node):
    if isinstance(node, dict):
        for k, v in node.items():
            if k == 'methods' and isinstance(v, dict):
                ops.update(v.keys())
            else:
                walk(v)
    elif isinstance(node, list):
        for v in node:
            walk(v)
walk(schema)
required = {
    'lab.coevolution.export_training',
    'lab.coevolution.export_regression_suite',
    'lab.coevolution.export_compatibility_tags',
    'lab.coevolution.import_snapshot',
    'lab.coevolution.guard_at_bind',
    'lab.coevolution.consolidation_candidates',
    'lab.coevolution.propose_consolidation',
    'lab.coevolution.consolidation_retirement_record',
    'lab.coevolution.cycle_open',
    'lab.coevolution.cycle_begin_phase',
    'lab.coevolution.cycle_complete_phase',
    'lab.coevolution.cycle_next',
    'lab.coevolution.cycle_stop',
    'lab.coevolution.cycle_record',
    'lab.org_policy.recipe',
    'lab.org_policy.default_removal_tests',
    'lab.debt.post_import_sweep',
}
missing = required - ops
if missing:
    print('coevolution ops missing from the single schema source:')
    for m in sorted(missing):
        print('  ' + m)
    sys.exit(1)
print('the coevolution ops stay in the schema (CC7); the absent tier answers Unsupported{by: "tier-c4"}')
EOF

echo "check-removability: S6.4 co-evolution boundary verified"

echo "== removability(S4.11): C1 MCP-server surface — hh-mcp-lab is a leaf consumer =="
# §7.3 CC6: the surface server is a removable slice — nothing below it
# depends on it. Removing the feature = removing the crate: no workspace
# package names `hh-mcp-lab` as a normal dependency (dev-deps don't ship),
# and hh-mcp-lab itself consumes only the kernel-side crates (hh-embed,
# hh-mcp, hh-ledger, hh-embed-schema, …) — never the reverse. The schema
# (CC7 single source) is unchanged by the surface's absence: `hh-embed/1`
# ops stay declared regardless of who fronts them.
cargo metadata --format-version 1 --no-deps | python3 -c "
import json,sys
meta=json.load(sys.argv[1]) if len(sys.argv)>1 else json.load(sys.stdin)
normal=lambda d: d.get('kind') in (None,'normal')
bad=[]
for pkg in meta['packages']:
    name=pkg['name']
    for d in pkg['dependencies']:
        if d['name'] == 'hh-mcp-lab' and name != 'hh-mcp-lab' and normal(d):
            bad.append(name + ' -> hh-mcp-lab (a kernel/base crate may not consume the surface)')
        if name == 'hh-mcp-lab' and d['name'] == 'hh-mcp-lab':
            bad.append('hh-mcp-lab self-edge')
# The surface may only sit ABOVE the kernel boundary — check it never
# reaches into another surface/extension tier.
allowed_deps = {
    'hh-wire','hh-identity','hh-ledger','hh-embed','hh-embed-schema',
    'hh-mcp','hh-env','hh-budget','hh-telemetry','hh-ontology',
    'hh-provenance','hh-hir','hh-assembly',
}
for pkg in meta['packages']:
    if pkg['name'] != 'hh-mcp-lab':
        continue
    for d in pkg['dependencies']:
        if normal(d) and d['name'] not in allowed_deps:
            bad.append('hh-mcp-lab -> ' + d['name'] + ' (surface depends on a non-kernel crate)')
if bad:
    print('mcp-lab removability violations:')
    for b in bad: print('  ' + b)
    sys.exit(1)
print('hh-mcp-lab is edge-free upward; removing the crate removes the feature')
"

echo "check-removability: S4.11 mcp-lab surface boundary verified"


echo "== removability(S4.12): C1 embedding/SDK surface — hh-cli + hh-acp are leaf consumers =="
# §7.1/§7.4 CC6: the CLI + ACP surface family is a removable slice — the
# only normal consumers of `hh-acp`/`hh-cli` are the surfaces themselves
# (dev-deps don't ship); both consume only kernel-side crates and drive
# `hh-embed/1` through the declared boundary, never the reverse. The
# schema (CC7 single source) is unchanged by the surfaces' absence:
# `hh-embed/1` ops stay declared regardless of who fronts them.
cargo metadata --format-version 1 --no-deps | python3 -c "
import json,sys
meta=json.load(sys.argv[1]) if len(sys.argv)>1 else json.load(sys.stdin)
normal=lambda d: d.get('kind') in (None,'normal')
bad=[]
surfaces={'hh-acp','hh-cli'}
for pkg in meta['packages']:
    name=pkg['name']
    for d in pkg['dependencies']:
        if not normal(d):
            continue
        if d['name'] in surfaces and name not in surfaces:
            bad.append(name + ' -> ' + d['name'] + ' (a kernel/base crate may not consume the surface)')
        if name in surfaces and d['name'] == name:
            bad.append(name + ' self-edge')
# The surfaces may only sit ABOVE the kernel boundary — they never reach
# into another surface/extension tier crate.
allowed_deps = {
    'hh-wire','hh-identity','hh-ledger','hh-embed','hh-embed-schema',
    'hh-embed-client-generated','hh-mcp','hh-env','hh-budget','hh-telemetry',
    'hh-ontology','hh-provenance','hh-hir','hh-assembly','hh-compiler',
    'hh-bundle','hh-registry','hh-acp',
}
for pkg in meta['packages']:
    if pkg['name'] not in surfaces:
        continue
    for d in pkg['dependencies']:
        if normal(d) and d['name'] not in allowed_deps:
            bad.append(pkg['name'] + ' -> ' + d['name'] + ' (surface depends on a non-kernel crate)')
if bad:
    print('cli/acp removability violations:')
    for b in bad: print('  ' + b)
    sys.exit(1)
print('hh-cli + hh-acp are edge-free upward; removing the pair removes the feature')
"

echo "check-removability: S4.12 cli/acp surface boundary verified"
