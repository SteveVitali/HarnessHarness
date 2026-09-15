#!/usr/bin/env bash
# CI schema-drift check (CC7): regenerate the schema export and the generated client bindings
# from the single schema source, then fail if the committed artifacts are stale. Schema drift
# is a build failure, never a runtime negotiation.
set -euo pipefail

ROOT="${1:-$(git rev-parse --show-toplevel)}"
cd "$ROOT"

echo "check-drift: regenerating artifacts from the single schema source…"
cargo run -q -p hh-codegen -- --root "$ROOT"

TARGETS=(schema/hh-embed-1.schema.json crates/hh-embed-client-generated/src/generated.rs)

if git diff --quiet -- "${TARGETS[@]}"; then
  echo "check-drift: ok — generated artifacts are in sync with the schema source."
  exit 0
fi

echo "check-drift: SCHEMA DRIFT — generated artifacts are stale." >&2
echo "  Fix: cargo run -p hh-codegen -- --root . && git add ${TARGETS[*]}" >&2
git --no-pager diff -- "${TARGETS[@]}" >&2
exit 1
