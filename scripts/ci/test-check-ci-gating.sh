#!/usr/bin/env bash
# test-check-ci-gating.sh — self-test for scripts/ci/check-ci-gating.sh
# (R4.2a / ADR-0362). Drives the lint over fixture workflows (one valid file
# plus sed/awk mutations) and over a throwaway repo for the readers scan.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
LINT="$ROOT/scripts/ci/check-ci-gating.sh"
FIX="$ROOT/scripts/ci/fixtures"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fails=0
say() { echo "test-check-ci-gating: $*"; }
bad() { echo "test-check-ci-gating: FAIL — $*" >&2; fails=$((fails + 1)); }

export CI_GATING_ALLOW="$TMP/empty.allow" ; : > "$CI_GATING_ALLOW"
export CI_GATING_ROOT="$TMP/nohits" ; mkdir -p "$CI_GATING_ROOT"

expect() {  # $1 = ok|fail, $2 = label, $3 = workflow file
  local want="$1" label="$2" wf="$3" out rc=0
  out=$(bash "$LINT" "$wf" 2>&1) || rc=$?
  if [ "$want" = ok ]; then
    [ "$rc" -eq 0 ] && say "ok  $label" || bad "$label: expected pass, got rc=$rc: $out"
  else
    [ "$rc" -ne 0 ] && say "ok  $label (rejected)" \
      || bad "$label: expected failure, got pass"
  fi
}

# --- the valid fixture passes --------------------------------------------------
expect ok "valid fixture" "$FIX/ci-valid.yml"

# --- the real workflow passes --------------------------------------------------
unset CI_GATING_ALLOW CI_GATING_ROOT
if bash "$LINT" "$ROOT/.github/workflows/ci.yml" >/dev/null 2>"$TMP/e"; then
  say "ok  real ci.yml"
else
  bad "real ci.yml rejected: $(cat "$TMP/e")"
fi
export CI_GATING_ALLOW="$TMP/empty.allow" CI_GATING_ROOT="$TMP/nohits"

# --- step-mode violations ------------------------------------------------------

# ungated step: drop Build's if:
awk '{ if (skip) { skip=0; next } if ($0 ~ /name: Build/) skip=1; print }' \
  "$FIX/ci-valid.yml" > "$TMP/ungated.yml"
expect fail "ungated" "$TMP/ungated.yml"
out=$(bash "$LINT" "$TMP/ungated.yml" 2>&1 || true)
grep -q "step 'Build'" <<<"$out" \
  || bad "ungated failure did not name the Build step"

# gate weakened with ||
sed "s/if: steps.path.outputs.mode != 'fast'\$/if: steps.path.outputs.mode != 'fast' || always()/" \
  "$FIX/ci-valid.yml" > "$TMP/orgate.yml"
expect fail "orgate" "$TMP/orgate.yml"

# always() inside a gate
sed "s/if: steps.path.outputs.mode != 'fast' && github.event_name/if: steps.path.outputs.mode != 'fast' \&\& always() \&\& github.event_name/" \
  "$FIX/ci-valid.yml" > "$TMP/always.yml"
expect fail "always" "$TMP/always.yml"

# !cancelled() in a gate
sed "s/if: steps.path.outputs.mode != 'fast'\$/if: !cancelled() \&\& steps.path.outputs.mode != 'fast'/" \
  "$FIX/ci-valid.yml" > "$TMP/cancelled.yml"
expect fail "notcancelled" "$TMP/cancelled.yml"

# anchor pair moved before the last two positions
awk '
  /- name: Compute anchor/,/upload-artifact@v4/ { next }
  { print }
' "$FIX/ci-valid.yml" > "$TMP/mid.yml"
sed "/name: Build/i\\
      - name: Compute anchor\\
        id: anchor\\
        if: success() \&\& steps.path.outputs.mode == 'full' \&\& (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository)\\
        run: echo x > ci-anchor.txt\\
