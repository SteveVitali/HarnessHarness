#!/usr/bin/env bash
# test-publish-order.sh — L4.5's self-test for scripts/publish-order.sh,
# registered in ci.yml's "CI scripts self-test" step (ci-mode: both).
#
# Builds a throwaway fixture workspace (two libs with a dep edge, one
# `publish = false` member, one umbrella consumer) and drives
# publish-order.sh at it with HH_PUBLISH_STUB_FILE answers +
# HH_PUBLISH_RECORDED_NOPUBLISH pointing at the fixture's flag set:
#   a) all 404 + --check-only → exit 0; the printed order respects the dep
#      edge (a before b before umbrella); publish=false member excluded
#   b) one 200 (taken)         → exit 3, per-name line names the collision
#   c) one 503 (inconclusive)  → exit 3 — inconclusive stops too
#   d) metadata publish=false ≠ the recorded set → exit 2 (drift fails)
# Offline-only — the stub replaces the crates.io leg entirely; --check-only
# never reaches the `cargo publish` phase.
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
SCRIPT="$ROOT/scripts/publish-order.sh"
FAIL=0
say() { printf 'test-publish-order: %s\n' "$*"; }
fail() { printf 'test-publish-order: FAIL — %s\n' "$*" >&2; FAIL=1; }

FIX="$(mktemp -d "${TMPDIR:-/tmp}/publish-fix.XXXXXX")"
trap 'rm -rf "$FIX"' EXIT
mkdir -p "$FIX/crates/a/src" "$FIX/crates/b/src" "$FIX/crates/fix/src" "$FIX/crates/umb/src"

cat > "$FIX/Cargo.toml" <<'EOF'
[workspace]
resolver = "2"
members = ["crates/a", "crates/b", "crates/fix", "crates/umb"]
EOF

mk() { # mk <crate> <name> [extra...]
  cat > "$FIX/crates/$1/Cargo.toml" <<EOF
[package]
name = "$2"
version = "0.1.0"
edition = "2021"
description = "fixture"
$3
EOF
  echo '//! fixture' > "$FIX/crates/$1/src/lib.rs"
}
mk a   fix-a ""
mk b   fix-b "
[dependencies]
fix-a = { path = \"../a\", version = \"0.1.0\" }"
mk fix fix-fixture "publish = false"
mk umb fix-umb "
[dependencies]
fix-b = { path = \"../b\", version = \"0.1.0\" }
fix-a = { path = \"../a\", version = \"0.1.0\" }"

printf 'fix-a\t404\nfix-b\t404\nfix-umb\t404\n' > "$FIX/all404.txt"
printf 'fix-a\t404\nfix-b\t200\nfix-umb\t404\n' > "$FIX/taken.txt"
printf 'fix-a\t503\nfix-b\t404\nfix-umb\t404\n' > "$FIX/inconclusive.txt"

# ── a) all free, --check-only: exit 0, ordered, fixture excluded ─────────────
out="$(HH_PUBLISH_STUB_FILE="$FIX/all404.txt" HH_PUBLISH_RECORDED_NOPUBLISH="fix-fixture" \
  bash "$SCRIPT" --root "$FIX" --check-only 2>&1)" && rc=0 || rc=$?
if [ "$rc" -ne 0 ]; then fail "all-free run exited $rc: $out"; fi
ord="$(printf '%s\n' "$out" | grep -nE '^  +[0-9]+\. ' )"
ia=$(printf '%s\n' "$ord" | grep -n 'fix-a' | cut -d: -f1)
ib=$(printf '%s\n' "$ord" | grep -n 'fix-b' | cut -d: -f1)
iu=$(printf '%s\n' "$ord" | grep -n 'fix-umb' | cut -d: -f1)
if [ -n "$ia" ] && [ -n "$ib" ] && [ -n "$iu" ]; then
  [ "$ia" -lt "$ib" ] && [ "$ib" -lt "$iu" ] || fail "order wrong: a=$ia b=$ib umb=$iu"
else fail "order lines missing: $out"; fi
printf '%s\n' "$ord" | grep -q 'fix-fixture' && fail "publish=false member leaked into the set" || true
printf '%s\n' "$out" | grep -q '3 publishable crates, 1 publish=false' \
  || fail "set-count line unexpected: $(printf '%s\n' "$out" | grep publishable)"

# ── b) a taken name stops the run (exit 3, never silent) ──────────────────────
out="$(HH_PUBLISH_STUB_FILE="$FIX/taken.txt" HH_PUBLISH_RECORDED_NOPUBLISH="fix-fixture" \
  bash "$SCRIPT" --root "$FIX" --check-only 2>&1)" && rc=0 || rc=$?
[ "$rc" -eq 3 ] || fail "taken name did not exit 3 (rc=$rc): $out"
printf '%s\n' "$out" | grep -q 'STOP' || fail "taken-name run lacked the STOP line"
printf '%s\n' "$out" | grep -q 'fix-b' || fail "taken name not named: $out"

# ── c) an inconclusive answer stops the run too ──────────────────────────────
out="$(HH_PUBLISH_STUB_FILE="$FIX/inconclusive.txt" HH_PUBLISH_RECORDED_NOPUBLISH="fix-fixture" \
  bash "$SCRIPT" --root "$FIX" --check-only 2>&1)" && rc=0 || rc=$?
[ "$rc" -eq 3 ] || fail "inconclusive answer did not exit 3 (rc=$rc)"

# ── d) publish-set drift fails loudly (exit 2) ───────────────────────────────
out="$(HH_PUBLISH_STUB_FILE="$FIX/all404.txt" HH_PUBLISH_RECORDED_NOPUBLISH="someone-else" \
  bash "$SCRIPT" --root "$FIX" --check-only 2>&1)" && rc=0 || rc=$?
[ "$rc" -eq 2 ] || fail "publish-set drift did not exit 2 (rc=$rc): $out"
printf '%s\n' "$out" | grep -q 'drift' || fail "drift run lacked the drift message"

if [ "$FAIL" -eq 0 ]; then say "ok — stubbed collision/drift/order paths all behave"; fi
exit "$FAIL"
