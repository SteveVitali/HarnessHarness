#!/usr/bin/env bash
# E3 smoke leg (R4.2; HUMAN-H1's E3=TypeScript binding): spawn
# `target/debug/hh-kernel serve --http` (binding (c), `local_network`)
# on a free loopback port and drive the generated TypeScript client —
# `clients/ts/hh-embed.ts`, emitted from the single schema source (CC7)
# — end to end via `clients/ts/smoke.ts`: the `hh doctor` facts plus one
# run's open/submit/read and an event-stream read.
#
# Exit 0 = all checks passed; 3 = skipped(node-absent) or node < 22
# (the honest skip — `crates/hh-codegen/tests/r4_2.rs` reports it);
# 2 = kernel binary missing; 1 = a leg failed.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
KERNEL="$ROOT/target/debug/hh-kernel"

if ! command -v node >/dev/null 2>&1; then
  echo "e3-smoke: skipped(node-absent) — node >= 22 required" >&2
  exit 3
fi
NODE_MAJOR="$(node -e 'process.stdout.write(process.versions.node.split(".")[0])')"
if [ "$NODE_MAJOR" -lt 22 ]; then
  echo "e3-smoke: skipped(node-absent) — node >= 22 required, found $(node --version)" >&2
  exit 3
fi
if [ ! -x "$KERNEL" ]; then
  echo "e3-smoke: $KERNEL missing — run 'cargo build -p hharness-kernel' first" >&2
  exit 2
fi

TMP="$(mktemp -d "${TMPDIR:-/tmp}/hh-e3-smoke.XXXXXX")"
KPID=""
cleanup() {
  if [ -n "$KPID" ]; then
    kill "$KPID" 2>/dev/null || true
    wait "$KPID" 2>/dev/null || true
  fi
  rm -rf "$TMP"
}
trap cleanup EXIT

# The run the smoke opens writes a real ledger — keep it inside the
# temp dir, never the worktree's `.hh/store`.
export HH_STORE_ROOT="$TMP/store"
export HH_WORKSPACE_ROOT="$TMP/workspace"
mkdir -p "$HH_WORKSPACE_ROOT"

# `serve --http 127.0.0.1:0` binds a free loopback port and prints
# `bound=http://<addr>` on stderr; the token lands mode-0600 via
# --token-file (never a URL, never a log — P9).
"$KERNEL" serve --http 127.0.0.1:0 --token-file "$TMP/token" \
  >"$TMP/out.log" 2>"$TMP/serve.log" &
KPID=$!

ADDR=""
for _ in $(seq 1 400); do
  if grep -q 'bound=http://' "$TMP/serve.log" 2>/dev/null; then
    ADDR="$(sed -n 's/.*bound=http:\/\/\([^ ]*\).*/\1/p' "$TMP/serve.log" | head -1)"
    break
  fi
  if ! kill -0 "$KPID" 2>/dev/null; then
    echo "e3-smoke: kernel exited before binding" >&2
    cat "$TMP/serve.log" >&2
    exit 1
  fi
  sleep 0.05
done
if [ -z "$ADDR" ]; then
  echo "e3-smoke: kernel did not bind within 20s" >&2
  cat "$TMP/serve.log" >&2
  exit 1
fi
TOKEN="$(tr -d '[:space:]' <"$TMP/token")"
# The doctor fact the smoke asserts: `KernelDescriptor.version` is the
# SemVer-class label — the `kernel_version_id` tail (ADR-0332).
EXPECT_VERSION="$(
  "$KERNEL" version | sed -n 's/.*"kernel_version_id":"[^"]*\/\([^"]*\)".*/\1/p'
)"

echo "e3-smoke: kernel bound at $ADDR (expect-version $EXPECT_VERSION)"
node "$ROOT/clients/ts/smoke.ts" \
  --addr "$ADDR" --token "$TOKEN" --expect-version "$EXPECT_VERSION"
echo "e3-smoke: ok — E3 (TypeScript) generated client drove hello + open/submit/read + stream over binding (c)"
