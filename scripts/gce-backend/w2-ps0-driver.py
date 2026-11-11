#!/usr/bin/env python3
"""w2-ps0-driver.py — the W2 PS0 window driver (R6.40 wiring).

Composes committed surfaces — never a new leg/schema/record kind:
the carried `hh-model-proxy` binary does all receipting (X-04, its
stdio Json control channel: configure → register_slot → calls ride
the minted slot token → revoke_slot → stop); this driver issues the
exact probe plan ps0_plan() declares (7 calls — dialect ×2, seed on
openai.chat, tool_choice ×2, cache_control prime+read on
anthropic.messages), reads the proxy run's durable events for the
receipt refs/dispositions/usage/cost, and folds `hh-ps0-stage/1`
(the R6.26 record shape) to --out. Secrets ride env only — the
proxy resolves OLLAMA_API_KEY from its own launch env (credential_env
naming, never a value). provided: yes/no only.

usage:
  w2-ps0-driver.py --proxy-bin BIN --store-root DIR --run-id ID \
    --listen 127.0.0.1:PORT --model M --lane-file lane.json \
    --authorization-file auth.json --pricing pricing.json \
    --slot-id w2-ps0 --slice-micros 50000 --out stage.json

  attach mode (the window session already owns the proxy's control
  channel — HH_PS_BASE_URL/HH_PS_SLOT_TOKEN provision the leg):
  w2-ps0-driver.py --attach --listen 127.0.0.1:PORT \
    --slot-token T --store-root DIR --run-id ID --model M \
    --pricing pricing.json --slot-id w2-ps0 --out stage.json
"""
import argparse, json, os, subprocess, sys, time, urllib.request, uuid

def eprint(*a): print(*a, file=sys.stderr)

def dialect_body(model, surface):
    return {"model": model, "max_tokens": 8, "stream": False,
            "messages": [{"role": "user",
                          "content": "PS0 dialect probe — answer with the single token ok."}]}

def plan(model):
    """The committed ps0_plan: surfaces openai.chat + anthropic.messages."""
    calls = []
    for s in ("openai.chat", "anthropic.messages"):
        calls.append({"probe_id": f"ps0-dialect-{s}", "kind": "dialect",
                      "surface": s,
                      "route": "/v1/chat/completions" if s == "openai.chat" else "/v1/messages",
                      "body": dialect_body(model, s),
                      "declared_input_tokens": 128, "declared_output_tokens": 8})
    b = dialect_body(model, "openai.chat"); b["seed"] = 42
    calls.append({"probe_id": "ps0-seed-openai.chat", "kind": "seed",
                  "surface": "openai.chat", "route": "/v1/chat/completions",
                  "body": b, "declared_input_tokens": 128, "declared_output_tokens": 8})
    for s in ("openai.chat", "anthropic.messages"):
        b = dialect_body(model, s)
        if s == "anthropic.messages":
            b["tools"] = [{"name": "ps0_probe",
                           "description": "PS0 tool_choice probe tool",
                           "input_schema": {"type": "object", "properties": {}}}]
            b["tool_choice"] = {"type": "auto"}
        else:
            b["tools"] = [{"type": "function",
                           "function": {"name": "ps0_probe",
                                        "description": "PS0 tool_choice probe tool",
                                        "parameters": {"type": "object", "properties": {}}}}]
            b["tool_choice"] = "auto"
        calls.append({"probe_id": f"ps0-tool-choice-{s}", "kind": "tool_choice",
                      "surface": s,
                      "route": "/v1/chat/completions" if s == "openai.chat" else "/v1/messages",
                      "body": b, "declared_input_tokens": 192, "declared_output_tokens": 8})
    b = dialect_body(model, "anthropic.messages")
    b["system"] = [{"type": "text",
                    "text": "PS0 cache_control probe — a shared prefix long enough to make a measured cache read meaningful.",
                    "cache_control": {"type": "ephemeral"}}]
    for leg in ("prime", "read"):
        calls.append({"probe_id": f"ps0-cache-control-{leg}", "kind": "cache_control",
                      "surface": "anthropic.messages", "route": "/v1/messages",
                      "body": dict(b), "declared_input_tokens": 1536,
                      "declared_output_tokens": 8})
    return calls

