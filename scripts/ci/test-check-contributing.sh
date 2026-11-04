#!/usr/bin/env bash
# test-check-contributing.sh — self-test for scripts/check-contributing.sh
# (R5.31 / REQ-A-205 + REQ-B-004). Builds throwaway fixture repos seeded
# from the real checkout's contribution surface and drives the checker
# over them: a clean tree passes; a missing lane, dropped report field,
# removed privacy/verifyfixed/no-ritual wording, an ungated future route,
# a missing issue-form field, a fictitious `hh` command, manifest
# machinery leaking into the ordinary path and a broken link all fail;
# --head tests committed HEAD, never working-tree dirt; stripping the
# ci-mode annotation from the real workflow fails the step-mode lint.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
CHK="$ROOT/scripts/check-contributing.sh"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fails=0
say() { echo "test-check-contributing: $*"; }
bad() { echo "test-check-contributing: FAIL — $*" >&2; fails=$((fails + 1)); }

seed() {  # $1 = dir — a minimal repo carrying the real checkout's contribution surface
  local d="$TMP/$1"
  mkdir -p "$d/.github/ISSUE_TEMPLATE" "$d/crates/hh-cli/src" "$d/scripts"
  cp "$ROOT/CONTRIBUTING.md" "$d/"
  cp "$ROOT/.github/ISSUE_TEMPLATE/bug_report.yml" "$ROOT/.github/ISSUE_TEMPLATE/config.yml" \
     "$d/.github/ISSUE_TEMPLATE/"
  cp "$ROOT/.github/pull_request_template.md" "$d/.github/"
  cp "$ROOT/CODEOWNERS" "$d/"
  cp "$ROOT/crates/hh-cli/src/cli.rs" "$d/crates/hh-cli/src/"
  cp "$ROOT/scripts/check-contributing.sh" "$d/scripts/"
  cp "$ROOT/SECURITY.md" "$d/" 2>/dev/null || echo "# security" > "$d/SECURITY.md"
  # placeholder files for every repo-relative link CONTRIBUTING cites —
  # the checker's `links` leg resolves targets, not contents
  python3 - "$ROOT" "$d" <<'PY'
import os, re, sys
root, d = sys.argv[1], sys.argv[2]
doc = open(os.path.join(root, "CONTRIBUTING.md")).read()
for target in re.findall(r"\]\(([^)\s]+)\)", doc):
    if re.match(r"[a-z]+://", target) or target.startswith("#"):
        continue
    t = target.split("#", 1)[0]
    if not t:
        continue
    p = os.path.join(d, os.path.normpath(t))
    os.makedirs(os.path.dirname(p), exist_ok=True)
    if not os.path.exists(p):
        open(p, "w").write("# fixture placeholder\n")
PY
  git -C "$d" init -q >/dev/null
  git -C "$d" config user.email t@t; git -C "$d" config user.name t
  git -C "$d" add -A >/dev/null 2>&1
  git -C "$d" commit -qm seed >/dev/null 2>&1
  printf '%s' "$d"
}

expect() {  # $1 = ok|fail, $2 = label, $3+ = check-contributing.sh args
  local want="$1" label="$2"; shift 2
  local out rc=0
  out=$(bash "$CHK" "$@" 2>&1) || rc=$?
  if [ "$want" = ok ]; then
    [ "$rc" -eq 0 ] && say "ok  $label" || bad "$label: expected pass, got rc=$rc: $out"
  else
    [ "$rc" -ne 0 ] && say "ok  $label (rejected)" \
      || bad "$label: expected failure, got pass"
  fi
}

mutate() {  # $1 = file, $2 = sed expression — in-place, then show as one mutation
  sed -i '' "$2" "$1" 2>/dev/null || sed -i "$2" "$1"
}

# --- the seeded tree passes ---------------------------------------------------
D=$(seed good)
expect ok "seeded tree" "$D"

# --- lane integrity -------------------------------------------------------------
D=$(seed no-lane)
mutate "$D/CONTRIBUTING.md" 's|^### Report a wrong run or receipt|### Report a wrong receipt|'
expect fail "receipt lane heading drifted" "$D"

# --- no-run fields ---------------------------------------------------------------
D=$(seed no-exit)
mutate "$D/CONTRIBUTING.md" 's|including the exit code or signal|with its output|'
expect fail "no-run lane loses the exit-code field" "$D"

# --- receipt fields ---------------------------------------------------------------
D=$(seed no-export)
mutate "$D/CONTRIBUTING.md" 's|`hh run export <run-id> --out <dir>` produces|a helper command produces|'
expect fail "receipt lane cites no real export route" "$D"

# --- privacy -----------------------------------------------------------------------
D=$(seed no-secrets-block)
mutate "$D/CONTRIBUTING.md" 's|blocked until|held for|'
expect fail "uncleared-secrets block removed" "$D"

