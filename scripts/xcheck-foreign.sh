#!/usr/bin/env bash
# xcheck-foreign — the committed two-toolchain foreign replay leg (R4.5,
# DF-S0.3-2 / M-S2-5). Three legs over the committed hh-xcheck-bundle/1:
#   E1  hh-xcheck selfcheck <bundle>                      (in-tree replay)
#   E2  python3 foreign/e2-python/foreign.py <bundle> <answers>  → verify
#   E3  node    foreign/e3-typescript/foreign.ts <bundle> <ans…> → verify
# The foreign answers are REGENERATED from the trees at run time — a
# committed answers.json is the record of a run, never this run's
# evidence.
#
# Honest skips follow the scripts/xcheck-e3.sh convention: a leg whose
# toolchain is absent reports `skipped(<tool>-absent)`; a `verify` exit
# > 1 means the check itself could not run — a real failure, never read
# as a pass. The run exits 3 whenever any leg skipped: on a runner where
# both toolchains are provisioned (the CI leg rides the existing
# build-test-drift job) a skip is a red, not a pass.
#
# Usage: xcheck-foreign.sh
#   (no arguments; HH_XCHECK_BIN overrides the hh-xcheck binary path)
# Exit 0 = every leg executed and verified green
#      1 = a covered case failed, or a leg/verify could not run
#      2 = the hh-xcheck binary is missing
#      3 = at least one foreign leg skipped (toolchain absent)
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
BUNDLE="$ROOT/crates/hh-xcheck/fixtures/cross-impl"
XCHECK="${HH_XCHECK_BIN:-$ROOT/target/debug/hh-xcheck}"

if [ ! -x "$XCHECK" ]; then
  echo "xcheck-foreign: $XCHECK missing — run 'cargo build -p hharness-xcheck' first" >&2
  exit 2
fi

TMP="$(mktemp -d "${TMPDIR:-/tmp}/hh-xcheck-foreign.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

FAILED=0
SKIPPED=0

# The arm ids a generated answers file covers, via the leg's own
# toolchain ($1 = e2|e3, $2 = answers path).
list_arms() {
  case "$1" in
    e2)
      python3 -c 'import json, sys
print("\n".join(json.load(open(sys.argv[1])).get("arms", {})))' "$2"
      ;;
    e3)
      node -e '
        const a = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8"));
        process.stdout.write(Object.keys(a.arms ?? {}).join("\n"));
      ' "$2"
      ;;
  esac
}

# verify + classify one leg's regenerated answers ($1 = e2|e3, $2 =
# answers path): a FAIL on an arm the tree DID answer, or any
# non-"missing answer" FAIL, is a real failure; "missing answer" on an
# arm the tree has not answered is a recorded pending cell, reported as
# such rather than counted as a pass. Prints the classification line;
# returns 1 on a real failure.
verify_leg() {
  local label="$1" answers="$2"
  local out="$TMP/verify-$label.out" status=0
  "$XCHECK" verify "$BUNDLE" "$answers" >"$out" 2>&1 || status=$?
  cat "$out"
  if [ "$status" -gt 1 ]; then
    echo "xcheck-foreign: $label verify could not run (exit $status)" >&2
    return 1
  fi
  local answered fails=0 pending=0 arm line
  answered="$(list_arms "$label" "$answers")"
  while IFS= read -r line; do
    case "$line" in
      FAIL*) ;;
      *) continue ;;
    esac
    arm="$(printf '%s' "$line" | awk '{print $3}')"
    case "$line" in
      *"missing answer"*)
        if printf '%s\n' "$answered" | grep -qx "$arm"; then
          fails=$((fails + 1))
        else
          pending=$((pending + 1))
        fi
        ;;
      *) fails=$((fails + 1)) ;;
    esac
  done <"$out"
  echo "xcheck-foreign: $label covered-arm failures: $fails; pending cells missing: $pending"
  [ "$fails" -eq 0 ]
}

# ── leg E1: the in-tree reference replay ───────────────────────────────
echo "xcheck-foreign: leg E1 — hh-xcheck selfcheck"
if ! "$XCHECK" selfcheck "$BUNDLE"; then
  echo "xcheck-foreign: E1 self-check FAILED" >&2
  FAILED=1
fi

# ── leg E2 (python3) ───────────────────────────────────────────────────
echo "xcheck-foreign: leg E2 — python3 foreign/e2-python"
if ! command -v python3 >/dev/null 2>&1; then
  echo "xcheck-foreign: e2 skipped(python3-absent)" >&2
  SKIPPED=1
elif python3 "$ROOT/foreign/e2-python/foreign.py" "$BUNDLE" "$TMP/answers-e2.json" \
    && verify_leg e2 "$TMP/answers-e2.json"; then
  echo "xcheck-foreign: e2 ok"
else
  echo "xcheck-foreign: e2 FAILED" >&2
  FAILED=1
fi

# ── leg E3 (node ≥ 22) ─────────────────────────────────────────────────
echo "xcheck-foreign: leg E3 — node foreign/e3-typescript"
if ! command -v node >/dev/null 2>&1; then
  echo "xcheck-foreign: e3 skipped(node-absent) — node >= 22 required" >&2
  SKIPPED=1
elif [ "$(node -e 'process.stdout.write(process.versions.node.split(".")[0])')" -lt 22 ]; then
  echo "xcheck-foreign: e3 skipped(node-absent) — node >= 22 required, found $(node --version)" >&2
  SKIPPED=1
elif node "$ROOT/foreign/e3-typescript/foreign.ts" "$BUNDLE" "$TMP/answers-e3.json" \
    && verify_leg e3 "$TMP/answers-e3.json"; then
  echo "xcheck-foreign: e3 ok"
else
  echo "xcheck-foreign: e3 FAILED" >&2
  FAILED=1
fi

# ── verdict ────────────────────────────────────────────────────────────
if [ "$FAILED" -gt 0 ]; then
  echo "xcheck-foreign: FAIL" >&2
  exit 1
fi
if [ "$SKIPPED" -gt 0 ]; then
  echo "xcheck-foreign: SKIP — at least one foreign leg did not run (see skipped(…) above)" >&2
  exit 3
fi
echo "xcheck-foreign: ok — E1 self-check + E2 + E3 legs all verified"
