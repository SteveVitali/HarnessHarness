#!/usr/bin/env bash
# check-live-evidence.sh — the hh-live-evidence/1 validator (R4.5a /
# ADR-0363 D9). Runs on a ci-mode: both step so docs-only heads are
# checked too. Invariants only (BM-TEST-01): record shape, the
# response-header allowlist, record↔spend-row correspondence, cumulative
# spend non-decreasing and ≤ cap, allocation table sums ≤ cap. It never
# pins a count, a date or a record's content.
set -euo pipefail
cd "$(dirname "$0")/.."
LIVE="${HH_LIVE_DIR:-docs/build/reports/live}"  # HH_LIVE_DIR: tests only

[ -d "$LIVE" ] || { echo "check-live-evidence: no $LIVE dir — nothing to validate"; exit 0; }
command -v python3 >/dev/null || { echo "check-live-evidence: python3 absent" >&2; exit 2; }

LIVE_DIR="$LIVE" python3 - <<'PY'
import glob, hashlib, json, os, re, sys

live = os.environ["LIVE_DIR"]
fails = []
def bad(msg): fails.append(msg)

ALLOWED_HDR = {"request-id", "x-request-id", "cf-ray", "openai-model",
               "openai-processing-ms", "openai-version", "x-served-model",
               "x-processing-time", "x-processing-time-ms", "retry-after"}
FORBIDDEN = ("authorization", "cookie", "set-cookie", "proxy-authenticate",
             "www-authenticate")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
HEX40 = re.compile(r"^[0-9a-f]{40}$")
ISOTS = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")

# ── pricing.json ────────────────────────────────────────────────────────
pricing_path = os.path.join(live, "pricing.json")
cap = None
if not os.path.exists(pricing_path):
    bad("pricing.json absent")
else:
    pricing = json.load(open(pricing_path))
    if pricing.get("schema") != "hh-live-pricing/1":
        bad("pricing.json: schema != hh-live-pricing/1")
    cap = pricing.get("cap_usd")
    if not isinstance(cap, (int, float)) or cap <= 0:
        bad("pricing.json: cap_usd not a positive number")
        cap = None
    alloc = pricing.get("allocations_usd", {})
    if not isinstance(alloc, dict) or not all(
        isinstance(v, (int, float)) and v >= 0 for v in alloc.values()):
        bad("pricing.json: allocations_usd must be non-negative numbers")
    elif cap is not None and sum(alloc.values()) > cap:
        bad("pricing.json: allocations sum %s exceeds cap %s"
            % (sum(alloc.values()), cap))
    if not isinstance(pricing.get("models"), dict):
        bad("pricing.json: models table absent")
    if not isinstance(pricing.get("retrieved"), str) \
            or not re.match(r"^\d{4}-\d{2}-\d{2}$", pricing["retrieved"]):
        bad("pricing.json: retrieved not a date")

# ── SPEND.jsonl ─────────────────────────────────────────────────────────
spend_path = os.path.join(live, "SPEND.jsonl")
spend_records = set()
cumulative = 0.0
if os.path.exists(spend_path):
    for i, line in enumerate(open(spend_path), 1):
        line = line.strip()
        if not line:
            continue
        try:
            row = json.loads(line)
        except Exception:
            bad(f"SPEND.jsonl:{i}: not JSON"); continue
        for f in ("ts", "ticket", "leg", "cost_usd", "cumulative_usd", "record"):
            if f not in row:
                bad(f"SPEND.jsonl:{i}: missing {f}")
        if not isinstance(row.get("cost_usd"), (int, float)) or row["cost_usd"] < 0:
            bad(f"SPEND.jsonl:{i}: cost_usd negative/non-number")
        if row.get("cumulative_usd", -1) + 1e-9 < cumulative:
            bad(f"SPEND.jsonl:{i}: cumulative_usd decreased")
        cumulative = row.get("cumulative_usd", cumulative)
        if "record" in row:
            spend_records.add(row["record"])
    if cap is not None and cumulative > cap + 1e-9:
        bad(f"SPEND.jsonl: cumulative {cumulative} exceeds cap {cap}")

