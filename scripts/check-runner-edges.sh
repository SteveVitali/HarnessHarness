#!/usr/bin/env bash
# check-runner-edges.sh — the runner crate must not introduce a K-2 edge:
# no dependency (direct or transitive) on hh-embed, hh-cli or
# hharness-hosting (manifest 261/R6.8; AR-04/ADR-0427). The live check
# reads `cargo tree`; the self-test seeds a tree through
# HH_RUNNER_TREE_FILE.
set -euo pipefail
cd "$(dirname "$0")/.."

FORBIDDEN='hharness-embed|hharness-cli|hharness-hosting|hh-embed-schema|hharness-embed-client-generated|hharness-kernel|hharness-orchestrator|hharness-fleet'

if [ -n "${HH_RUNNER_TREE_FILE:-}" ]; then
  TREE="$(cat "$HH_RUNNER_TREE_FILE")"
else
  TREE="$(cargo tree -p hharness-runner --prefix none -e normal 2>/dev/null || true)"
fi

if [ -z "$TREE" ]; then
  echo "check-runner-edges: FAIL — no cargo-tree output for hharness-runner" >&2
  exit 1
fi

HITS="$(printf '%s\n' "$TREE" | grep -E "(${FORBIDDEN})[[:space:]]" || true)"
if [ -n "$HITS" ]; then
  echo "check-runner-edges: FAIL — hharness-runner introduces a K-2 edge:" >&2
  printf '%s\n' "$HITS" >&2
  exit 1
fi
echo "check-runner-edges: OK — no edge into hh-embed/hh-cli/hosting/kernel/orchestrator/fleet"
