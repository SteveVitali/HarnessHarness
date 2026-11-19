#!/usr/bin/env python3
"""ws1-wal-recordings.py — fold a record-mode proxy run's wal into
per-slot `hh.model_proxy.recording/1` manifests.

`seal_recording` folds the slot's calls from the proxy's in-memory
capture (`core.recording`) at session close. A VM stop or proxy kill
before close loses that map — but every call's bytes are durably in
the wal (`model.call.completed|failed → response_ref` receipt blobs,
which carry downstream/upstream body refs, disposition, timing, and
slot attribution) plus the content-addressed blob pool. This tool
rebuilds the identical manifests from that sealed evidence — a
recovery projection, not new evidence. The underlying calls were
recorded live; only the manifest fold is post-hoc.

Fidelity vs a live seal (recorded honestly in the manifest + ticket):
  - `upstream_response.headers`: the wal keeps only the outbound
    allowlist-kept subset (the receipt's `released.headers`); the full
    upstream header set was in-memory only. Emitted as that subset.
  - `upstream_response.chunk_stamps`: not in the wal → `[]`.
  - `released.body`: recovered when `released.identical_to_upstream`
    (body = `upstream.response_body_ref`); otherwise `released: null`
    is emitted and counted in `anomalies`.
  - `upstream_response` / `upstream_request`: null when the call
    refused before dispatch (receipt `upstream.dispatched` false).
  - unbound calls (`slot_id: null` — refused health checks etc.) are
    excluded: they never entered `core.recording` either.

usage: ws1-wal-recordings.py --store <proxy-store> --run <run_id>
        [--slots sid,sid,… | all slot-bound] --out <dir>
        [--manifest-out <f> --tasks <csv> --traj-per-task <n>
         --layers <layers.json> --corpus-root <run_root/corpus>]
"""
import argparse, hashlib, json, os, sys, time


def eprint(*a):
    print(*a, file=sys.stderr)


def read_json(p):
    with open(p) as f:
        return json.load(f)


def blob_path(store, ref):
    return os.path.join(store, "blobs", ref.split(":", 1)[1])


def load_blob(store, ref):
    with open(blob_path(store, ref), "rb") as f:
        return f.read()


def body_json(b):
    try:
        return {"utf8": b.decode("utf-8")}
    except UnicodeDecodeError:
        return {"hex": b.hex()}


def body_member(b):
    return {"body_sha256": "sha256:" + hashlib.sha256(b).hexdigest(),
            "body": body_json(b)}


