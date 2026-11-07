#!/usr/bin/env bash
# compute-gate-span.sh — the REQ-XH-009 gate-span check (manifest
# 266/R6.13; spec §3.9). READ-ONLY: a provisioning window whose
# provision→teardown span crosses a gate row whose wait is not bounded
# inside its dead-man D fails, unless that gate's readout records that the
# VM stays up and why — a `vm_stays_up: <reason>` line in the readout.
#
#   compute-gate-span.sh --reports DIR --readouts DIR
#
# Each readout's gate time is the latest `20YY-MM-DDTHH:MM` UTC stamp it
# records (its own dated evidence). A gate inside (provision, teardown)
# requires the marker; a gate after teardown or before provision does not
# constrain the window.
set -euo pipefail
cd "$(dirname "$0")/.."

REPORTS="${HH_COMPUTE_REPORTS_DIR:-$(pwd)/docs/build/reports/compute}"
READOUTS="$(pwd)/docs/build/readouts"
while [ $# -gt 0 ]; do
  case "$1" in
    --reports) REPORTS="$2"; shift 2 ;;
    --readouts) READOUTS="$2"; shift 2 ;;
    -h|--help) sed -n '2,15p' "$0"; exit 0 ;;
    *) echo "usage: compute-gate-span.sh --reports DIR --readouts DIR" >&2; exit 2 ;;
  esac
done

[ -d "$REPORTS" ] || { echo "compute-gate-span: no reports root — nothing to check"; exit 0; }

python3 -I - "$REPORTS" "$READOUTS" <<'PY'
import glob, json, os, re, sys
root, rdir = sys.argv[1], sys.argv[2]
receipts = []
for p in sorted(glob.glob(os.path.join(root, "*.json"))):
    try:
        d = json.load(open(p))
    except Exception:
        continue
    if d.get("schema") == "hh-compute-receipt/1":
        receipts.append(d)
provs = {(r["phase"], r["window"]): r for r in receipts if r["kind"] == "provision"}
tors = {(r["phase"], r["window"]): r for r in receipts if r["kind"] == "teardown"}
ts = re.compile(r"20\d\d-\d\d-\d\dT\d\d:\d\d(?::\d\d)?Z")
fails = 0
for p in sorted(glob.glob(os.path.join(rdir, "*.md"))):
    text = open(p, encoding="utf-8", errors="replace").read()
    stamps = ts.findall(text)
    if not stamps:
        continue
    gate_t = max(stamps)
    stays = re.search(r"vm_stays_up:\s*(\S[^\n]*)", text)
    for (ph, w), prov in provs.items():
        start = prov["recorded_at"]
        tor = tors.get((ph, w))
        end = tor["recorded_at"] if tor else None
        if end is None or not (start < gate_t < end):
            continue
        # the window's span crosses this gate's wait — the readout must
        # record that the VM stays up and why (REQ-XH-009)
        if not (stays and stays.group(1).strip() not in ("no", "none", "-")):
            print(f"compute-gate-span: FAIL — {ph}/{w} spans {os.path.basename(p)} "
                  f"({gate_t} inside {start}..{end}) without a vm_stays_up record")
            fails += 1
if fails:
    sys.exit(1)
print("compute-gate-span: OK — no window spans an unbounded gate wait")
PY
