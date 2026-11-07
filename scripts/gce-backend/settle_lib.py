#!/usr/bin/env python3
# settle_lib.py — the controller-side settlement act + telemetry poller
# (manifest 267/R6.14; spec §4.7; REQ-XC-006/121/127; classes registered
# once in classes.rs by R6.10). Stdlib only; `python3 -I`.
#
#   settle_lib.py settle <ledger> <window> <hour_boundary> <now>
#       <usage1.json> <usage2.json> <balance_pre.json> <balance_post.json>
#       <hh.json> [--non-exclusive]
#       fold + append measurement.provider.usage.observed (one per read)
#       and measurement.cost.reconciled{window, hh, provider, diff, status}
#       rows to the phase spend ledger. `settled` before the REQ-XC-127
#       clock (>=2h after the UTC hour boundary) refuses; agreeing stale
#       reads before the clock stay pending.
#   settle_lib.py ratelimit <req_ts1> <req_ts2>
#       REQ-XC-006: combined /api/usage + /api/balance <= 1/min.
#   settle_lib.py retry-after <now> <retry_after_seconds|-> <status>
#       429 honours Retry-After; 503 tolerated.
import datetime
import json
import sys

S_PER_MIN = 60
CLOCK_DELAY_S = 2 * 3600          # REQ-XC-127: >= 2 h past the boundary
TOL_PCT = 0.02                    # max(2 %, $0.05)
TOL_MICROS = 50_000
BOUNDARY_S = 60                   # ±60 s hour-boundary flag


def refuse(reason):
    print(f"Refused{{settle.{reason}}}", file=sys.stderr)
    sys.exit(2)


def load(p):
    with open(p, "r", encoding="utf-8") as f:
        return json.load(f)


def parse_utc(s):
    if len(s) in (16, 17):          # minute-precision UTC → pad seconds
        s = s.rstrip("Z") + ":00Z"
    return datetime.datetime.strptime(s[:19], "%Y-%m-%dT%H:%M:%S")


def secs(a, b):
    return (parse_utc(b) - parse_utc(a)).total_seconds()


def usage_total(u):
    return int(u.get("total_micros", u.get("micros", 0)))


def cmd_ratelimit(argv):
    # exit 0 when req2 may fire at req_ts2 given req_ts1 (combined limit)
    if secs(argv[2], argv[3]) >= S_PER_MIN:
        print("ratelimit: allowed")
        return
    wait = S_PER_MIN - secs(argv[2], argv[3])
    print(f"ratelimit: wait {int(wait)}s")
    sys.exit(2)


def cmd_retry_after(argv):
    now, ra, status = argv[2], argv[3], argv[4]
    if status == "429" and ra != "-":
        print(f"retry-after: hold {ra}s")
        return
    if status == "503":
        print("retry-after: 503 tolerated")
        return
    print("retry-after: ok")


def cmd_settle(argv):
    (ledger, window, boundary, now, u1p, u2p, bprep, bpostp, hhp) = argv[2:11]
    non_exclusive = "--non-exclusive" in argv[11:]
    for p, name in ((bprep, "balance_pre"), (bpostp, "balance_post")):
        if p == "-":
            refuse(f"{name}_snapshot_required")
    u1, u2 = load(u1p), load(u2p)
    bpre, bpost = load(bprep), load(bpostp)
    hh = load(hhp)
    gap = secs(boundary, now)
    clock_run = gap >= CLOCK_DELAY_S
    # reads' timestamps and gap (two consecutive agreeing reads >= 1 min)
    t1, t2 = u1["read_at"], u2["read_at"]
    agree = usage_total(u1) == usage_total(u2)
    min_apart = secs(t1, t2) >= S_PER_MIN
    provider = usage_total(u2)
    hh_m = int(hh["micro_units"])
    diff = provider - hh_m
    tol = max(TOL_MICROS, int(abs(hh_m) * TOL_PCT))
    # boundary flag: any dispatched call within ±60 s of an hour boundary
    flags = []
    for ts in hh.get("call_utc", []):
        sec_of_hour = parse_utc(ts).second + parse_utc(ts).minute * 60
        if sec_of_hour <= BOUNDARY_S or sec_of_hour >= 3600 - BOUNDARY_S:
            flags.append("boundary_ambiguous")
            break
    conventions = {
        "dispatched": hh.get("requests_dispatched", hh.get("requests", 0)),
        "successful": hh.get("requests_successful", hh.get("requests", 0)),
    }
    if non_exclusive:
        status = "unreconcilable{non_exclusive}"
    elif not clock_run:
        status = "pending"          # agreeing stale reads stay pending
    elif not (agree and min_apart):
        status = "pending"
    elif abs(diff) <= tol:
        status = "settled"
    else:
        status = "settled_discrepant"
    ts = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    rows = []
    for tag, u in (("usage-1", u1), ("usage-2", u2)):
        rows.append({
            "type": "provider_usage",
            "class": "measurement.provider.usage.observed",
            "ts": u.get("read_at", ts), "phase": hh.get("phase"),
            "window": window,
            "endpoint": u.get("endpoint", "/api/usage"),
            "query": u.get("query", "range=24h&group_by=model"),
            "read_ms": u.get("read_ms", 0),
            "status": u.get("status", 200),
            "body": u.get("body", u),
        })
    for tag, b in (("balance-pre", bpre), ("balance-post", bpost)):
        rows.append({
            "type": "provider_usage",
            "class": "measurement.provider.usage.observed",
            "ts": b.get("read_at", ts), "phase": hh.get("phase"),
            "window": window,
            "endpoint": "/api/balance", "query": "",
            "read_ms": b.get("read_ms", 0),
            "status": b.get("status", 200),
            "body": b.get("body", b),
        })
    rows.append({
        "type": "cost_reconciled",
        "class": "measurement.cost.reconciled",
        "ts": ts, "phase": hh.get("phase"), "window": window,
        "hh": {"requests": hh.get("requests", 0),
               "input_total": hh.get("input_total", 0),
               "cache_read": hh.get("cache_read", 0),
               "output_total": hh.get("output_total", 0),
               "micro_units": hh_m},
        "provider": {"micro_units": provider,
                     "hourly_buckets": u2.get("hourly_buckets", [])},
        "diff_micros": diff,
        "tolerance_micros": tol,
        "boundary_flags": flags,
        "conventions": conventions,
        "balance": {"pre": bpre.get("body", bpre),
                    "post": bpost.get("body", bpost)},
        "status": status,
    })
    if status == "settled" and not clock_run:
        refuse("settled_before_clock")
    if status == "settled" and clock_run and gap < CLOCK_DELAY_S:
        refuse("settled_before_clock")
    import os
    here = os.path.dirname(os.path.abspath(__file__))
    import importlib.util
    spec = importlib.util.spec_from_file_location(
        "cost_lib", os.path.join(here, "cost_lib.py"))
    cl = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(cl)
    cl.spend_append_rows(ledger, rows)
    print(f"settlement: {status} (window {window})")


OPS = {
    "ratelimit": cmd_ratelimit,
    "retry-after": cmd_retry_after,
    "settle": cmd_settle,
}


def main():
    if len(sys.argv) < 2 or sys.argv[1] not in OPS:
        print("usage: settle_lib.py <ratelimit|retry-after|settle> …",
              file=sys.stderr)
        sys.exit(1)
    OPS[sys.argv[1]](sys.argv)


if __name__ == "__main__":
    main()
