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
# S2.2 (§8.4 removability tiers): hh-varhost, hh-plugin-fixture and
# hh-compact-evict-oldest are the extension tier — removable without
# breaking the base class contracts. The check: no *other* workspace crate
# names them as a normal dependency (dev-dependency edges don't ship).
EXT="hh-varhost hh-plugin-fixture hh-compact-evict-oldest"
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
# `tier-c4` feature is the single consumer (an *optional* dependency);
# absent ⇒ the `fleet.*` ops stay in the schema (CC7 single source) and
# answer `Unsupported{by: "tier-c4"}` — a typed refusal, never silent
# degrade. Lower tiers are untouched: hh-fleet consumes hh-ledger /
# hh-budget / hh-monitor / hh-embed-schema and nothing below C4 depends
# on hh-fleet.
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
        if name == 'hh-fleet' and d['name'] == 'hh-embed':
            bad.append('hh-fleet -> hh-embed (C4 depends on its consumer)')
        if d['name'] == 'hh-fleet' and name != 'hh-fleet':
            consumers.append((name, 'optional' if d.get('optional') else 'REQUIRED'))
for name, kind in consumers:
    if name != 'hh-embed':
        bad.append(name + ' -> hh-fleet (consumer outside the boundary)')
    elif kind == 'REQUIRED':
        bad.append('hh-embed -> hh-fleet must be OPTIONAL (tier-c4 feature)')
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
print('hh-fleet is edge-free upward; hh-embed consumes it behind tier-c4 only')
"

echo "check-removability: S4.9 fleet boundary verified"

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
