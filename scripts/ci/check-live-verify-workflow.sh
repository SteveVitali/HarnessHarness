#!/usr/bin/env bash
# check-live-verify-workflow.sh — the dispatch-guard lint for
# .github/workflows/live-verify.yml (L4.3 / the "Local + manual
# workflow" ruling). Plain bash/awk for this repository's workflows,
# not a YAML parser.
#   check-live-verify-workflow.sh [workflows-dir]   (default .github/workflows)
# Checks: (1) live-verify.yml's `on:` block is workflow_dispatch-only —
#   no pull_request* token anywhere in the file (a dispatch-only
#   workflow never names a fork-visible event); (2) the job binds the
#   protected `live-verify` Environment; (3) every workflow whose `on:`
#   block contains a pull_request trigger references no `secrets.`
#   (a PR-triggered job never sees a secret — pull_request CI stays
#   secretless).
set -euo pipefail

DIR="${1:-.github/workflows}"
LV="$DIR/live-verify.yml"
fails=0
fail() { echo "check-live-verify-workflow: FAIL — $*" >&2; fails=$((fails + 1)); }

on_block() { awk '/^on:/ {o=1; next} o && /^[a-zA-Z]/ {o=0} o' "$1"; }

[ -f "$LV" ] || fail "no such file: $LV"
if [ -f "$LV" ]; then
  on_block "$LV" | grep -q 'workflow_dispatch' \
    || fail "live-verify.yml is not dispatch-triggered"
  # No pull_request* token outside comments — dispatch-only is the
  # contract; comments explaining it stay legal.
  if grep -vE '^[[:space:]]*#' "$LV" | grep -q 'pull_request'; then
    grep -n 'pull_request' "$LV" | grep -vE '^[0-9]+:[[:space:]]*#' >&2
    fail "live-verify.yml names pull_request* — dispatch-only is the contract"
  fi
  grep -qE '^[[:space:]]+environment:[[:space:]]*live-verify[[:space:]]*$' "$LV" \
    || fail "live-verify.yml does not bind the protected live-verify environment"
fi

for wf in "$DIR"/*.yml "$DIR"/*.yaml; do
  [ -f "$wf" ] || continue
  if on_block "$wf" | grep -q 'pull_request' && grep -q 'secrets\.' "$wf"; then
    fail "$wf has a pull_request trigger and references secrets.* — \
a PR-triggered job never sees a secret"
  fi
done

[ "$fails" -eq 0 ] || { echo "check-live-verify-workflow: $fails violation(s)" >&2; exit 1; }
echo "check-live-verify-workflow: OK"
