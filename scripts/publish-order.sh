#!/usr/bin/env bash
# publish-order.sh — L4.5's publish-set derivation, topological ordering and
# publish dry-run (the 2026-10-20 "Libraries + installable bins" ruling as
# applied per crate by ADR-0380 (a)).
#
# Phases:
#   1. Derive the publish set from `cargo metadata --no-deps`: every workspace
#      member except the ones carrying `publish = false`. The `publish = false`
#      set must equal ADR-0380 (a)'s recorded set exactly — drift in either
#      direction fails loudly (a library never silently drops out of the set,
#      and a fixture never silently enters it).
#   2. Topological publish order over intra-workspace normal+build dependency
#      edges (dev-dependencies excluded — `cargo publish` strips path-only
#      dev-deps and they form false cycles, e.g. varhost ⇄ plugin-fixture).
#      Kahn's algorithm, name-sorted ties: deterministic. A cycle fails.
#   3. Run-time crates.io availability re-check over the final publish set:
#      one `GET https://crates.io/api/v1/crates/<name>` per name, ≥ 1 s apart,
#      descriptive User-Agent, dated per-name output (404 = free, 200 = taken,
#      anything else = inconclusive). ANY taken or inconclusive name is a
#      STOP-and-ask to the operator — exit 3, never a silent rename and never
#      a quiet drop from the publish set.
#   4. `cargo publish --workspace --dry-run`: cargo packages every publishable
#      member in dependency order against the `target/package/tmp-registry`
#      overlay so in-flight siblings satisfy each other's verification.
#      (Must run on a committed tree — `cargo publish` refuses a dirty one;
#      pass --allow-dirty explicitly if you really mean a scratch check.)
#
# Usage:
#   publish-order.sh [--root DIR] [--check-only] [--no-availability-check]
#     --check-only            phases 1–3 only (set + order + availability)
#     --no-availability-check skip phase 3 (documented re-runs/tests only —
#                             the default path always re-checks crates.io)
#     --no-dry-run            stop after phase 3 (order + availability only)
# Env:
#   HH_PUBLISH_STUB_FILE — when set, phase 3 reads `name<TAB>http-status`
#     lines from this file instead of the network (the self-test's stubbed-
#     answers seam; still emits dated per-name lines).
#   HH_PUBLISH_RECORDED_NOPUBLISH — when set, overrides the recorded
#     ADR-0380 publish=false set (the self-test's fixture seam; never set
#     against the real workspace).
#   HH_PUBLISH_EXTRA_ARGS — extra args forwarded to `cargo publish` (e.g.
#     "--allow-dirty" for a scratch dry-run).
# Exit: 0 ok · 2 usage/tool/drift error · 3 crates.io name taken or
# inconclusive — STOP-and-ask.
set -euo pipefail

ROOT=""
CHECK_ONLY=0
NO_CHECK=0
NO_DRYRUN=0
while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT="$2"; shift 2 ;;
    --check-only) CHECK_ONLY=1; shift ;;
    --no-availability-check) NO_CHECK=1; shift ;;
    --no-dry-run) NO_DRYRUN=1; shift ;;
    -h|--help) sed -n '1,45p' "$0"; exit 0 ;;
    *) echo "publish-order: unknown arg $1" >&2; exit 2 ;;
  esac
done
if [ -z "$ROOT" ]; then
  ROOT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
fi
[ -n "$ROOT" ] || { echo "publish-order: no --root and not inside a git work tree" >&2; exit 2; }
cd "$ROOT"
command -v cargo   >/dev/null || { echo "publish-order: cargo absent" >&2; exit 2; }
command -v python3 >/dev/null || { echo "publish-order: python3 absent" >&2; exit 2; }
command -v curl    >/dev/null || { echo "publish-order: curl absent" >&2; exit 2; }

# ── Phase 1+2: derive the publish set and the topological order ───────────────
META_FILE="$(mktemp "${TMPDIR:-/tmp}/hh-publish-meta.XXXXXX")"
DERIVE_FILE="$(mktemp "${TMPDIR:-/tmp}/hh-publish-derive.XXXXXX")"
trap 'rm -f "$META_FILE" "$DERIVE_FILE"' EXIT
cargo metadata --no-deps --format-version 1 --manifest-path "$ROOT/Cargo.toml" --offline >"$META_FILE" 2>/dev/null \
  || cargo metadata --no-deps --format-version 1 --manifest-path "$ROOT/Cargo.toml" >"$META_FILE" 2>/dev/null \
  || { echo "publish-order: cargo metadata failed" >&2; exit 2; }
python3 - "$META_FILE" > "$DERIVE_FILE" <<'PY'
import json, sys

# ADR-0380 (a)'s recorded publish=false set — fixtures, packaged plugin
# variants, test-support. The metadata publish flags must match exactly.
# HH_PUBLISH_RECORDED_NOPUBLISH is the self-test's fixture seam.
import os
_recorded = os.environ.get("HH_PUBLISH_RECORDED_NOPUBLISH")
RECORDED_NOPUBLISH = (set(_recorded.split()) if _recorded else {
    "hharness-plugin-fixture",
    "hharness-tracker-fixture",
    "hharness-compact-evict-oldest",
    "hharness-memory-store",
    "hharness-github-issues",
})

m = json.load(open(sys.argv[1]))
ws = set(m["workspace_members"])
pkgs = [p for p in m["packages"] if p["id"] in ws]
names = {p["name"] for p in pkgs}
nopub = {p["name"] for p in pkgs if p.get("publish") in ([], False)}
pub = names - nopub

