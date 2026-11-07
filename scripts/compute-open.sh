#!/usr/bin/env bash
# compute-open.sh — the open-compute index (manifest 266/R6.13; spec §3.9,
# REQ-XB-211 / REQ-XH-008's empty-output closure). READ-ONLY: lists
# provision receipts under the compute reports root that have no matching
# teardown receipt for the same phase+window. Empty output = zero open.
# Phase and round closure require it empty.
#
#   compute-open.sh [--reports DIR]   (default docs/build/reports/compute;
#                                    HH_COMPUTE_REPORTS_DIR overrides)
set -euo pipefail
cd "$(dirname "$0")/.."

REPO="$(pwd)"
REPORTS="${HH_COMPUTE_REPORTS_DIR:-$REPO/docs/build/reports/compute}"
case "${1:-}" in
  --reports) REPORTS="$2" ;;
  -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
  "") ;;
  *) echo "usage: compute-open.sh [--reports DIR]" >&2; exit 2 ;;
esac

[ -d "$REPORTS" ] || exit 0  # no receipts yet — zero open is the index

python3 -I - "$REPORTS" <<'PY'
import glob, json, os, sys
root = sys.argv[1]
def pw(path):
    try:
        d = json.load(open(path))
    except Exception:
        return None
    if d.get("schema") != "hh-compute-receipt/1":
        return None
    return (d.get("phase"), d.get("window"), d.get("kind"), path)
prov, torn = {}, set()
for p in sorted(glob.glob(os.path.join(root, "*.json"))):
    r = pw(p)
    if not r:
        continue
    ph, w, kind, path = r
    if kind == "provision":
        prov[(ph, w)] = path
    elif kind == "teardown":
        torn.add((ph, w))
for (ph, w), path in prov.items():
    if (ph, w) not in torn:
        print(f"{ph} | {w} | {path}")
PY