\\
      - name: Upload anchor\\
        if: success() \&\& steps.path.outputs.mode == 'full' \&\& (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository)\\
        uses: actions/upload-artifact@v4\\
" "$TMP/mid.yml" > "$TMP/moved.yml"
expect fail "movedanchor" "$TMP/moved.yml"

# anchor if: wrong
sed "s/(github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository)/(always())/" \
  "$FIX/ci-valid.yml" > "$TMP/badanchor.yml"
expect fail "badanchor" "$TMP/badanchor.yml"

# '|| always()' appended to Upload anchor's if: (M12 shape)
awk '{ if (mark) { mark=0; print $0 " || always()"; next }
       if ($0 ~ /name: Upload anchor/) mark=1; print }' \
  "$FIX/ci-valid.yml" > "$TMP/anchoror.yml"
expect fail "anchoror" "$TMP/anchoror.yml"

# --- permissions / triggers / hook ---------------------------------------------
sed '/actions: read/d' "$FIX/ci-valid.yml" > "$TMP/noperm.yml"
expect fail "noperm" "$TMP/noperm.yml"
sed 's/branches: \[main\]/branches: ["**"]/' "$FIX/ci-valid.yml" > "$TMP/pushall.yml"
expect fail "pushall" "$TMP/pushall.yml"
sed '/branches: \[main\]/d' "$FIX/ci-valid.yml" > "$TMP/pushnone.yml"
expect fail "pushnone" "$TMP/pushnone.yml"
sed 's/run: echo full/run: echo full # CI_ANCHOR_LISTING/' "$FIX/ci-valid.yml" > "$TMP/hook.yml"
expect fail "hook" "$TMP/hook.yml"

# --- readers scan ----------------------------------------------------------------
scanrepo() {  # a throwaway repo whose only hit is the line $1
  local d="$TMP/scan$2"
  mkdir -p "$d/crates/x" && cd "$d" && git init -q .
  printf '%s\n' "$1" > crates/x/t.rs
  echo seed > README && git add -A
  git -c user.name=ci -c user.email=ci@localhost commit -qm i
  echo "$d"
}

# a real read hit with no allowlist entry -> fail (M10 shape)
D=$(scanrepo 'let s = read("docs/secret.txt");' 1)
CI_GATING_ROOT="$D" CI_GATING_ALLOW="$TMP/empty.allow" \
  expect fail "unallowlisted readers-scan hit" "$FIX/ci-valid.yml"
# the same hit allowlisted exactly -> pass
printf 'crates/x/t.rs\tlet s = read("docs/secret.txt");\t# fixture test literal\n' \
  > "$TMP/one.allow"
CI_GATING_ROOT="$D" CI_GATING_ALLOW="$TMP/one.allow" \
  expect ok "allowlisted hit" "$FIX/ci-valid.yml"
# a broad-pattern entry -> fail (M11 shape)
printf 'crates/x/t.rs\tdocs/*\t# pattern\n' > "$TMP/pat.allow"
CI_GATING_ROOT="$D" CI_GATING_ALLOW="$TMP/pat.allow" \
  expect fail "patternallow" "$FIX/ci-valid.yml"
# a whole-file entry (path only) -> fail
printf 'crates/x/t.rs\n' > "$TMP/whole.allow"
CI_GATING_ROOT="$D" CI_GATING_ALLOW="$TMP/whole.allow" \
  expect fail "wholefile" "$TMP/whole.yml" 2>/dev/null \
  || expect fail "wholefile" "$FIX/ci-valid.yml"
# a stale entry -> fail
printf 'crates/x/t.rs\tlet s = read("docs/other.txt");\t# stale\n' \
  >> "$TMP/one.allow"
CI_GATING_ROOT="$D" CI_GATING_ALLOW="$TMP/one.allow" \
  expect fail "stale" "$FIX/ci-valid.yml"

[ "$fails" -eq 0 ] || { echo "test-check-ci-gating: $fails failure(s)" >&2; exit 1; }
echo "test-check-ci-gating: OK"
