#!/usr/bin/env python3
"""ws1-corpus-driver.py — the Ws1 L1 RECORD corpus on-VM leg driver.

7 development tasks x 4 trajectories = 28 metered slots of original
mini-swe-agent (the REQ-XH-114/REQ-XD-107 natural corpus). Each slot
is `hh-w1 arm-run` against the record-mode session proxy; the slot's
sealed `hh.model_proxy.recording/1` is the corpus member. The leg's
output dir carries one recording per trajectory plus the folded
`hh-l1-corpus/1` manifest rows — the partition digest f7ddbc1f… binds
dev/gate-A/gate-B by task (hh-fp-partition owns the partition record;
this driver keys each recording to its task for the gate readers).

usage:
  ws1-corpus-driver.py --cfg cfg.json --tasks T1,T2,... --layers-file F \
    --admission mini-swe-agent.json --proxy-session session.json \
    --model-upstream PORT --out-dir DIR --run-root DIR

`--proxy-session` is the record-mode session's session.json — grants
ws1-corpus-01..28 minted by ws1-proxy-session.py; recordings are
sealed by the shim's --seal-recordings at session close and folded
from the proxy store's recordings dir (or `seal_recording` replies).
OLLAMA_API_KEY rides the shim env only; tokens ride umask-077 files.
"""
import argparse, json, os, subprocess, sys, time

def eprint(*a): print(*a, file=sys.stderr, flush=True)

def read_json(p):
    with open(p) as f:
        return json.load(f)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cfg", required=True)
    ap.add_argument("--tasks", required=True, help="comma-separated, 7 dev tasks")
    ap.add_argument("--traj-per-task", type=int, default=4)
    ap.add_argument("--layers-file", required=True)
    ap.add_argument("--admission", required=True)
    ap.add_argument("--proxy-session", required=True)
    ap.add_argument("--model-upstream", required=True, type=int)
    ap.add_argument("--slot-slice-micros", type=int, default=850000)
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--w1-bin", default="hh-w1")
    ap.add_argument("--run-root", required=True)
    a = ap.parse_args()

    sess = read_json(a.proxy_session)
    grants = sess.get("grants") or {}
    layers = (read_json(a.layers_file).get("mini-swe-agent") or {})
    tasks = [t.strip() for t in a.tasks.split(",") if t.strip()]
    os.makedirs(a.out_dir, exist_ok=True)

    results = []
    n = 0
    for task in tasks:
        image = layers.get(task)
        for t in range(a.traj_per_task):
            n += 1
            sid = f"ws1-corpus-{n:02d}"
            grant = grants.get(sid) or {}
            token = grant.get("slot_token")
            slot_dir = os.path.join(a.run_root, "corpus", sid)
            os.makedirs(slot_dir, exist_ok=True)
            row = {"slot_id": sid, "task": task, "trajectory": t,
                   "image": image}
            if not token or not image:
                row["result"] = "withheld{corpus_infra:grant_or_layer_absent}"
                eprint(f"{sid}: no grant/image — withheld")
                results.append(row)
                continue
            tok_file = os.path.join(slot_dir, "slot-token")
            with open(tok_file, "w") as f:
                f.write(token)
            os.chmod(tok_file, 0o600)
            cmd = [
                a.w1_bin, "arm-run",
                "--cfg", a.cfg,
                "--task", task,
                "--image", image,
                "--admission", a.admission,
                "--slot-id", sid,
                "--slot-token-file", tok_file,
                "--model-upstream", str(a.model_upstream),
                "--slot-slice-micros", str(a.slot_slice_micros),
                "--out", slot_dir,
            ]
            eprint(f"{sid}: {task} traj {t}")
            p = subprocess.run(cmd, capture_output=True, text=True)
            with open(os.path.join(slot_dir, "arm-run.log"), "w") as f:
                f.write(p.stdout + "\n--- stderr ---\n" + p.stderr)
            row["arm_run_rc"] = p.returncode
            row["result"] = ("recorded" if p.returncode == 0
                             else "withheld{corpus_infra:arm_run_rc}")
            results.append(row)
            eprint(f"{sid}: rc={p.returncode}")
    # recordings fold — the record-mode proxy writes each sealed
    # hh.model_proxy.recording/1 under its store; fold them out.
    store = sess.get("store_root") or ""
    recs = []
    for dp, _, fns in os.walk(store):
        for fn in fns:
            if "recording" in fn and fn.endswith(".json"):
                p = os.path.join(dp, fn)
                try:
                    d = read_json(p)
                except Exception:
                    continue
                if d.get("schema") == "hh.model_proxy.recording/1":
                    dst = os.path.join(a.out_dir, os.path.basename(p))
                    with open(dst, "w") as f:
                        json.dump(d, f, indent=1, sort_keys=True)
                    recs.append({"file": os.path.basename(p),
                                 "slot_id": d.get("slot_id")})
    manifest = {"schema": "hh-ws1-corpus-manifest/1",
                "trajectories": len(results),
                "recordings": recs,
                "slots": results,
                "run_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                        time.gmtime())}
    with open(os.path.join(a.out_dir, "_manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1, sort_keys=True)
    print(json.dumps({"trajectories": len(results),
                      "recordings": len(recs),
                      "withheld": sum(1 for r in results
                                      if r["result"] != "recorded")}),
          flush=True)
    sys.exit(0)

if __name__ == "__main__":
    main()
