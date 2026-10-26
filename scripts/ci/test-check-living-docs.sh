#!/usr/bin/env bash
# test-check-living-docs.sh — self-test for scripts/check-living-docs.sh
# (L4.1). Builds throwaway fixture repos and drives the guard over them:
# clean trees pass, personal paths fail, placeholders + provenance trees +
# code fixtures pass. Also runs the guard on the real checkout.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
CHK="$ROOT/scripts/check-living-docs.sh"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fails=0
say() { echo "test-check-living-docs: $*"; }
bad() { echo "test-check-living-docs: FAIL — $*" >&2; fails=$((fails + 1)); }

fixture() {  # $1 = name; remaining stdin lines are "path<TAB>content" written+added
  local d="$TMP/$1" p c
  mkdir -p "$d"; git -C "$d" init -q >/dev/null
  git -C "$d" config user.email t@t; git -C "$d" config user.name t
  while IFS=$'\t' read -r p c; do
    [ -n "$p" ] || continue
    mkdir -p "$d/$(dirname "$p")"; printf '%s\n' "$c" > "$d/$p"
  done
  git -C "$d" add -A >/dev/null 2>&1
  printf '%s' "$d"
}

expect() {  # $1 = ok|fail, $2 = label, $3 = repo dir
  local want="$1" label="$2" d="$3" out rc=0
  out=$(bash "$CHK" "$d" 2>&1) || rc=$?
  if [ "$want" = ok ]; then
    [ "$rc" -eq 0 ] && say "ok  $label" || bad "$label: expected pass, got rc=$rc: $out"
  else
    [ "$rc" -ne 0 ] && say "ok  $label (rejected)" || bad "$label: expected failure, got pass"
  fi
}

# clean tree: markdown + scripts + a provenance path inside a provenance dir
d=$(fixture clean <<'EOF'
README.md	see the guide
docs/README.md	the index
docs/build/runs/T1.md	built in /Users/stevenvitali/worktree — provenance
research/LEDGER.md	authored at /home/steven/lab — provenance
scripts/x.sh	#!/bin/sh
EOF
)
expect ok "clean + provenance exempt" "$d"

# /Users/<name> in a living doc fails
d=$(fixture users <<'EOF'
GUIDE.md	check out /Users/stevenvitali/MetaHarness then build
EOF
)
expect fail "Users path rejected" "$d"

# /home/<name> in a living doc fails
d=$(fixture home <<'EOF'
CONTRIBUTING.md	clone to /home/steven/MetaHarness
EOF
)
expect fail "home path rejected" "$d"

# placeholders + code fixtures pass
d=$(fixture placeholders <<'EOF'
README.md	check out /home/user/HarnessHarness or /Users/<you>/HarnessHarness
.github/workflows/ci.yml	runs-on: ubuntu; workspace /home/runner/work/x
crates/hh-x/tests/t.rs	let p = "/home/u";
docs/adr/ADR-1.md	provenance path /Users/stevenvitali kept
EOF
)
expect ok "placeholders + code + provenance pass" "$d"

# the real checkout is green
expect ok "real checkout" "$ROOT"

[ "$fails" -eq 0 ] || { echo "test-check-living-docs: $fails failure(s)" >&2; exit 1; }
echo "test-check-living-docs: OK"
