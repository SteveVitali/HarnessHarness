#!/usr/bin/env bash
# live-oauth-device.sh — the `oauth-device` leg driver (R4.7a;
# DF-S4.11-2; ADR-0363). Phases:
#
#   scripts/live-oauth-device.sh initiate   # device-code request → code file + state
#   scripts/live-oauth-device.sh complete   # poll → token → GET /user → subject hash
#   scripts/live-oauth-device.sh auto       # initiate, show the code, poll (operator present)
#
# The user_code + verification_uri are written to $HH_OAUTH_CODE_FILE
# (default docs/build/logs/oauth-device.code — a gitignored path, NEVER
# a transcript/ledger) and echoed to /dev/tty when one is attached; the
# device_code persists at $HH_OAUTH_STATE (mode 0600) for the resumable
# complete leg. Secrets reach the cells only as NAME values
# (HH_GITHUB_OAUTH_CLIENT_ID) — no value ever prints.
set -euo pipefail
set +x

REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
STATE="${HH_OAUTH_STATE:-$REPO/docs/build/logs/oauth-device.state.json}"
CODE="${HH_OAUTH_CODE_FILE:-$REPO/docs/build/logs/oauth-device.code}"
PHASE="${1:-auto}"

export HH_OAUTH_STATE="$STATE" HH_OAUTH_CODE_FILE="$CODE"

run_cell() { # $1 = test filter
  (cd "$REPO" && cargo test -p hharness-mcp-lab --test r4_7a_live "$1" -- --ignored --nocapture) 2>&1
}

show_code() { # operator-facing channel — /dev/tty when attached, else the file path only
  if [ -f "$CODE" ]; then
    if [ -w /dev/tty ]; then cat "$CODE" > /dev/tty; fi
    printf 'OAUTH_CODE_FILE %s\n' "$CODE"
  fi
}

case "$PHASE" in
  initiate)
    run_cell r47a_oauth_device_initiate
    show_code
    ;;
  complete)
    run_cell r47a_oauth_device_complete
    ;;
  auto)
    run_cell r47a_oauth_device_initiate
    show_code
    run_cell r47a_oauth_device_complete
    ;;
  *)
    printf 'usage: %s [initiate|complete|auto]\n' "$0" >&2
    exit 1
    ;;
esac
