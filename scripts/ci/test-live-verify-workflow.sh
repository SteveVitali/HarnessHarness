#!/usr/bin/env bash
# test-live-verify-workflow.sh — mutation self-test for
# scripts/ci/check-live-verify-workflow.sh (L4.3). Copies the real
# workflows into a temp dir, applies one mutation per case, and asserts
# the lint turns red — plus a green run over the real directory.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
CK="$ROOT/scripts/ci/check-live-verify-workflow.sh"
SRC="$ROOT/.github/workflows"
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
fails=0
say() { echo "test-live-verify-workflow: $*"; }
bad() { echo "test-live-verify-workflow: FAIL — $*" >&2; fails=$((fails + 1)); }

mksuite() { # $1 = case dir; copies the real workflows
  mkdir -p "$T/$1" && cp "$SRC"/*.yml "$T/$1/"
}
check() { # $1 desc, $2 want-exit, $3 dir
  local rc=0; "$CK" "$3" >/dev/null 2>&1 || rc=$?
  [ "$rc" = "$2" ] || bad "$1 — exit $rc, want $2"
  [ "$rc" = "$2" ] && say "ok  $1"
}

check "real workflows pass" 0 "$SRC"

# A pull_request token anywhere in live-verify.yml is a violation.
mksuite mut-pr
printf '  pull_request:\n' >> "$T/mut-pr/live-verify.yml"
check "planted pull_request trigger is refused" 1 "$T/mut-pr"

# A PR-triggered workflow must never reference secrets.*
mksuite mut-secrets
printf '      # secrets.DEPLOY_TOKEN added\n' >> "$T/mut-secrets/ci.yml"
check "secrets ref in a pull_request workflow is refused" 1 "$T/mut-secrets"

# A new pull_request workflow that references secrets is refused too.
mksuite mut-newwf
printf 'name: x\non:\n  pull_request:\njobs:\n  j:\n    runs-on: ubuntu-latest\n    steps:\n      - env: { T: ${{ secrets.X }} }\n        run: echo x\n' \
  > "$T/mut-newwf/evil.yml"
check "new PR workflow referencing secrets is refused" 1 "$T/mut-newwf"

# Dropping the environment binding is a violation.
mksuite mut-env
grep -v 'environment: live-verify' "$SRC/live-verify.yml" > "$T/mut-env/live-verify.yml"
check "missing environment binding is refused" 1 "$T/mut-env"

# Dropping the dispatch trigger is a violation.
mksuite mut-dispatch
grep -v 'workflow_dispatch' "$SRC/live-verify.yml" > "$T/mut-dispatch/live-verify.yml"
check "missing workflow_dispatch is refused" 1 "$T/mut-dispatch"

[ "$fails" -eq 0 ] || { echo "test-live-verify-workflow: $fails failure(s)" >&2; exit 1; }
echo "test-live-verify-workflow: OK"