# --- verifyfixed ---------------------------------------------------------------------
D=$(seed no-vfix)
mutate "$D/CONTRIBUTING.md" 's|unable to reproduce|cannot be checked|'
expect fail "verifyfixed loses the unable-to-reproduce rule" "$D"

# --- no internal ritual ---------------------------------------------------------------
D=$(seed no-ritual)
python3 - "$D/CONTRIBUTING.md" <<'PY'
import re, sys
p = sys.argv[1]; t = open(p).read()
t = re.sub(r"internal question register, build ledger, or manifest\s*\n?\s*ticket ritual",
           "internal review", t)
open(p, "w").write(t)
PY
expect fail "no-ritual statement removed" "$D"

# --- gated routes ----------------------------------------------------------------------
D=$(seed ungated)
mutate "$D/CONTRIBUTING.md" 's|stays gated until its|opens immediately with its|'
expect fail "component lane ungated" "$D"

# --- issue form fields -------------------------------------------------------------------
D=$(seed no-form-field)
mutate "$D/.github/ISSUE_TEMPLATE/bug_report.yml" '/id: exit/d'
expect fail "issue form loses a documented field" "$D"

D=$(seed no-cleared)
python3 - "$D/.github/ISSUE_TEMPLATE/bug_report.yml" <<'PY'
import re, sys
p = sys.argv[1]; t = open(p).read()
t = re.sub(r"  - type: checkboxes.*", "", t, flags=re.S)
open(p, "w").write(t)
PY
expect fail "issue form loses the sanitization attestation" "$D"

# --- fictitious command ----------------------------------------------------------------------
D=$(seed bogus-cmd)
cat >> "$D/CONTRIBUTING.md" <<'EOF'

For one-step reports, run `hh bug-bundle --out repro.zip` and attach it.
EOF
expect fail "fictitious hh command advertised" "$D"

# --- CODEOWNERS broad manifest wording returns / owner review dropped -----------------------------
D=$(seed codeowners-back)
cat > "$D/CODEOWNERS" <<'EOF'
# CODEOWNERS — default review ownership.
# Everything lands through the manifest ticket chain; the repository owner
# reviews all paths.

* @SteveVitali
EOF
expect fail "broad manifest-chain CODEOWNERS wording" "$D"

D=$(seed codeowners-noowner)
cat > "$D/CODEOWNERS" <<'EOF'
# CODEOWNERS — default review ownership.
# The manifest ticket chain governs assigned build tickets only.
EOF
expect fail "CODEOWNERS loses the owner review line" "$D"

# --- manifest machinery leaking into the ordinary path ------------------------------------------
D=$(seed leak-manifest)
cat >> "$D/CONTRIBUTING.md" <<'EOF'

Every PR must first run `bash scripts/deferrals-open.sh` and record a
row in `docs/build/LEDGER.md`.
EOF
expect fail "manifest bureaucracy imposed on ordinary PRs" "$D"

# --- broken link ----------------------------------------------------------------------------------
D=$(seed bad-link)
mutate "$D/CONTRIBUTING.md" 's|(SECURITY.md)|(SECURITY-MISSING.md)|'
expect fail "repo-relative link does not resolve" "$D"

# --- committed-HEAD vs working-tree ------------------------------------------------------------------
D=$(seed head-dirty)
mutate "$D/CONTRIBUTING.md" 's|including the exit code or signal|with its output|'
expect ok "--head ignores working-tree dirt" --head "$D"
expect fail "working-tree mode sees the dirt" "$D"
git -C "$D" add -A && git -C "$D" commit -qm dirty >/dev/null 2>&1
expect fail "--head fails on committed drift" --head "$D"

# --- missing ci-mode annotation fails the step-mode lint -----------------------------------------------
sed '/# ci-mode: both — reads committed contribution-route docs and forms/,+2d' \
  "$ROOT/.github/workflows/ci.yml" > "$TMP/noannot.yml"
if grep -q "Contribution routes" "$TMP/noannot.yml"; then
  if (cd "$ROOT" && bash scripts/ci/check-ci-gating.sh "$TMP/noannot.yml") >"$TMP/noannot.out" 2>&1; then
    bad "ci.yml minus the annotation passed check-ci-gating"
  else
    grep -q "Contribution routes" "$TMP/noannot.out" \
      && say "ok  missing ci-mode annotation rejected" \
      || bad "lint failed but did not name the Contribution routes step: $(cat "$TMP/noannot.out")"
  fi
else
  say "skip annotation-mutation leg (Contribution routes step not yet in ci.yml)"
fi

# --- the real checkout is green ------------------------------------------------------------------------
expect ok "real checkout (working tree)" "$ROOT"

[ "$fails" -eq 0 ] || { echo "test-check-contributing: $fails failure(s)" >&2; exit 1; }
echo "test-check-contributing: OK"