drift = []
for n in sorted(nopub - RECORDED_NOPUBLISH):
    drift.append(f"{n} carries publish = false but is not in ADR-0380 (a)'s recorded set")
for n in sorted(RECORDED_NOPUBLISH - nopub):
    drift.append(f"{n} is recorded publish = false by ADR-0380 (a) but the flag is absent")
if drift:
    print("publish-order: FAIL — publish-set drift vs ADR-0380 (a):", file=sys.stderr)
    for d in drift:
        print("  " + d, file=sys.stderr)
    sys.exit(2)

# Topological order over intra-workspace normal+build edges only — dev-deps
# are stripped at package time and would create false cycles.
edges = {}  # dep -> {dependents}; indeg via normal/build deps within pub set
indeg = {n: 0 for n in pub}
adj = {n: set() for n in pub}
for p in pkgs:
    if p["name"] not in pub:
        continue
    for d in p["dependencies"]:
        tgt = d.get("name")
        if d.get("kind") == "dev" or tgt not in pub or not d.get("path"):
            continue
        if p["name"] not in adj[tgt]:
            adj[tgt].add(p["name"])
            indeg[p["name"]] += 1
import heapq
ready = sorted(n for n in pub if indeg[n] == 0)
order = []
heap = list(ready); heapq.heapify(heap)
while heap:
    n = heapq.heappop(heap)
    order.append(n)
    for dep in sorted(adj[n]):
        indeg[dep] -= 1
        if indeg[dep] == 0:
            heapq.heappush(heap, dep)
if len(order) != len(pub):
    cyc = sorted(n for n in pub if n not in order)
    print("publish-order: FAIL — dependency cycle among publishable crates: "
          + ", ".join(cyc), file=sys.stderr)
    sys.exit(2)
print(json.dumps({"publish": order, "nopublish": sorted(nopub)}))
PY
[ -s "$DERIVE_FILE" ] || { echo "publish-order: derivation produced nothing" >&2; exit 2; }

N_PUB="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["publish"]))' "$DERIVE_FILE")"
N_NOPUB="$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["nopublish"]))' "$DERIVE_FILE")"
echo "publish-order: $N_PUB publishable crates, $N_NOPUB publish=false (ADR-0380 (a) set)"
echo "publish-order: topological publish order:"
python3 -c 'import json,sys
o=json.load(open(sys.argv[1]))["publish"]
for i,n in enumerate(o,1): print(f"  {i:2}. {n}")' "$DERIVE_FILE"

# ── Phase 3: run-time crates.io availability re-check ─────────────────────────
if [ "$NO_CHECK" -eq 0 ]; then
  TOTAL="$N_PUB"
  echo "publish-order: crates.io availability re-check — $TOTAL names ($(date -u +%FT%TZ))"
  FAIL=""
  i=0
  # Iterate via a tempfile — a `while` in a pipe subshell cannot set FAIL.
  NAMES_FILE="$(mktemp "${TMPDIR:-/tmp}/hh-publish-names.XXXXXX")"
  trap 'rm -f "$META_FILE" "$DERIVE_FILE" "$NAMES_FILE"' EXIT
  python3 -c 'import json,sys
print("\n".join(sorted(json.load(open(sys.argv[1]))["publish"])))' "$DERIVE_FILE" > "$NAMES_FILE"
  while IFS= read -r name; do
    [ -n "$name" ] || continue
    i=$((i + 1))
    ts="$(date -u +%FT%TZ)"
    if [ -n "${HH_PUBLISH_STUB_FILE:-}" ]; then
      code="$(awk -F'\t' -v n="$name" '$1==n{print $2}' "$HH_PUBLISH_STUB_FILE" | head -1)"
      [ -n "$code" ] || code="stub-missing"
    else
      code="$(curl -sS -o /dev/null -w '%{http_code}' \
        -A 'HarnessHarness-publish-order (L4.5 scripted availability re-check; https://github.com/SteveVitali/MetaHarness)' \
        "https://crates.io/api/v1/crates/$name" 2>/dev/null || echo curl-error)"
      # ≥ 1 s between requests (crates.io API courtesy).
      [ "$i" -lt "$TOTAL" ] && sleep 1.2
    fi
    case "$code" in
      404) verdict=free ;;
      200) verdict=TAKEN; FAIL="$FAIL $name(200)" ;;
      *)   verdict=inconclusive; FAIL="$FAIL $name($code)" ;;
    esac
    printf '%s  %-38s  %-12s  %s\n' "$ts" "$name" "$code" "$verdict"
  done < "$NAMES_FILE"
  if [ -n "$FAIL" ]; then
    {
      echo "publish-order: STOP — crates.io names taken or inconclusive:$FAIL"
      echo "  A run-time collision is a STOP-and-ask to the operator (L4.5 contract):"
      echo "  never a silent rename, never a quiet drop from the publish set."
    } >&2
    exit 3
  fi
  echo "publish-order: all $TOTAL publish-set names free on crates.io."
fi
[ "$CHECK_ONLY" -eq 1 ] && exit 0
[ "$NO_DRYRUN" -eq 1 ] && exit 0

# ── Phase 4: the publish dry-run over the whole ordered set ───────────────────
echo "publish-order: cargo publish --workspace --dry-run over the $N_PUB-crate set"
# shellcheck disable=SC2086
cargo publish --workspace --dry-run ${HH_PUBLISH_EXTRA_ARGS:-}
echo "publish-order: dry-run green — the $N_PUB-crate publish set packages in order."
