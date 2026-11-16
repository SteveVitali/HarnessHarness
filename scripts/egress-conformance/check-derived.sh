#!/usr/bin/env bash
# check-derived.sh — the drift guard for the mechanical shape
# inventory: regenerates scripts/egress-conformance/derived/ into a
# scratch dir and diffs it against the committed bytes (offline, $0).
set -euo pipefail
cd "$(dirname "$0")/../.."
REPO="$PWD"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT

python3 "$REPO/scripts/egress-conformance/extract.py" "$REPO" "$T" >/dev/null

BIN="$REPO/target/debug/hh_w1"
if [ ! -x "$BIN" ]; then
  cargo build -q -p hharness-runner --bin hh_w1
fi
"$BIN" scan-hosts --list "$T/scan-inputs.txt" > "$T/scanned-hosts.txt"

rc=0
for f in members.json test-scripts.txt logical-lines.txt bootstrap.txt \
         apt-install.txt pip.txt uvx.txt scan-inputs.txt scanned-hosts.txt; do
  if ! diff -q "$REPO/scripts/egress-conformance/derived/$f" "$T/$f" >/dev/null 2>&1; then
    echo "check-derived: $f drifted — run scripts/egress-conformance/extract.sh" >&2
    diff "$REPO/scripts/egress-conformance/derived/$f" "$T/$f" | head -20 >&2 || true
    rc=1
  fi
done
[ "$rc" -eq 0 ] && echo "check-derived: OK — the shape inventory matches the committed member tests/*"
exit "$rc"