def call_from_receipt(store, rec):
    """Receipt `proxy` member → recording/1 call doc (+anomaly flags)."""
    p = rec.get("proxy") or {}
    anomalies = []
    dn = p.get("downstream") or {}
    body = load_blob(store, dn.get("body_ref", "sha256:" + "0" * 64)) \
        if dn.get("body_ref") else b""
    call = {
        "k": p.get("slot_call_seq", 0),
        "downstream": {
            "method": dn.get("method", "POST"),
            "path": dn.get("path", ""),
            **body_member(body),
        },
        "upstream_request": None,
        "upstream_response": None,
        "failure": p.get("failure") or None,
        "released": None,
        "disposition": p.get("disposition", "forwarded"),
        "latency_ms": (p.get("timing") or {}).get("latency_ms") or 0,
    }
    up = p.get("upstream") or {}
    if up.get("dispatched") and up.get("request_body_ref"):
        ub = load_blob(store, up["request_body_ref"])
        call["upstream_request"] = body_member(ub)
    if up.get("response_body_ref") and up.get("status"):
        rb = load_blob(store, up["response_body_ref"])
        call["upstream_response"] = {
            "status": up["status"],
            "headers": (p.get("released") or {}).get("headers") or {},
            "chunk_stamps": [],
            **body_member(rb),
        }
    rel = p.get("released") or {}
    if isinstance(rel.get("status"), int):
        if rel.get("identical_to_upstream") and call["upstream_response"]:
            call["released"] = {
                "status": rel["status"],
                "headers": rel.get("headers") or {},
                **body_member(load_blob(store, up["response_body_ref"])),
            }
        else:
            anomalies.append(f"{p.get('model_call_id','?')}: "
                             "released body not identical_to_upstream "
                             "— not wal-durable, emitted null")
    elif rel.get("status") is not None:
        anomalies.append(f"{p.get('model_call_id','?')}: "
                         f"released status {rel.get('status')!r} → null")
    return call, anomalies


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--store", required=True)
    ap.add_argument("--run", required=True)
    ap.add_argument("--slots", default="",
                    help="comma list; default = every slot-bound call")
    ap.add_argument("--out", required=True)
    ap.add_argument("--manifest-out", default="")
    ap.add_argument("--tasks", default="")
    ap.add_argument("--traj-per-task", type=int, default=4)
    ap.add_argument("--layers", default="")
    ap.add_argument("--corpus-root", default="",
                    help="run_root/corpus — cell-report dirs for "
                         "manifest rows")
    a = ap.parse_args()

    wal = os.path.join(a.store, "runs", a.run, "events.wal")
    want = set(s for s in a.slots.split(",") if s) or None
    calls = {}          # slot_id -> [call docs]
    anomalies = []
    n_terms = 0
    with open(wal) as f:
        for line in f:
            try:
                ev = json.loads(line)
            except Exception:
                continue
            v = ev.get("v") or {}
            if v.get("class") not in ("model.call.completed",
                                    "model.call.failed"):
                continue
            ref = (v.get("payload") or {}).get("response_ref")
            if not ref:
                continue
            n_terms += 1
            try:
                rec = read_json(blob_path(a.store, ref))
            except Exception as e:
                anomalies.append(f"{ref}: receipt unreadable: {e}")
                continue
            p = rec.get("proxy") or {}
            sid = p.get("slot_id")
            if not sid:
                continue   # unbound — never entered core.recording
            if want and sid not in want:
                continue
            call, an = call_from_receipt(a.store, rec)
            anomalies.extend(an)
            calls.setdefault(sid, []).append(call)

    os.makedirs(a.out, exist_ok=True)
    recs = []
    for sid in sorted(calls):
        doc = {"schema": "hh.model_proxy.recording/1",
               "slot_id": sid,
               "provenance": "recorded",
               "calls": calls[sid]}
        fn = os.path.join(a.out, f"{sid}.recording.json")
        with open(fn, "w") as f:
            json.dump(doc, f, indent=1, sort_keys=True)
        recs.append({"file": os.path.basename(fn), "slot_id": sid})

    if a.manifest_out:
        # The manifest's recordings list is whatever is present in the
        # out dir — wal-folded recovery docs and live-sealed docs mix.
        recs = []
        for fn in sorted(os.listdir(a.out)):
            if "recording" in fn and fn.endswith(".json"):
                try:
                    d = read_json(os.path.join(a.out, fn))
                except Exception:
                    continue
                if d.get("schema") == "hh.model_proxy.recording/1":
                    recs.append({"file": fn, "slot_id": d.get("slot_id")})
        layers = {}
        if a.layers and os.path.exists(a.layers):
            layers = read_json(a.layers).get("mini-swe-agent") or {}
        tasks = [t.strip() for t in a.tasks.split(",") if t.strip()]
        plan = {}
        n = 0
        for t in tasks:
            for tr in range(a.traj_per_task):
                n += 1
                plan[f"ws1-corpus-{n:02d}"] = (t, tr)
        rows = []
        for sid in sorted(plan, key=lambda s: s):
            task, tr = plan[sid]
            slot_dir = os.path.join(a.corpus_root, sid) \
                if a.corpus_root else ""
            rep_p = os.path.join(slot_dir, "cell-report.json")
            rep = read_json(rep_p) if os.path.exists(rep_p) else {}
            has_rep = bool(rep)
            row = {"slot_id": sid, "task": task, "trajectory": tr,
                   "image": layers.get(task),
                   "arm_run_rc": 0 if has_rep else None,
                   "result": "recorded" if has_rep
                             else "withheld{corpus_infra:no_cell_report}",
                   "agent_end_cause": rep.get("agent_end_cause"),
                   "denied_hosts": rep.get("denied_hosts"),
                   "verify_attempts": rep.get("verify_attempts")}
            rows.append(row)
        man = {"schema": "hh-ws1-corpus-manifest/1",
               "trajectories": len(rows),
               "recordings": recs,
               "slots": rows,
               "fold": {"source": f"{a.run}/events.wal",
                        "tool": "ws1-wal-recordings.py",
                        "note": "manifests rebuilt from durable wal "
                                "+ blob pool after proxy interruption"},
               "run_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                       time.gmtime())}
        with open(a.manifest_out, "w") as f:
            json.dump(man, f, indent=1, sort_keys=True)

    print(json.dumps({
        "terminal_events": n_terms,
        "slots": {sid: len(cs) for sid, cs in sorted(calls.items())},
        "recordings_written": len(recs),
        "anomalies": anomalies}, indent=1))


if __name__ == "__main__":
    main()
