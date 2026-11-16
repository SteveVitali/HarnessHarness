#!/usr/bin/env bash
# extract.sh — regenerate scripts/egress-conformance/derived/ from the
# committed member tests/* (the mechanical shape inventory; R6.45k).
# Also refreshes derived/scanned-hosts.txt through `hh-w1 scan-hosts`
# (the committed scan_hosts_in_text — never a re-implementation).
set -euo pipefail
cd "$(dirname "$0")/../.."
REPO="$PWD"
OUT="${1:-$REPO/scripts/egress-conformance/derived}"

python3 "$REPO/scripts/egress-conformance/extract.py" "$REPO" "$OUT"

# The D16 scan set — the committed scan_hosts_in_text over every
# committed staging/tests/* file (built like live-verify's scan_binary).
BIN="$REPO/target/debug/hh_w1"
if [ ! -x "$BIN" ]; then
  cargo build -q -p hharness-runner --bin hh_w1
fi
"$BIN" scan-hosts --list "$OUT/scan-inputs.txt" > "$OUT/scanned-hosts.txt"
echo "extract: scanned-hosts.txt $(wc -l < "$OUT/scanned-hosts.txt" | tr -d ' ') hosts"
