#!/usr/bin/env python3
"""w4x-smoke-driver.py — the W4x PP0 replay smoke battery (ticket 325d;
ADR-0453; REQ-XC-324). Executes the §5.7 variant table at $0: one
replay-mode `hh-model-proxy` session per variant (the committed
`ws1-proxy-session.py --mode replay` control flow — no upstream, no
budget charge) and one `hh-w1 arm-run` cell per (arm × variant) — the
mediated hosted-arm lane verbatim, slot tokens on umask-077 files.

Inputs are the committed corpus: `battery.json` (variant/injection
table), the per-arm `hh.model_proxy.recording/1` documents, the task
dir and the r6_22 admission records. Per-variant recordings are
materialized from the arm's base recording under the declared
`injected` member (call-k `answer` replaces `upstream_response`;
`disposition` replaced where declared; `slot_id` re-spelled
`sm-<variant>-<arm>` so replay keying `sequence` binds one script).

Every (arm × variant) emits `hh-pp0-ag-evidence/1` (the Ws1 fold —
AG3/AG5/AG6/AG8 over the slot's own records: cell-report end cause,
mediator denied hosts, proxy receipts, endpoint errors). A leg that
cannot run records typed unavailability — never a pass.

The `--lane-file` `model` member MUST spell the recorded model
(`glm-5.3-flash` for the committed PP0 recordings) — receipts are
attributed under the lane's model and a non-priced spelling such as
`"replay"` mints `model.call` rows no evidence table can index.

usage:
  w4x-smoke-driver.py --cfg cfg.json --battery DIR --w1-bin BIN \
    --proxy-session ws1-proxy-session.py --proxy-bin hh-model-proxy \
    --admissions-dir DIR --layers-file F --tasks-dir DIR \
    --lane-file F --authorization-file F --store-root DIR \
    --run-id ID --listen 127.0.0.1:PORT --out DIR \
    [--arms pi,claude-code] [--variants SM-pos,...] \
    [--effort high] [--slot-slice-micros N] [--deadline-sec N]
"""
import argparse, copy, hashlib, json, os, subprocess, sys, time

def eprint(*a): print(*a, file=sys.stderr, flush=True)

def read_json(p):
    with open(p) as f:
        return json.load(f)

ARMS = ["pi", "claude-code"]
VARIANTS = ["SM-pos", "SM-429", "SM-402", "SM-slow", "SM-hang"]
ADMISSION_FILES = {"pi": "pi.json", "claude-code": "claude-code.json",
                   "mini-swe-agent": "mini-swe-agent.json"}


def slot_id(variant, arm):
    return f"sm-{variant.split('-', 1)[1].lower()}-{arm}"


def injected_recording(base, variant, sid, deadline_sec):
    """Materialize the variant's recording under battery `injected` —
    the declared transform, applied to the committed recording's own
    members (canonicalizer/keying come from the battery's replay
    block, not re-invented here)."""
    rec = copy.deepcopy(base)
    rec["slot_id"] = sid
    calls = rec.get("calls") or []
    inj = variant
    if inj == "SM-429" and calls:
        calls[0]["upstream_response"] = {
            "status": 429, "headers": {"retry-after": "1"},
            "body": {"utf8": "{\"error\":{\"message\":\"rate limited\",\"type\":\"rate_limit_error\"}}"}}
        calls[0]["released"] = copy.deepcopy(calls[0]["upstream_response"])
        calls[0]["disposition"] = "forwarded"
    elif inj == "SM-402" and len(calls) > 1:
        calls[1]["disposition"] = "refused_budget"
        calls[1]["upstream_response"] = {
            "status": 402, "headers": {},
            "body": {"utf8": "{\"error\":{\"message\":\"slot budget exhausted\",\"type\":\"budget_refused\"}}"}}
        calls[1]["released"] = copy.deepcopy(calls[1]["upstream_response"])
    elif inj == "SM-hang" and calls:
        # the recorded first call's tool arguments become the
        # `sleep 100000` injection — the agent blocks until the
        # declared deadline (agent_timeout, never a synthetic hang).
        c0 = calls[0]
        for member in ("upstream_response", "released"):
            resp = c0.get(member) or {}
            body = (resp.get("body") or {}).get("utf8")
            if isinstance(body, str):
                body = body.replace("printf 'ok' > /app/hh_smoke.txt",
                                    "sleep 100000")
                resp.setdefault("body", {})["utf8"] = body
                resp["body_sha256"] = "sha256:" + hashlib.sha256(
                    body.encode()).hexdigest()
    return rec


