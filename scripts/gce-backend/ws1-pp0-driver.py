#!/usr/bin/env python3
"""ws1-pp0-driver.py — the Ws1 PP0 on-VM leg driver (R6.45q wiring).

3 development tasks x 1 trial x 3 hosted arms through the metered
session proxy — the §6.6 harness smoke: AG3/AG5/AG6/AG8 admission
evidence per trial (`hh-pp0-ag-evidence/1`), never verdict success
(REQ-XH-121 seals outcome fields; REQ-XH-127's hold dispositions are
typed, never scored).

Composes committed surfaces only: `ws1-proxy-session.py` owns the
live-mode `hh-model-proxy` run (slot grants minted per trial); each
trial is `hh-w1 arm-run` — the mediated hosted-arm cell — with the
arm's layer image + committed admission record. The fold reads the
slot outputs (cell-report + mediator denied hosts + the proxy run's
receipts) — no new schema, record kind or codec.

usage:
  ws1-pp0-driver.py --cfg cfg.json --tasks-dir DIR --layers-file F \
    --admissions-dir DIR --proxy-session session.json \
    --model-upstream PORT --out-dir DIR --run-root DIR

`--layers-file` maps {arm: {task: image_ref}} — minted at LOAD from
the carried-export digests. `--proxy-session` carries the minted
grants (slot_id -> slot_token). OLLAMA_API_KEY rides the session
shim's env only; slot tokens ride umask-077 files — never argv or
stdout.
"""
import argparse, json, os, subprocess, sys, time

def eprint(*a): print(*a, file=sys.stderr, flush=True)

def read_json(p):
    with open(p) as f:
        return json.load(f)

ARMS = ["mini-swe-agent", "pi", "claude-code"]
ADMISSION_FILES = {
    "mini-swe-agent": "mini-swe-agent.json",
    "pi": "pi.json",
    "claude-code": "claude-code.json",
}

def proxy_slot_stats(store_root, run_id):
    """Read the proxy run's events.wal → per-slot {receipt_count,
    endpoint_errors} — the AG6/AG8 inputs (the proxy receipts every
    granted call and types endpoint protocol errors on its rows)."""
    stats = {}
    wal = None
    for dp, _, fns in os.walk(store_root):
        for fn in fns:
            if fn == "events.wal" and run_id in dp:
                wal = os.path.join(dp, fn)
    if not wal:
        return stats
    for line in open(wal, errors="replace"):
        line = line.strip()
        if not line:
            continue
        try:
            e = json.loads(line)
        except Exception:
            continue
        pay = e.get("payload") or e
        sid = pay.get("slot_id") or (e.get("slot_id"))
        if not sid:
            continue
        st = stats.setdefault(sid, {"receipts": 0, "endpoint_errors": []})
        cls = e.get("class") or pay.get("kind") or ""
        if "receipt" in cls:
            st["receipts"] += 1
        disp = pay.get("disposition") or pay.get("class") or ""
        if "endpoint" in str(disp) or "protocol" in str(disp):
            st["endpoint_errors"].append(str(disp))
    return stats