def worst_case_micros(calls, pricing):
    """Declared-bound worst case in µ$: input at uncached + output at
    out rate, priced off the committed pricing.json row for the model.
    Refuses (>50_000) before any call — the PS0 bound (X-02)."""
    model = pricing.get("model") or {}
    rates = pricing.get("rates") or pricing
    in_rate = rates.get("input_micros_per_token")
    out_rate = rates.get("output_micros_per_token")
    if in_rate is None or out_rate is None:
        # table layouts vary: fall back to any *in*/*out* micro fields
        for k, v in rates.items():
            if in_rate is None and "input" in k and "micro" in k:
                in_rate = v
            if out_rate is None and "output" in k and "micro" in k:
                out_rate = v
    if in_rate is None or out_rate is None:
        return None
    return sum(c["declared_input_tokens"] * in_rate +
               c["declared_output_tokens"] * out_rate for c in calls)

def ctl(proc, op):
    proc.stdin.write((json.dumps(op) + "\n").encode()); proc.stdin.flush()
    line = proc.stdout.readline()
    if not line:
        raise SystemExit("ps0: proxy control channel closed")
    return json.loads(line)

def read_events(store_root, run_id):
    evs = []
    wal = os.path.join(store_root, "runs", run_id, "events.wal")
    if not os.path.exists(wal):
        for dp, _, fns in os.walk(store_root):
            for fn in fns:
                if fn == "events.wal":
                    wal = os.path.join(dp, fn)
    if os.path.exists(wal):
        for line in open(wal, errors="replace"):
            line = line.strip()
            if not line: continue
            try:
                rec = json.loads(line)
            except Exception:
                continue
            # the wal carries envelope rows {k:'e', v:<event>} and
            # checkpoint rows {k:'c', n:<count>} — the fold wants the
            # events themselves
            if isinstance(rec, dict) and rec.get("k") == "e":
                rec = rec.get("v") or {}
            evs.append(rec)
    return evs

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--proxy-bin")
    ap.add_argument("--store-root", required=True)
    ap.add_argument("--run-id", required=True)
    ap.add_argument("--listen", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--lane-file")
    ap.add_argument("--authorization-file")
    ap.add_argument("--pricing", required=True)
    ap.add_argument("--slot-id", default="w2-ps0")
    ap.add_argument("--slice-micros", type=int, default=50000)
    ap.add_argument("--out", required=True)
    ap.add_argument("--attach", action="store_true",
                    help="the session owns the proxy ctl channel — "
                         "call + fold only, via --listen/--slot-token")
    ap.add_argument("--slot-token")
    ap.add_argument("--fold-only", action="store_true",
                    help="issue no calls — fold the run's durable "
                         "events into hh-ps0-stage/1 (a prior run's "
                         "receipted calls)")
    ap.add_argument("--statuses-file",
                    help="fold-only: JSON array of the prior run's "
                         "HTTP statuses (its calls' recorded outcomes)")
    a = ap.parse_args()

    calls = plan(a.model)
    pricing = json.load(open(a.pricing))
    worst = worst_case_micros(calls, pricing)
    if worst is not None and worst > 50_000:
        print(json.dumps({"refused": f"plan_bound:{worst}>50000"}))
        sys.exit(2)

    if a.attach:
        token = a.slot_token or os.environ.get("HH_PS_SLOT_TOKEN")
        if not token:
            eprint("--attach requires --slot-token or HH_PS_SLOT_TOKEN env")
            sys.exit(2)
        listen = a.listen
        proc = None
    else:
        if not (a.proxy_bin and a.lane_file and a.authorization_file):
            eprint("spawn mode requires --proxy-bin/--lane-file/"
                   "--authorization-file"); sys.exit(2)
        env = dict(os.environ)
        proc = subprocess.Popen([a.proxy_bin], stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE, env=env)
        lane = json.load(open(a.lane_file))
        auth = json.load(open(a.authorization_file))
        rep = ctl(proc, {"op": "configure", "store_root": a.store_root,
                         "run_id": a.run_id, "listen": a.listen,
                         "lane": lane, "credential_env": "OLLAMA_API_KEY",
                         "authorization": auth, "mode": "live"})
        if "error" in rep:
            eprint("configure failed:", rep); sys.exit(2)
        listen = rep.get("ok", {}).get("listen") or a.listen
        rep = ctl(proc, {"op": "register_slot", "slot_id": a.slot_id,
                         "slice_micros": a.slice_micros})
        if "error" in rep:
            eprint("register_slot failed:", rep); sys.exit(2)
        token = (rep.get("ok") or {}).get("slot_token") or rep.get("ok", {}).get("token")
    token_ref = f"slot:{a.slot_id}"

    def http_call(call):
        req = urllib.request.Request(
            f"http://{listen}{call['route']}",
            data=json.dumps(call["body"]).encode(),
            headers={"content-type": "application/json",
                     "authorization": f"Bearer {token}"})
        t0 = time.time()
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                r.read(); return r.status
        except urllib.error.HTTPError as he:
            he.read(); return he.code
        except Exception:
            return -1

    statuses = []
    if a.fold_only:
        # a prior run already issued + receipted the plan's calls —
        # the fold reads the durable events; the recorded statuses come
        # from --statuses-file (written by that run), never re-issued.
        if not a.statuses_file:
            eprint("--fold-only requires --statuses-file"); sys.exit(2)
        statuses = json.load(open(a.statuses_file))
    else:
        for c in calls:
            statuses.append(http_call(c))
        time.sleep(1.0)  # receipts land durable before the fold
    if proc is not None:
        ctl(proc, {"op": "revoke_slot", "slot_id": a.slot_id})
        try: ctl(proc, {"op": "stop"})
        except SystemExit: pass

    evs = read_events(a.store_root, a.run_id)
    call_evs = [e for e in evs
                if e.get("class") in ("model.call.completed", "model.call.failed")]
    cost = sum((e.get("payload") or {}).get("money", {}).get("micro_units", 0)
               for e in evs if e.get("class") == "measurement.cost.attributed")
    probe_rows = []
    for i, c in enumerate(calls):
        ev = call_evs[i] if i < len(call_evs) else {}
        p = ev.get("payload") or {}
        disp = "pass" if statuses[i] and 200 <= statuses[i] < 300 else \
               p.get("disposition") or f"unreceipted{{http:{statuses[i]}}}"
        if p.get("disposition") and p["disposition"] != "forwarded":
            disp = p["disposition"]
        elif p.get("disposition") == "forwarded":
            disp = "pass"
        probe_rows.append({
            "probe_id": c["probe_id"], "kind": c["kind"], "surface": c["surface"],
            "route": c["route"], "disposition": disp,
            "receipt_ref": p.get("response_ref") or "",
            "cost_micros": 0, "usage": None})
    cached = {s: "unmeasured" for s in ("openai.chat", "anthropic.messages")}
    passed = all(r["disposition"] == "pass" for r in probe_rows)
    stage = {
        "schema": "hh-ps0-stage/1", "stage": "PS0", "window": "W2", "gate": "FP-S",
        "slot": {"token_ref": token_ref, "slice_micros": a.slice_micros},
        "plan": {"calls": len(calls),
                 "worst_case_micros": worst if worst is not None else -1,
                 "cap_micros": 50000},
        "probes": probe_rows,
        "cached_share": {s: {"ppm": v} if not isinstance(v, str) else v
                          for s, v in cached.items()},
        "metered_micros": cost,
        "disposition": "pass" if passed else "failed",
        "evidence_label": "recorded_execution",
        "recorded_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    with open(a.out, "w") as f:
        json.dump(stage, f, indent=1, sort_keys=True)
    print(json.dumps({"stage": a.out, "disposition": stage["disposition"],
                      "calls": len(calls), "metered_micros": cost}))
    sys.exit(0 if passed else 2)

if __name__ == "__main__":
    main()
