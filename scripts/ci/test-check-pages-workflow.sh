#!/usr/bin/env bash
# test-check-pages-workflow.sh — mutation battery for
# scripts/ci/check-pages-workflow.sh (AR-28): seeds a copy of pages.yml,
# confirms the clean copy is green and each contract mutation is
# rejected; the real file is green.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
CHK="$ROOT/scripts/ci/check-pages-workflow.sh"
SRC="$ROOT/.github/workflows/pages.yml"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fails=0
say() { echo "test-check-pages-workflow: $*"; }
bad() { echo "test-check-pages-workflow: FAIL — $*" >&2; fails=$((fails + 1)); }

expect() {  # $1 = ok|fail, $2 = label, $3 = file
  local want="$1" label="$2" f="$3" out rc=0
  out=$(bash "$CHK" "$f" 2>&1) || rc=$?
  if [ "$want" = ok ]; then
    [ "$rc" -eq 0 ] && say "ok  $label" || bad "$label: expected pass: $out"
  else
    [ "$rc" -eq 1 ] && say "ok  $label (rejected)" \
      || bad "$label: expected rc=1, got rc=$rc: $out"
  fi
}

F="$TMP/wf.yml"; cp "$SRC" "$F"
expect ok "real pages.yml" "$SRC"
expect ok "seeded copy" "$F"

python3 - "$F" <<'PY'
import sys; p=sys.argv[1]; t=open(p).read()
assert 'branches: [main]' in t
open(p,"w").write(t.replace('branches: [main]','branches: [main, dev]'))
PY
expect fail "push not restricted to main" "$F"

cp "$SRC" "$F"
python3 - "$F" <<'PY'
import sys; p=sys.argv[1]; t=open(p).read()
assert 'workflow_dispatch:' in t
open(p,"w").write(t.replace('  workflow_dispatch:',''))
PY
expect fail "workflow_dispatch removed" "$F"

cp "$SRC" "$F"
python3 - "$F" <<'PY'
import sys; p=sys.argv[1]; t=open(p).read()
assert '  workflow_dispatch:' in t
open(p,"w").write(t.replace('  workflow_dispatch:','  workflow_dispatch:\n  pull_request:'))
PY
expect fail "pull_request trigger added" "$F"

cp "$SRC" "$F"
python3 - "$F" <<'PY'
import sys; p=sys.argv[1]; t=open(p).read()
assert '  contents: read' in t
open(p,"w").write(t.replace('  contents: read','  contents: write'))
PY
expect fail "permissions widened" "$F"

cp "$SRC" "$F"
python3 - "$F" <<'PY'
import sys; p=sys.argv[1]; t=open(p).read()
assert 'actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1' in t
open(p,"w").write(t.replace(
  'actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1',
  'actions/checkout@v7.0.1'))
PY
expect fail "action pinned by tag not SHA" "$F"

cp "$SRC" "$F"
python3 - "$F" <<'PY'
import sys; p=sys.argv[1]; t=open(p).read()
assert "steps.pub.outputs.published" in t
open(p,"w").write(t.replace("if: steps.pub.outputs.published == 'yes'",
                            "", 1))
PY
expect fail "published guard dropped from a deploy step" "$F"

cp "$SRC" "$F"
python3 - "$F" <<'PY'
import sys; p=sys.argv[1]; t=open(p).read()
assert 'concurrency:' in t
open(p,"w").write(t.replace('concurrency:\n  group: pages\n  cancel-in-progress: false\n',''))
PY
expect fail "concurrency block removed" "$F"

cp "$SRC" "$F"
python3 - "$F" <<'PY'
import sys; p=sys.argv[1]; t=open(p).read()
assert '--profile public' in t
open(p,"w").write(t.replace('--profile public',''))
PY
expect fail "public profile dropped" "$F"

cp "$SRC" "$F"
python3 - "$F" <<'PY'
import sys; p=sys.argv[1]; t=open(p).read()
assert 'docs/results/publication/*.json' in t
open(p,"w").write(t.replace('docs/results/publication/*.json','docs/results/*.json'))
PY
expect fail "record glob widened" "$F"

bash "$CHK" "$TMP/missing.yml" 2>/dev/null && bad "missing file passed" \
  || say "ok  missing file exits nonzero"

[ "$fails" -eq 0 ] || { echo "test-check-pages-workflow: $fails failure(s)" >&2; exit 1; }
echo "test-check-pages-workflow: OK"
