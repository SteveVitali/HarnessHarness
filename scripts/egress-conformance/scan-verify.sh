#!/usr/bin/env bash
# scan-verify.sh — the D16 mechanical scan check: `hh-w1 scan-hosts`
# (the committed scan_hosts_in_text) over every committed member
# tests/* file, diffed against a freeze record's verify set.
#   scan-verify.sh [freeze.json]   (default: newest committed epoch)
# Prints the scanned set, the verify-minus-scanned remainder (agent
# base + mirror/oracle-observed — expected), and FAILS on any scanned
# host absent from verify (a derivation gap; R-6).
set -euo pipefail
cd "$(dirname "$0")/../.."
REPO="$PWD"

FREEZE="${1:-}"
if [ -z "$FREEZE" ]; then
  FREEZE="$(ls -1 "$REPO"/docs/build/reports/*-fp-egress-allowlist-freeze-epoch-*.json | sort | tail -1)"
fi
[ -f "$FREEZE" ] || { echo "scan-verify: no freeze file" >&2; exit 2; }

BIN="$REPO/target/debug/hh_w1"
if [ ! -x "$BIN" ]; then
  cargo build -q -p hharness-runner --bin hh_w1
fi

T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
# The committed scan set — the extractor's scan-inputs, regenerated so
# a member-set change can never be silently skipped.
python3 "$REPO/scripts/egress-conformance/extract.py" "$REPO" "$T" >/dev/null
"$BIN" scan-hosts --list "$T/scan-inputs.txt" > "$T/scanned.txt"

python3 - "$FREEZE" "$T/scanned.txt" <<'PY'
import json, sys
freeze, scanned_path = sys.argv[1], sys.argv[2]
f = json.load(open(freeze))
verify = set(f["list"]["verify"])
scanned = {l.strip() for l in open(scanned_path) if l.strip()}
missing = sorted(scanned - verify)
print(f"scan-verify: freeze {freeze}")
print(f"scan-verify: protocol_id={f['protocol_id']} epoch={f['epoch']}")
print(f"scan-verify: scanned {len(scanned)} hosts: {sorted(scanned)}")
print(f"scan-verify: verify-not-scanned {len(verify - scanned)} "
      f"(agent base ∪ mirror/oracle-observed): {sorted(verify - scanned)}")
if missing:
    print(f"scan-verify: GAP — scanned hosts absent from verify: {missing}")
    sys.exit(1)
print("scan-verify: OK — every scanned host is granted in verify")
PY
