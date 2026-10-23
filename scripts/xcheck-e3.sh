#!/usr/bin/env bash
# xcheck-e3 — the E3 (TypeScript) foreign replay leg (R4.4; R4.5's CI leg
# reuses it). Runs `node foreign/e3-typescript/foreign.ts <bundle>
# <answers>` then `hh-xcheck verify <bundle> <answers>` and reports the
# covered vs pending cells.
#
# The E3 tree (R4.4 + R4.4a) answers seven of the eight bundle arms —
# every arm but `hir-documents`; `verify` reports the R4.4b-owned cells
# (hir-documents, five cases) `missing answer` by name — recorded
# pending, never a fabricated pass. A FAIL naming an arm the E3 tree
# *did* answer, or anything but `missing answer` on a pending arm, is a
# real failure (exit 1).
#
# Usage: xcheck-e3.sh [--answers PATH]
#   --answers PATH   write the replayed answers here instead of a tempdir
#
# Exit 0 = every answered case verified and only pending-arm cells
#          reported missing; 3 = skipped(node-absent) or node < 22 (the
#          honest skip); 2 = hh-xcheck binary missing; 1 = a leg failed.
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
BUNDLE="$ROOT/crates/hh-xcheck/fixtures/cross-impl"
XCHECK="${HH_XCHECK_BIN:-$ROOT/target/debug/hh-xcheck}"
ANSWERS=""

while [ $# -gt 0 ]; do
  case "$1" in
    --answers) ANSWERS="$2"; shift 2 ;;
    *) echo "xcheck-e3: unknown argument $1" >&2; exit 2 ;;
  esac
done

if ! command -v node >/dev/null 2>&1; then
  echo "xcheck-e3: skipped(node-absent) — node >= 22 required" >&2
  exit 3
fi
NODE_MAJOR="$(node -e 'process.stdout.write(process.versions.node.split(".")[0])')"
if [ "$NODE_MAJOR" -lt 22 ]; then
  echo "xcheck-e3: skipped(node-absent) — node >= 22 required, found $(node --version)" >&2
  exit 3
fi
if [ ! -x "$XCHECK" ]; then
  echo "xcheck-e3: $XCHECK missing — run 'cargo build -p hh-xcheck' first" >&2
  exit 2
fi

TMP="$(mktemp -d "${TMPDIR:-/tmp}/hh-xcheck-e3.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
if [ -z "$ANSWERS" ]; then
  ANSWERS="$TMP/answers-e3.json"
fi

node "$ROOT/foreign/e3-typescript/foreign.ts" "$BUNDLE" "$ANSWERS"

# The arm ids the E3 tree answered — read from the answers file itself
# (node is present; the file is canonical JSON).
ANSWERED="$(node -e '
  const a = JSON.parse(require("node:fs").readFileSync(process.argv[1], "utf8"));
  process.stdout.write(Object.keys(a.arms ?? {}).join("\n"));
' "$ANSWERS")"

VERIFY_OUT="$TMP/verify.out"
VERIFY_STATUS=0
"$XCHECK" verify "$BUNDLE" "$ANSWERS" >"$VERIFY_OUT" 2>&1 || VERIFY_STATUS=$?
cat "$VERIFY_OUT"
# verify's exit contract: 0 all-green, 1 named case failures, anything
# else = the check itself could not run (read/parse/bundle error) —
# never read a non-result as a pass.
if [ "$VERIFY_STATUS" -gt 1 ]; then
  echo "xcheck-e3: verify could not run (exit $VERIFY_STATUS)" >&2
  exit 1
fi

# Every FAIL must be a `missing answer` on an arm the tree did NOT answer
# (the pending R4.4b cells). A covered-arm fail, or a non-"missing answer"
# fail anywhere, is a real failure.
FAILS=0
PENDING=0
while IFS= read -r line; do
  case "$line" in
    FAIL*) ;;
    *) continue ;;
  esac
  arm="$(printf '%s' "$line" | awk '{print $3}')"
  case "$line" in
    *"missing answer"*)
      if printf '%s\n' "$ANSWERED" | grep -qx "$arm"; then
        FAILS=$((FAILS + 1))
      else
        PENDING=$((PENDING + 1))
      fi
      ;;
    *) FAILS=$((FAILS + 1)) ;;
  esac
done <"$VERIFY_OUT"

echo "xcheck-e3: covered-arm failures: $FAILS; pending (R4.4b) cells missing: $PENDING"
if [ "$FAILS" -gt 0 ]; then
  echo "xcheck-e3: FAIL — $FAILS failure(s) outside the pending arms" >&2
  exit 1
fi
echo "xcheck-e3: ok — every answered case verified; $PENDING pending cells recorded (R4.4b)"
