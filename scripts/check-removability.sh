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
        if d['name'] in ext and d['kind'] == 'normal':
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
        if d['name'] == 'hh-hosting' and d['kind'] == 'normal':
            bad.append(pkg['name'] + ' -> ' + d['name'])
if bad:
    print('hosting-tier edges into the base:')
    for b in bad: print('  ' + b)
    sys.exit(1)
print('hosting tier is edge-free (hosting_edges = [], removable)')
"

echo "check-removability: hosting tier verified removable"