# ── evidence records ────────────────────────────────────────────────────
records = sorted(glob.glob(os.path.join(live, "*.json")))
records = [p for p in records if os.path.basename(p) != "pricing.json"]
seen = set()
for path in records:
    base = os.path.basename(path)
    seen.add(base)
    try:
        rec = json.load(open(path))
    except Exception:
        bad(f"{base}: not JSON"); continue
    if rec.get("schema") != "hh-live-evidence/1":
        bad(f"{base}: schema != hh-live-evidence/1"); continue
    if not ISOTS.match(rec.get("recorded_at", "")):
        bad(f"{base}: recorded_at not an ISO Z timestamp")
    for f in ("ticket", "leg", "provider", "result", "git_head", "harness"):
        if not isinstance(rec.get(f), str) or not rec[f]:
            bad(f"{base}: {f} missing/not a nonempty string")
    if not HEX40.match(rec.get("git_head", "")):
        bad(f"{base}: git_head not a sha")
    host = rec.get("endpoint_host")
    if host is not None and ("/" in host or "@" in host or " " in host):
        bad(f"{base}: endpoint_host carries path/userinfo material")
    prov = rec.get("provided")
    if not isinstance(prov, dict) or not all(v in ("yes", "no") for v in prov.values()):
        bad(f"{base}: provided map values must be yes/no")
    if rec.get("transcript_sha256") is not None \
            and not HEX64.match(rec["transcript_sha256"]):
        bad(f"{base}: transcript_sha256 not 64-hex")
    tp = rec.get("transcript_path")
    if tp is not None:
        found = tp if os.path.exists(tp) else os.path.join(live, os.path.basename(tp))
        if not os.path.exists(found):
            bad(f"{base}: transcript_path {tp} not committed")
        else:
            if HEX64.match(rec.get("transcript_sha256", "")):
                digest = hashlib.sha256(open(found, "rb").read()).hexdigest()
                if digest != rec["transcript_sha256"]:
                    bad(f"{base}: transcript sha mismatch")
            if os.path.getsize(found) > 65536:
                bad(f"{base}: committed transcript exceeds 64 KiB")
    if not isinstance(rec.get("cost_usd"), (int, float)) or rec["cost_usd"] < 0:
        bad(f"{base}: cost_usd negative/non-number")
    if not isinstance(rec.get("spend_cumulative_usd"), (int, float)):
        bad(f"{base}: spend_cumulative_usd not a number")
    hdrs = rec.get("response_headers", {})
    if not isinstance(hdrs, dict):
        bad(f"{base}: response_headers not an object")
    else:
        for k in hdrs:
            kl = k.lower()
            ok = kl in ALLOWED_HDR or kl.startswith(("x-ratelimit-", "ratelimit-"))
            if not ok:
                bad(f"{base}: response header {k} outside the ADR-0363 allowlist")
            if kl in FORBIDDEN or kl.endswith(("-key", "-token", "-secret")):
                bad(f"{base}: forbidden response header {k}")
    if not isinstance(rec.get("request_ids"), list):
        bad(f"{base}: request_ids not a list")
    if not isinstance(rec.get("assertions"), list):
        bad(f"{base}: assertions not a list")
    rh = rec.get("run_ledger_head")
    if rh is not None and not (isinstance(rh, dict) and HEX64.match(rh.get("sha256", ""))):
        bad(f"{base}: run_ledger_head malformed")

# record ↔ spend-row correspondence (CC3 — nothing unaccounted)
for base in seen - spend_records:
    bad(f"record {base} has no SPEND.jsonl row")
for base in spend_records - seen:
    bad(f"SPEND.jsonl row names uncommitted record {base}")

if fails:
    for f in fails:
        print(f"check-live-evidence: FAIL — {f}", file=sys.stderr)
    sys.exit(1)
print(f"check-live-evidence: OK — {len(records)} record(s), "
      f"cumulative spend ${cumulative:.4f}, cap enforced")
PY
