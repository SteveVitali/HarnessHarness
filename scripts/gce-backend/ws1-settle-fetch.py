#!/usr/bin/env python3
"""ws1-settle-fetch.py — Ws1 settlement provider reads (R6.45q session
wiring). Reads OLLAMA_API_KEY at use from HH_OPERATOR_ENV_FILE (never
copied, `provided: yes` only) and performs ONE provider read per
invocation — the caller paces REQ-XC-006 (≤1 req/min combined). Wraps
the raw body into the settle_lib {read_at, endpoint, query, read_ms,
status, body, total_micros} shape. usage:
  ws1-settle-fetch.py balance out.json
  ws1-settle-fetch.py usage out.json
"""
import json, os, sys, time, urllib.request, urllib.error

def key():
    envf = os.environ.get("HH_OPERATOR_ENV_FILE", "")
    if not envf or not os.path.exists(envf):
        print("Refused{settle.env_file_absent}", file=sys.stderr); sys.exit(2)
    for line in open(envf):
        if line.startswith("OLLAMA_API_KEY="):
            return line.split("=", 1)[1].strip()
    print("Refused{settle.key_unprovided}", file=sys.stderr); sys.exit(2)

def main():
    which, out = sys.argv[1], sys.argv[2]
    if which == "balance":
        url = "https://ollama.com/api/balance"; endpoint = "/api/balance"; query = ""
    elif which == "usage":
        url = "https://ollama.com/api/usage?range=24h&group_by=model"
        endpoint = "/api/usage"; query = "range=24h&group_by=model"
    else:
        sys.exit(1)
    req = urllib.request.Request(url, headers={
        "authorization": "Bearer " + key(), "accept": "application/json"})
    t0 = time.time(); status = -1; raw = b""
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            raw = r.read(); status = r.status
    except urllib.error.HTTPError as he:
        raw = he.read(); status = he.code
    except Exception as e:
        status = -1; raw = json.dumps({"error": str(e)}).encode()
    ms = int((time.time() - t0) * 1000)
    try:
        body = json.loads(raw.decode("utf-8", "replace"))
    except Exception:
        body = {"raw_unparsed": raw.decode("utf-8", "replace")[:2000]}
    # fold a total_micros when the body shape allows — else None
    total = None
    if isinstance(body, dict):
        for k in ("total_micros", "micros", "total_usd_micros"):
            if isinstance(body.get(k), (int, float)):
                total = int(body[k]); break
        if total is None and isinstance(body.get("models"), list):
            s = 0
            for m in body["models"]:
                if isinstance(m, dict) and isinstance(m.get("cost_micros"), (int, float)):
                    s += int(m["cost_micros"])
            total = s or None
    rec = {"read_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
           "endpoint": endpoint, "query": query, "read_ms": ms,
           "status": status, "body": body}
    if total is not None:
        rec["total_micros"] = total
    with open(out, "w") as f:
        json.dump(rec, f, indent=1, sort_keys=True)
    print(json.dumps({"status": status, "read_ms": ms, "out": out}))

if __name__ == "__main__":
    main()
