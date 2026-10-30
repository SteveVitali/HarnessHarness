#!/usr/bin/env bash
# dispatch-driver.sh — hh-dispatch-driver/1 entry point (R5.2).
#
# The versioned guarded dispatch driver for the native launch-path proof:
# `plan` performs every pre-spawn step (retained guard PASS + DISPATCH route,
# chain-tip/base ancestry, recorded ci-boundary/1 read, first-eligible row)
# and emits an exact hh-dispatch-envelope/1 — or refuses with named
# diagnostics. `validate` checks the coordinator's hh-dispatch-transcript/1
# against the envelope (ordered calls, spawn count, identities, digests,
# zero product mutation / provider budget, resume fresh-state re-read).
#
#   dispatch-driver.sh plan --leg {happy,resume} --ci-json FILE [--json]
#   dispatch-driver.sh validate --envelope FILE --transcript FILE
#
# Exit 0 EMIT/VALID · 1 REFUSE/INVALID · 2 usage or missing prerequisite.
set -euo pipefail
cd "$(dirname "$0")/../../.."
exec python3 docs/build/tools/dispatch_driver.py "$@"
