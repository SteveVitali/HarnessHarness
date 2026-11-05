#!/usr/bin/env bash
# run-examples.sh — R5.30 / REQ-F-106: the §7.3 "full offline" lane —
# one bounded leg after the existing build, executing the committed
# offline example cards (EX-01/EX-02 plus the admitted deterministic EX-03/EX-04 legs) through their checked batteries.
#
# Canonical inputs only: the cards under scripts/examples/ and the
# crate battery crates/hharness/tests/r5_29.rs — this lane never reads
# committed documentation or research content (ADR-0362: those stay out
# of the full-mode executor's transitive reads). Scratch stores only;
# no credentials, services, provider or task dispatch; USD 0.
#
# The live lane is recorded, never exercised: ADR-0363 owns authorized
# live legs and the unset allowance is zero — a skip is never a pass.
# Exit 0 = battery green; 1 = a leg failed.
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/hh-examples.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT
export HH_STORE_ROOT="$SCRATCH/store" HH_WORKSPACE_ROOT="$SCRATCH/workspace"
mkdir -p "$HH_STORE_ROOT" "$HH_WORKSPACE_ROOT"
say() { printf '%s\n' "run-examples: $*"; }

set +e
cargo test -p hharness --test r5_29 >"$SCRATCH/diag.log" 2>&1
rc=$?
if [ "$rc" -eq 0 ]; then
  cargo test -p hharness --test r5_45 >>"$SCRATCH/diag.log" 2>&1
  rc=$?
fi
set -e
if [ "$rc" -ne 0 ]; then
  sed 's/^/  /' "$SCRATCH/diag.log" >&2
  echo "run-examples: FAIL — the offline card battery exited $rc" >&2
  exit 1
fi
tail -n 4 "$SCRATCH/diag.log" | sed 's/^/  /'
bytes=$(wc -c <"$SCRATCH/diag.log" | tr -d ' ')
say "offline cards EX-01–04 green (diagnostics ${bytes} bytes — bound ≤50 MiB; scratch under the temp dir, bound ≤1 GiB)"
say "live: skipped(no routine PR calls — ADR-0363 owns authorized live legs; unset allowance is zero)"
