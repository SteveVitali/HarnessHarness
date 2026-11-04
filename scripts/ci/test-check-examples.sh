#!/usr/bin/env bash
# test-check-examples.sh — self-test for scripts/check-examples.sh
# (R5.30 / REQ-F-106/107). Builds throwaway fixture repos seeded from the
# real checkout and drives the checker over them: a clean tree passes;
# broken cards, stale pins, stale/missing displayed blocks, runner-
# unmapped check ids and unpinned fixture data fail; --head tests
# committed HEAD, never working-tree dirt; stripping the ci-mode
# annotation from the real workflow fails the step-mode lint.
set -euo pipefail

ROOT=$(git rev-parse --show-toplevel)
CHK="$ROOT/scripts/check-examples.sh"
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fails=0
say() { echo "test-check-examples: $*"; }
bad() { echo "test-check-examples: FAIL — $*" >&2; fails=$((fails + 1)); }

seed() {  # $1 = dir — a minimal repo carrying the real checkout's example surface
  local d="$TMP/$1"
  mkdir -p "$d/scripts/examples" "$d/docs" "$d/crates/hharness/src" "$d/clients/ts"
  cp "$ROOT/scripts/examples/"*.json "$ROOT/scripts/examples/README.md" "$d/scripts/examples/"
  cp "$ROOT/scripts/check-examples.sh" "$d/scripts/"
  cp "$ROOT/docs/getting-started.md" "$d/docs/"
  cp "$ROOT/MAINTAINING.md" "$d/"
  cp "$ROOT/crates/hharness/src/examples.rs" "$d/crates/hharness/src/"
  cp "$ROOT/clients/ts/fixture-definition.json" "$d/clients/ts/"
  git -C "$d" init -q >/dev/null
  git -C "$d" config user.email t@t; git -C "$d" config user.name t
  git -C "$d" add -A >/dev/null 2>&1
  git -C "$d" commit -qm seed >/dev/null 2>&1
  printf '%s' "$d"
}

expect() {  # $1 = ok|fail, $2 = label, $3+ = check-examples.sh args
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

# --- the seeded tree passes ----------------------------------------------------
D=$(seed good)
expect ok "seeded tree" "$D"

# --- broken cards ---------------------------------------------------------------
D=$(seed extra-member)
python3 - "$D/scripts/examples/ex-01.json" <<'PY'
import json, sys
p = sys.argv[1]; c = json.load(open(p)); c["bogus"] = 1
json.dump(c, open(p, "w"), indent=2)
PY
expect fail "card member outside the closed set" "$D"

D=$(seed missing-checks)
python3 - "$D/scripts/examples/ex-01.json" <<'PY'
import json, sys
p = sys.argv[1]; c = json.load(open(p)); del c["checks"]
json.dump(c, open(p, "w"), indent=2)
PY
expect fail "card missing a required member" "$D"

D=$(seed bad-case)
python3 - "$D/scripts/examples/ex-01.json" <<'PY'
import json, sys
p = sys.argv[1]; c = json.load(open(p)); c["case"] = "ex-99"
json.dump(c, open(p, "w"), indent=2)
PY
expect fail "unadmitted case" "$D"

# --- stale pins ------------------------------------------------------------------
D=$(seed stale-pin)
python3 - "$D/scripts/examples/ex-01.json" <<'PY'
import json, sys
p = sys.argv[1]; c = json.load(open(p))
c["pins"]["definition"]["sha256"] = "sha256:" + "0" * 64
json.dump(c, open(p, "w"), indent=2)
PY
expect fail "drifted sha256" "$D"

D=$(seed drifted-file)
echo x >> "$D/clients/ts/fixture-definition.json"
expect fail "edited pinned file" "$D"

# --- stale / missing displayed snapshot ------------------------------------------
D=$(seed stale-display)
sed 's|cargo test -p hharness --test r5_29 ex01|cargo test -p hharness --test r5_29 ex01 --stale|' \
  "$D/docs/getting-started.md" > "$D/docs/gs.tmp" && mv "$D/docs/gs.tmp" "$D/docs/getting-started.md"
expect fail "displayed command drifted from the card" "$D"

D=$(seed no-marker)
python3 - "$D/docs/getting-started.md" <<'PY'
import re, sys
p = sys.argv[1]; t = open(p).read()
t = re.sub(r"<!-- examples:begin -->.*?<!-- examples:end -->", "", t, flags=re.S)
open(p, "w").write(t)
PY
expect fail "displayed block absent" "$D"

# --- checker mapping ---------------------------------------------------------------
D=$(seed unmapped-check)
python3 - "$D/scripts/examples/ex-01.json" <<'PY'
import json, sys
p = sys.argv[1]; c = json.load(open(p)); c["checks"].append("not_a_runner_check")
json.dump(c, open(p, "w"), indent=2)
PY
expect fail "card check the runner never emits" "$D"

# --- unpinned fixture data -----------------------------------------------------------
D=$(seed stray-json)
echo '{"note":"stray"}' > "$D/scripts/examples/stray.json"
expect fail "committed fixture data no card pins" "$D"

# --- committed-HEAD vs working-tree --------------------------------------------------
D=$(seed head-dirty)
sed 's|cargo test -p hharness --test r5_29 ex01|cargo test -p hharness --test r5_29 ex01 --dirty|' \
  "$D/docs/getting-started.md" > "$D/docs/gs.tmp" && mv "$D/docs/gs.tmp" "$D/docs/getting-started.md"
expect ok "--head ignores working-tree dirt" --head "$D"
expect fail "working-tree mode sees the dirt" "$D"
git -C "$D" add -A && git -C "$D" commit -qm dirty >/dev/null 2>&1
expect fail "--head fails on committed drift" --head "$D"

# --- missing ci-mode annotation fails the step-mode lint -------------------------------
sed '/# ci-mode: both — reads committed displayed\/canonical example records/,+2d' \
  "$ROOT/.github/workflows/ci.yml" > "$TMP/noannot.yml"
if grep -q "Example cards" "$TMP/noannot.yml"; then
  if (cd "$ROOT" && bash scripts/ci/check-ci-gating.sh "$TMP/noannot.yml") >"$TMP/noannot.out" 2>&1; then
    bad "ci.yml minus the annotation passed check-ci-gating"
  else
    grep -q "Example cards" "$TMP/noannot.out" \
      && say "ok  missing ci-mode annotation rejected" \
      || bad "lint failed but did not name the Example cards step: $(cat "$TMP/noannot.out")"
  fi
else
  say "skip annotation-mutation leg (Example cards step not yet in ci.yml)"
fi

# --- the real checkout is green --------------------------------------------------------
expect ok "real checkout (working tree)" "$ROOT"

[ "$fails" -eq 0 ] || { echo "test-check-examples: $fails failure(s)" >&2; exit 1; }
echo "test-check-examples: OK"