def replay_decl(battery, recs, variant):
    r = dict(battery.get("replay") or {})
    if variant == "SM-slow":
        r["latency"] = "recorded"
    return {"recordings": recs,
            "canonicalizer": r.get("canonicalizer", "identity"),
            "strict": r.get("strict", False),
            "keying": r.get("keying", "sequence"),
            "latency": r.get("latency", "immediate")}


def proxy_stats(store_root, run_id, sid):
    """Receipts + endpoint-error dispositions attributed to `sid` on
    the proxy run's wal.

    Two durable bases, counted separately (R6.55 cont. — the W4x
    battery is §4.9 `replay`, which allocates NO slice budget, so
    `measurement.cost.attributed` is never minted by contract):
    - `cost_attributed` — the Ws1 fold's live-mode basis:
      `measurement.cost.attributed` rows scoped by
      `attribution.participant_ref`.
    - `receipts` — the durable per-call receipt: terminal
      `model.call.completed`/`failed` events whose `response_ref`
      names an `hh.model_proxy.receipt/1` doc carrying
      `proxy.slot_id == sid`. This is the mode-universal "the call
      was receipted" row AG6 reads — present in replay where the
      spend row is absent, and on every live call too.
    Endpoint-error dispositions are read straight off the terminal
    event's `disposition` member (the same member the
    cost-attributed `source_event` chain resolved to)."""
    stats = {"receipts": 0, "cost_attributed": 0, "endpoint_errors": []}
    wal = None
    for dp, _, fns in os.walk(store_root):
        for fn in fns:
            if fn == "events.wal" and run_id in dp:
                wal = os.path.join(dp, fn)
    if not wal:
        return None
    events = {}
    for line in open(wal, errors="replace"):
        try:
            e = json.loads(line)
        except Exception:
            continue
        ev = e.get("v") or e
        if ev.get("event_id"):
            events[ev["event_id"]] = ev

    def blob_json(ref):
        if not isinstance(ref, str) or not ref.startswith("sha256:"):
            return None
        p = os.path.join(store_root, "blobs", ref.split(":", 1)[1])
        try:
            with open(p) as f:
                return json.load(f)
        except Exception:
            return None

    for ev in events.values():
        cls = ev.get("class") or ""
        pay = ev.get("payload") or {}
        if cls == "measurement.cost.attributed":
            if (pay.get("attribution") or {}).get(
                    "participant_ref") == sid:
                stats["cost_attributed"] += 1
                src = (pay.get("source_event") or {}).get("event_id")
                disp = str(((events.get(src) or {}).get("payload") or {})
                           .get("disposition") or "")
                if "endpoint" in disp or "protocol" in disp:
                    stats["endpoint_errors"].append(disp)
            continue
        if cls not in ("model.call.completed", "model.call.failed"):
            continue
        doc = blob_json(pay.get("response_ref"))
        if not doc:
            continue
        if ((doc.get("proxy") or {}).get("slot_id")) != sid:
            continue
        stats["receipts"] += 1
        disp = str(pay.get("disposition")
                   or ((doc.get("outcome") or {}).get("disposition"))
                   or "")
        if "endpoint" in disp or "protocol" in disp:
            stats["endpoint_errors"].append(disp)
    return stats