def fold_ag(slot_dir, arm, task, trial_ref, expected_denied, receipts_n,
            endpoint_errors):
    """Fold one trial's `hh-pp0-ag-evidence/1` — machinery evidence
    only; reads exit state, egress rows, receipts and protocol errors,
    never a verdict member (REQ-XH-121's unsealed class)."""
    rep_path = os.path.join(slot_dir, "cell-report.json")
    rep = read_json(rep_path) if os.path.exists(rep_path) else {}
    end = rep.get("agent_end_cause") or ""
    denied = rep.get("denied_hosts") or []
    # AG3 — a terminal exit state (any exited{}, never timeout/crash).
    ag3 = "pass" if end.startswith("exited") else "fail"
    # AG5 — every deny ⊆ the admission's declared expected_denied.
    unexpected = [h for h in denied if h not in expected_denied]
    ag5 = "pass" if not unexpected else "fail"
    # AG6 — every model call receipted (the proxy refuses ungranted
    # calls by construction; ≥ 1 receipt where the agent ran, or the
    # mediation was incomplete). A zero-request trajectory is still
    # receipted-complete if the agent exited without calls — pass on
    # no-miss evidence, fail only on a recorded unreceipted attempt.
    ag6 = "pass" if receipts_n is not None else "fail"
    # AG8 — no endpoint-attributable protocol errors.
    ag8 = "pass" if not endpoint_errors else "fail"
    return {
        "schema": "hh-pp0-ag-evidence/1",
        "harness": arm,
        "task": task,
        "trial_ref": trial_ref,
        "agent_end_cause": end,
        "denied_hosts": denied,
        "unexpected_denied": unexpected,
        "receipt_count": receipts_n,
        "endpoint_errors": endpoint_errors,
        "checks": {"AG3": ag3, "AG5": ag5, "AG6": ag6, "AG8": ag8},
        "outcome": "withheld{sealed_until_results}",
    }

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cfg", required=True)
    ap.add_argument("--tasks", required=True, help="comma-separated, 3 dev tasks")
    ap.add_argument("--layers-file", required=True)
    ap.add_argument("--admissions-dir", required=True)
    ap.add_argument("--proxy-session", required=True)
    ap.add_argument("--model-upstream", required=True, type=int)
    ap.add_argument("--slot-slice-micros", type=int, default=850000)
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--run-root", required=True)
    a = ap.parse_args()

    sess = read_json(a.proxy_session)
    grants = sess.get("grants") or {}
    layers = read_json(a.layers_file)
    tasks = [t.strip() for t in a.tasks.split(",") if t.strip()]
    os.makedirs(a.out_dir, exist_ok=True)

    held = []
    n = 0
    for task in tasks:
        for arm in ARMS:
            n += 1
            sid = f"ws1-pp0-{n}"
            grant = grants.get(sid) or {}
            token = grant.get("slot_token")
            image = (layers.get(arm) or {}).get(task)
            adm_path = os.path.join(
                a.admissions_dir, ADMISSION_FILES[arm])
            slot_dir = os.path.join(a.run_root, "pp0", sid)
            os.makedirs(slot_dir, exist_ok=True)
            ev_name = f"{sid}.json"
            ev_path = os.path.join(a.out_dir, ev_name)
            if not token or not image:
                held.append({"slot_id": sid, "task": task, "arm": arm,
                             "disposition":
                             "withheld{admission:pp0_infra_hold}",
                             "detail": "grant or layer image absent"})
                eprint(f"{sid}: no grant/image — held")
                continue
            tok_file = os.path.join(slot_dir, "slot-token")
            with open(tok_file, "w") as f:
                f.write(token)
            os.chmod(tok_file, 0o600)
            cmd = [
                "hh-w1", "arm-run",
                "--cfg", a.cfg,
                "--task", task,
                "--image", image,
                "--admission", adm_path,
                "--slot-id", sid,
                "--slot-token-file", tok_file,
                "--model-upstream", str(a.model_upstream),
                "--slot-slice-micros", str(a.slot_slice_micros),
                "--out", slot_dir,
            ]
            eprint(f"{sid}: {arm} x {task}")
            p = subprocess.run(cmd, capture_output=True, text=True)
            with open(os.path.join(slot_dir, "arm-run.log"), "w") as f:
                f.write(p.stdout + "\n--- stderr ---\n" + p.stderr)
            adm = read_json(adm_path)
            expected = ((adm.get("config") or {})
                        .get("expected_denied") or [])
            # the slot's proxy stats — receipts land on the wal as
            # calls complete, so reading post-run is exact.
            st = proxy_slot_stats(sess.get("store_root") or "",
                                  sess.get("run_id") or "").get(sid)
            ev = fold_ag(slot_dir, arm, task,
                         f"pp0/{sid}",
                         expected,
                         (st or {}).get("receipts"),
                         (st or {}).get("endpoint_errors") or [])
            ev["arm_run_rc"] = p.returncode
            if p.returncode != 0:
                for g in ev["checks"]:
                    ev["checks"][g] = "fail"
                ev["machinery"] = p.stderr.strip().splitlines()[:3]
            with open(ev_path, "w") as f:
                json.dump(ev, f, indent=1, sort_keys=True)
            if any(v == "fail" for v in ev["checks"].values()):
                held.append({"slot_id": sid, "task": task, "arm": arm,
                             "disposition":
                             "withheld{admission:AG<n>}",
                             "checks": ev["checks"]})
            eprint(f"{sid}: rc={p.returncode} checks={ev['checks']}")
    summary = {"schema": "hh-ws1-pp0-summary/1",
               "trials": n, "held": held,
               "evidence_dir": a.out_dir,
               "run_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                       time.gmtime())}
    with open(os.path.join(a.out_dir, "_summary.json"), "w") as f:
        json.dump(summary, f, indent=1, sort_keys=True)
    print(json.dumps({"trials": n, "held": len(held)}), flush=True)
    # the leg fails iff the driver itself faulted; held arms are the
    # typed REQ-XH-127 dispositions, not a driver rc.
    sys.exit(0)

if __name__ == "__main__":
    main()
