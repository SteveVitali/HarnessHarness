#!/usr/bin/env bash
# ollama-settle.sh — the controller-side settlement act (manifest
# 267/R6.14; spec §4.7, REQ-XC-127; offline fixture mode only — the live
# Ollama read is R6.40/R6.47's). Reads OLLAMA_API_KEY at use from
# HH_OPERATOR_ENV_FILE (never copied; `provided: yes/no` only), folds
# the supplied provider reads and appends the
# measurement.provider.usage.observed + measurement.cost.reconciled
# rows with the window id to the phase spend ledger (ADR-0432).
#
#   ollama-settle.sh <window> <hour-boundary-utc> <now-utc> \
#       <usage1.json> <usage2.json> <balance-pre.json> <balance-post.json> \
#       <hh-facts.json> [--non-exclusive]
set -euo pipefail
REPO="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
SLIB="$REPO/scripts/gce-backend/settle_lib.py"
LEDGER="${HH_PHASE_SPEND:-$REPO/docs/build/reports/live/PHASE_SPEND.jsonl}"
[ $# -ge 8 ] || { sed -n '2,16p' "$0"; exit 1; }
envf="${HH_OPERATOR_ENV_FILE:-}"
key=no
[ -n "$envf" ] && [ -f "$envf" ] && grep -q '^OLLAMA_API_KEY=' "$envf" \
  && key=yes
printf 'OLLAMA_API_KEY provided: %s\n' "$key"
[ "$key" = yes ] || { printf 'Refused{settle.key_unprovided}\n' >&2; exit 2; }
python3 -I "$SLIB" settle "$LEDGER" "$@"