def fold_ag(slot_dir, arm, task, trial_ref, expected_denied, st):
    """`hh-pp0-ag-evidence/1` — the committed Ws1 fold, verbatim."""
    rep_path = os.path.join(slot_dir, "cell-report.json")
    rep = read_json(rep_path) if os.path.exists(rep_path) else {}
    end = rep.get("agent_end_cause") or ""
    denied = rep.get("denied_hosts") or []
    unexpected = [h for h in denied if h not in expected_denied]
    receipts_n = None if st is None else st["receipts"]
    cost_attributed_n = None if st is None else st["cost_attributed"]
    endpoint_errors = [] if st is None else st["endpoint_errors"]
    # the native log's byte total — a smoke asserting the harness's own
    # completion form can never pass on a silent run (the exec-time
    # `bash: <exe>: command not found` and the pipeline's own rc are
    # outside `cell-report.json`, so the byte count is the leg's own
    # honesty check: zero bytes + zero receipts is a silent no-op,
    # recorded `inconclusive{adapter:silent_exit}`, never a pass).
    nlog = rep.get("native_log") or {}
    # the harness's own terminal stream — seeded state files
    # (.claude.json, .hh-env) are rendered artifacts, never agent
    # output; only the *.txt trajectory stream counts.
    native_bytes = sum((f.get("bytes") or 0)
                       for n, f in (nlog.get("files") or {}).items()
                       if n.endswith(".txt"))
    silent = end.startswith("exited") and native_bytes == 0 and receipts_n == 0
    return {
        "schema": "hh-pp0-ag-evidence/1",
        "harness": arm,
        "task": task,
        "trial_ref": trial_ref,
        "agent_end_cause": end,
        "denied_hosts": denied,
        "unexpected_denied": unexpected,
        # `receipt_count` = durable `hh.model_proxy.receipt/1` call
        # receipts scoped to the slot (the AG6 basis — mode-universal);
        # `cost_attributed_count` = `measurement.cost.attributed` rows
        # (live-mode spend rows — §4.9 replay mints none by contract).
        "receipt_count": receipts_n,
        "cost_attributed_count": cost_attributed_n,
        "endpoint_errors": endpoint_errors,
        "native_log_bytes": native_bytes,
        "checks": {
            "AG3": "pass" if end.startswith("exited") else "fail",
            "AG5": "pass" if not unexpected else "fail",
            "AG6": "pass" if (receipts_n or 0) > 0 else "fail",
            "AG8": "pass" if not endpoint_errors else "fail",
        },
        "outcome": ("inconclusive{adapter:silent_exit}" if silent
                    else "withheld{sealed_until_results}"),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cfg", required=True)
    ap.add_argument("--battery", required=True,
                    help="dir carrying battery.json + recordings/ + task/")
    ap.add_argument("--w1-bin", required=True)
    ap.add_argument("--proxy-session", required=True)
    ap.add_argument("--proxy-bin", required=True)
    ap.add_argument("--admissions-dir", required=True)
    ap.add_argument("--layers-file", required=True)
    ap.add_argument("--tasks-dir", required=True)
    ap.add_argument("--lane-file", required=True)
    ap.add_argument("--authorization-file", required=True)
    ap.add_argument("--store-root", required=True)
    ap.add_argument("--run-id", required=True)
    ap.add_argument("--listen", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--arms", default=",".join(ARMS))
    ap.add_argument("--variants", default=",".join(VARIANTS))
    ap.add_argument("--effort", default="high")
    ap.add_argument("--slot-slice-micros", type=int, default=850_000)
    ap.add_argument("--hang-deadline-sec", type=int, default=60)
    a = ap.parse_args()

    battery = read_json(os.path.join(a.battery, "battery.json"))
    arms = a.arms.split(",")
    variants = a.variants.split(",")
    layers = read_json(a.layers_file)
    task = battery.get("task") or "hh-smoke/write-file"
    task_id = task.split("/")[-1]
    os.makedirs(a.out, exist_ok=True)
    held = []
    legs = 0

    for variant in variants:
        # one replay session per variant — recordings re-spelled to
        # this variant's slot ids and injected per the battery table.
        recs = []
        decl = None
        for arm in arms:
            base_path = os.path.join(
                a.battery,
                (battery.get("recordings") or {}).get(arm, ""))
            if not base_path or not os.path.exists(base_path):
                continue
            base = read_json(base_path)
            sid = slot_id(variant, arm)
            recs.append(injected_recording(
                base, variant, sid, a.hang_deadline_sec))
        decl = replay_decl(battery, recs, variant)
        vdir = os.path.join(a.out, variant)
        os.makedirs(vdir, exist_ok=True)
        decl_path = os.path.join(vdir, "replay.json")
        with open(decl_path, "w") as f:
            json.dump(decl, f, indent=1, sort_keys=True)
        sess_file = os.path.join(vdir, "session.json")
        sentinel = os.path.join(vdir, "proxy.sentinel")
        slot_specs = ",".join(
            f"{slot_id(variant, arm)}:{a.slot_slice_micros}:{a.effort}"
            for arm in arms)
        run_id = f"{a.run_id}-{variant.lower()}"
        ps = subprocess.Popen(
            ["python3", a.proxy_session, "--proxy-bin", a.proxy_bin,
             "--lane-file", a.lane_file,
             "--authorization-file", a.authorization_file,
             "--store-root", a.store_root,
             "--run-id", run_id, "--listen", a.listen,
             "--slots", slot_specs, "--mode", "replay",
             "--replay-file", decl_path,
             "--session-out", sess_file, "--sentinel", sentinel,
             "--max-wait-seconds", "3600"],
            stdout=open(os.path.join(vdir, "proxy-session.log"), "w"),
            stderr=subprocess.STDOUT, text=True)
        sess = None
        t0 = time.time()
        while time.time() - t0 < 90:
            if os.path.exists(sess_file):
                try:
                    sess = read_json(sess_file)
                except Exception:
                    sess = None
                if sess and sess.get("grants"):
                    break
            if ps.poll() is not None:
                break
            time.sleep(0.5)
        if not sess or not sess.get("grants"):
            for arm in arms:
                legs += 1
                held.append({
                    "variant": variant, "arm": arm,
                    "slot_id": slot_id(variant, arm),
                    "disposition":
                        "unavailable{infra:replay_session}",
                    "checks": {g: "fail" for g in
                               ("AG3", "AG5", "AG6", "AG8")}})
            try:
                ps.kill()
            except Exception:
                pass
            continue
        listen_port = int((sess.get("listen") or a.listen)
                          .rsplit(":", 1)[1])
        grants = sess.get("grants") or {}
        for arm in arms:
            legs += 1
            sid = slot_id(variant, arm)
            slot_dir = os.path.join(vdir, sid)
            os.makedirs(slot_dir, exist_ok=True)
            adm_path = os.path.join(
                a.admissions_dir, ADMISSION_FILES.get(arm, f"{arm}.json"))
            image = (layers.get(arm) or {}).get(task_id) \
                or (layers.get(arm) or {}).get("*")
            token = (grants.get(sid) or {}).get("slot_token")
            ev = None
            if not token or not image or not os.path.exists(adm_path):
                ev = {"schema": "hh-pp0-ag-evidence/1", "harness": arm,
                      "task": task, "trial_ref": f"smoke/{variant}/{sid}",
                      "disposition":
                          "unavailable{infra:%s}" % (
                              "slot_grant" if not token else
                              "layer_image" if not image else "admission"),
                      "checks": {g: "fail"
                                 for g in ("AG3", "AG5", "AG6", "AG8")}}
            else:
                tok_file = os.path.join(slot_dir, "slot-token")
                with open(tok_file, "w") as f:
                    f.write(token)
                os.chmod(tok_file, 0o600)
                cmd = [a.w1_bin, "arm-run", "--cfg", a.cfg,
                       "--task", task_id, "--image", image,
                       "--admission", adm_path,
                       "--slot-id", sid,
                       "--slot-token-file", tok_file,
                       "--model-upstream", str(listen_port),
                       "--slot-slice-micros", str(a.slot_slice_micros),
                       "--out", slot_dir]
                eprint(f"{variant} {arm}: arm-run → {slot_dir}")
                p = subprocess.run(cmd, capture_output=True, text=True)
                with open(os.path.join(slot_dir, "arm-run.log"), "w") as f:
                    f.write(p.stdout + "\n--- stderr ---\n" + p.stderr)
                adm = read_json(adm_path)
                expected = ((adm.get("config") or {})
                            .get("expected_denied") or [])
                st = proxy_stats(sess.get("store_root") or a.store_root,
                                 run_id, sid)
                ev = fold_ag(slot_dir, arm, task,
                             f"smoke/{variant}/{sid}", expected, st)
                ev["arm_run_rc"] = p.returncode
                ev["variant"] = variant
                if p.returncode != 0:
                    ev["machinery"] = p.stderr.strip().splitlines()[:3]
            with open(os.path.join(vdir, f"{sid}.json"), "w") as f:
                json.dump(ev, f, indent=1, sort_keys=True)
            if any(v == "fail" for v in ev.get("checks", {}).values()):
                held.append({"variant": variant, "arm": arm,
                             "slot_id": sid,
                             "disposition": "withheld{admission:AG<n>}",
                             "checks": ev.get("checks")})
            eprint(f"{variant} {arm}: checks={ev.get('checks')}")
        # close the variant's session honestly.
        try:
            with open(sentinel, "w") as f:
                f.write("done")
            ps.wait(timeout=120)
        except Exception:
            try:
                ps.terminate()
            except Exception:
                pass

    summary = {"schema": "hh-w4x-smoke-summary/1",
               "battery_sha256": "sha256:" + hashlib.sha256(
                   open(os.path.join(
                       a.battery, "battery.json"), "rb").read()).hexdigest(),
               "legs": legs,
               "held": held,
               "evidence_dir": a.out,
               "mode": "replay",
               "run_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                       time.gmtime())}
    with open(os.path.join(a.out, "_summary.json"), "w") as f:
        json.dump(summary, f, indent=1, sort_keys=True)
    print(json.dumps({"legs": legs, "held": len(held)}), flush=True)
    sys.exit(0)


if __name__ == "__main__":
    main()
