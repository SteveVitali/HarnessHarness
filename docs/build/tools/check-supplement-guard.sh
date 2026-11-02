#!/usr/bin/env bash
# check-supplement-guard.sh — hh-supplement-guard/1 entry point (R5.1).
#
# Retained, versioned, READ-ONLY supplemental coverage/routing guard for the
# evidence-and-adoption round. Validates the literal candidate register and
# ownership, AC/layer/contributor link resolution, active-round first-unlanded
# nextTicket against landed evidence, full-tail roles, chain/base ancestry and
# the scoped historical diagnostic baseline. Emits input digests, counts and
# actionable diagnostics; writes nothing into build state.
#
#   check-supplement-guard.sh [--json] [--checker-json FILE | --no-checker]
#
# Run from the repository root (or pass --root DIR). With no --checker-json it
# runs the STOCK external validator itself (AGENT_SKILLS_ROOT or
# CHECK_BUILD_MEMORY) and consumes its build-memory-check/2 sidecar; the stock
# validator and the repo row-tool checks stay unchanged — this guard is
# supplemental, never a second owner. Exit 0 pass / 1 guard failure / 2 usage
# or missing prerequisite.

set -euo pipefail
cd "$(dirname "$0")/../../.."
exec python3 docs/build/tools/supplement_guard.py --root . "$@"
