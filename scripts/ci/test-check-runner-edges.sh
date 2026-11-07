#!/usr/bin/env bash
# Self-test for check-runner-edges.sh: a clean seeded tree passes, a tree
# seeded with an hh-embed edge fails (AC-3.2-3 — the negative leg proves
# the gate sees a violation, never vacuously passes).
set -euo pipefail
cd "$(dirname "$0")/../.."

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat >"$TMP/clean.txt" <<'EOF'
hharness-runner v0.1.0 (crates/hh-runner)
hharness-experiment v0.1.0 (crates/hh-experiment)
hharness-ledger v0.1.0 (crates/hh-ledger)
hharness-wire v0.1.0 (crates/hh-wire)
EOF

cat >"$TMP/dirty.txt" <<'EOF'
hharness-runner v0.1.0 (crates/hh-runner)
hharness-embed v0.1.0 (crates/hh-embed)
hharness-ledger v0.1.0 (crates/hh-ledger)
EOF

HH_RUNNER_TREE_FILE="$TMP/clean.txt" bash scripts/check-runner-edges.sh
if HH_RUNNER_TREE_FILE="$TMP/dirty.txt" bash scripts/check-runner-edges.sh 2>"$TMP/err.txt"; then
  echo "test-check-runner-edges: FAIL — seeded hh-embed edge passed" >&2
  exit 1
fi
grep -q 'K-2 edge' "$TMP/err.txt" || { echo "test-check-runner-edges: FAIL — wrong refusal text" >&2; exit 1; }
echo "test-check-runner-edges: OK — clean tree passes, seeded edge refuses"
