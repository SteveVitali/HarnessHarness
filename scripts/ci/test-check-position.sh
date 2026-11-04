#!/usr/bin/env bash
# test-check-position.sh — self-test for scripts/check-position.sh
# (R5.35 / REQ-B-001 + REQ-C-008 + REQ-C-101 + REQ-C-102). Builds
# throwaway fixture repos seeded from the real checkout's positioning
# surface and drives the checker over them: a clean tree passes; a
# dropped entry choice, an invented card state, a useful-task card
# marked available (fixture substitution), a dropped family/later-group
# row, a removed card field, a missing claim-map source, banned
# favorable-treatment language, a lost unmet clause, a broken link and a
# missing nav row all fail; --head tests committed HEAD, never
# working-tree dirt; stripping the ci-mode annotation from the real
# workflow fails the step-mode lint.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
CHK="$ROOT/scripts/check-position.sh"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fails=0
say() { echo "test-check-position: $*"; }
bad() { echo "test-check-position: FAIL — $*" >&2; fails=$((fails + 1)); }

seed() {  # $1 = dir — a minimal repo carrying the real checkout's positioning surface
  local d="$TMP/$1"
  mkdir -p "$d/docs" "$d/scripts"
  cp "$ROOT/README.md" "$d/"
  cp "$ROOT/docs/evidence.md" "$d/docs/"
  cp "$ROOT/docs/README.md" "$d/docs/"
  cp "$ROOT/docs/evidence-and-adoption-spec.md" "$d/docs/"
  cp "$ROOT/scripts/check-position.sh" "$d/scripts/"
  # placeholder files for every repo-relative link in docs/evidence.md
  # and the README spine block — the checker's link legs resolve
  # targets, not contents
  python3 - "$ROOT" "$d" <<'PY'
import os, re, sys
root, d = sys.argv[1], sys.argv[2]
def marked(doc, name):
    m = re.search(
        rf"<!--\s*{re.escape(name)}(?::begin)?\s*-->(.*?)<!--\s*{re.escape(name)}:end\s*-->",
        doc, re.S)
    return m.group(1) if m else ""
for rel, base in (("docs/evidence.md", "docs"), ("README.md", "")):
    doc = open(os.path.join(root, rel)).read()
    if rel == "README.md":
        doc = marked(doc, "spine")
    for target in re.findall(r"\]\(([^)\s]+)\)", doc):
        if re.match(r"[a-z]+://", target) or target.startswith("#"):
            continue
        t = target.split("#", 1)[0]
        if not t:
            continue
        p = os.path.join(d, os.path.normpath(os.path.join(base, t)))
        if t.endswith("/"):
            os.makedirs(p, exist_ok=True)
            # a bare dir target: leave a tracked file — git ignores empty
            # dirs, so a bare makedirs would vanish under --head's clone
            keep = os.path.join(p, ".keep")
            if not os.path.exists(keep):
                open(keep, "w").write("\n")
            continue
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

expect() {  # $1 = ok|fail, $2 = label, $3+ = check-position.sh args
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

mutate() {  # $1 = file, $2 = sed expression — in-place
  sed -i '' "$2" "$1" 2>/dev/null || sed -i "$2" "$1"
}

# --- the seeded tree passes ---------------------------------------------------
D=$(seed good)
expect ok "seeded tree" "$D"

# --- spine entries ---------------------------------------------------------------
D=$(seed no-entry)
mutate "$D/README.md" 's|\*\*Run a useful task\*\*|**Attempt a task**|'
expect fail "entry choice renamed" "$D"

D=$(seed no-exit)
python3 - "$D/README.md" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
t = t.replace("[troubleshooting](docs/getting-started.md#troubleshooting) or a\n[report](CONTRIBUTING.md)",
              "[corrections](docs/reference.md)")
open(p, "w").write(t)
PY
expect fail "spine loses the failure/report exit" "$D"

# --- card shape / labels / gates -----------------------------------------------------
D=$(seed bad-state)
mutate "$D/docs/evidence.md" 's|- \*\*State:\*\* Available — scoped evidence|- **State:** Verified|'  # first card
expect fail "invented card state" "$D"

D=$(seed fixture-subst)
python3 - "$D/docs/evidence.md" <<'PY'
import re, sys
p = sys.argv[1]; t = open(p).read()
m = re.search(r"### Card — Run a useful task \[useful-task\].*?(?=\n### )", t, re.S)
t = t.replace(m.group(0), m.group(0).replace(
    "**State:** Not yet available", "**State:** Available"))
open(p, "w").write(t)
PY
expect fail "fixture substitution — useful-task marked available" "$D"

D=$(seed no-field)
python3 - "$D/docs/evidence.md" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
open(p, "w").write(t.replace("- **Limits:**", "- **Limit:**", 1))
PY
expect fail "card loses a required field" "$D"

# --- portfolio retention ----------------------------------------------------------------
D=$(seed no-family)
mutate "$D/docs/evidence.md" '/^| Fleet and external work sources |/d'
expect fail "a capability family silently deleted" "$D"

D=$(seed no-later)
mutate "$D/docs/evidence.md" '/^| L7 — external harnesses/d'
expect fail "a later group silently deleted" "$D"

D=$(seed no-scenario)
python3 - "$D/docs/evidence.md" <<'PY'
import re, sys
p = sys.argv[1]; t = open(p).read()
t = re.sub(r"^\| S4 — operations.*$", "", t, count=1, flags=re.M)
open(p, "w").write(t)
PY
expect fail "a scenario silently deleted" "$D"

D=$(seed no-job)
python3 - "$D/docs/evidence.md" <<'PY'
import sys
p = sys.argv[1]; t = open(p).read()
open(p, "w").write(t.replace("harness evaluator", "bench evaluator"))
PY
expect fail "selected job unnamed" "$D"

D=$(seed no-demand)
mutate "$D/docs/evidence.md" 's|a \*hypothesis\* chosen|a need established|'
expect fail "inferred-demand label dropped" "$D"

# --- unmet preservation --------------------------------------------------------------------
D=$(seed no-unmet)
mutate "$D/docs/evidence.md" 's|are \*\*unmet\*\*:|are complete:|'
expect fail "S1/S3 unmet clause weakened" "$D"

# --- claim-map sources -----------------------------------------------------------------------
D=$(seed bad-source)
mutate "$D/docs/evidence.md" 's|](research/04_audiences-and-jobs.md)|](research/04_missing.md)|'
expect fail "claim maps to a nonexistent source" "$D"

# --- honesty ------------------------------------------------------------------------------------
D=$(seed claim)
cat >> "$D/docs/evidence.md" <<'EOF'

This runtime is production ready for every harness workload.
EOF
expect fail "favorable-treatment language on the positioning surface" "$D"

# --- nav -------------------------------------------------------------------------------------------
D=$(seed no-nav)
mutate "$D/docs/README.md" 's|(evidence\.md)|(evidence-missing.md)|g'
expect fail "docs/README nav row drifted" "$D"

# --- committed-HEAD vs working-tree ------------------------------------------------------------------
D=$(seed head-dirty)
mutate "$D/README.md" 's|\*\*Run a useful task\*\*|**Attempt a task**|'
expect ok "--head ignores working-tree dirt" --head "$D"
expect fail "working-tree mode sees the dirt" "$D"
git -C "$D" add -A && git -C "$D" commit -qm dirty >/dev/null 2>&1
expect fail "--head fails on committed drift" --head "$D"

# --- missing ci-mode annotation fails the step-mode lint -----------------------------------------------
sed '/# ci-mode: both — reads committed reader-spine/,+2d' \
  "$ROOT/.github/workflows/ci.yml" > "$TMP/noannot.yml"
if grep -q "Reader spine" "$TMP/noannot.yml"; then
  if (cd "$ROOT" && bash scripts/ci/check-ci-gating.sh "$TMP/noannot.yml") >"$TMP/noannot.out" 2>&1; then
    bad "ci.yml minus the annotation passed check-ci-gating"
  else
    grep -q "Reader spine" "$TMP/noannot.out" \
      && say "ok  missing ci-mode annotation rejected" \
      || bad "lint failed but did not name the Reader spine step: $(cat "$TMP/noannot.out")"
  fi
else
  say "skip annotation-mutation leg (Reader spine step not yet in ci.yml)"
fi

# --- the real checkout is green ------------------------------------------------------------------------
expect ok "real checkout (working tree)" "$ROOT"

[ "$fails" -eq 0 ] || { echo "test-check-position: $fails failure(s)" >&2; exit 1; }
echo "test-check-position: OK"
