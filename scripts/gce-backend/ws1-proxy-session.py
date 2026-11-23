#!/usr/bin/env python3
"""ws1-proxy-session.py — the Ws1 window-proxy session shim (R6.45q wiring).

Cloned from W2's retained `w2-proxy-session.py` (same committed shape — the
runner-side owner of the phase proxy run: holds the spawned process's stdio
control channel for the whole session so slot grants stay live across the leg
boundary). Composes the committed hh-model-proxy control ops verbatim —
configure → register_slot → …sentinel… → (seal_recording) → revoke_slot →
stop. No new leg, schema, record kind or codec.

Adds over W2's live-only shim: `--mode live|record|replay` and `--replay-file`
(proxy's replay declaration JSON — recordings + canonicalizer + strict +
keying per spec §7.4). `record` mode is the corpus leg (each sealed slot mints
its `hh.model_proxy.recording/1`); `replay` serves the L1 legs at $0.

usage:
  ws1-proxy-session.py --proxy-bin BIN --store-root DIR --run-id ID \
    --listen HOST:PORT --lane-file lane.json --authorization-file auth.json \
    --slots 'ws1-pp0:850000:high,ws1-corpus-01:850000:high' \
    --mode live [--replay-file replay.json] \
    --session-out session.json --sentinel /path/done --poll-seconds 20 \
    [--seal-recordings]

OLLAMA_API_KEY rides the launch env only (credential_env naming); the shim
never reads, prints or records it.
"""
import argparse, json, os, subprocess, sys, time

def eprint(*a): print(*a, file=sys.stderr, flush=True)

def ctl(proc, op):
    proc.stdin.write((json.dumps(op) + "\n").encode()); proc.stdin.flush()
    line = proc.stdout.readline()
    if not line:
        raise SystemExit("session: proxy control channel closed")
    return json.loads(line)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--proxy-bin", required=True)
    ap.add_argument("--store-root", required=True)
    ap.add_argument("--run-id", required=True)
    ap.add_argument("--listen", required=True)
    ap.add_argument("--lane-file", required=True)
    ap.add_argument("--authorization-file", required=True)
    ap.add_argument("--slots", required=True,
                    help="comma-separated slot_id:slice_micros:effort")
    ap.add_argument("--mode", choices=["live", "record", "replay"],
                    default="live")
    ap.add_argument("--replay-file", default=None,
                    help="replay declaration JSON (mode=replay only)")
    ap.add_argument("--seal-recordings", action="store_true",
                    help="seal_recording each slot at close (record mode)")
    ap.add_argument("--session-out", required=True)
    ap.add_argument("--sentinel", required=True)
    ap.add_argument("--poll-seconds", type=int, default=20)
    ap.add_argument("--max-wait-seconds", type=int, default=9 * 3600)
    a = ap.parse_args()

    if a.mode == "replay" and not a.replay_file:
        ap.error("--replay-file required when --mode replay")

    lane = json.load(open(a.lane_file))
    auth = json.load(open(a.authorization_file))
    cfg = {"op": "configure", "store_root": a.store_root,
           "run_id": a.run_id, "listen": a.listen,
           "lane": lane, "credential_env": "OLLAMA_API_KEY",
           "authorization": auth, "mode": a.mode}
    if a.mode == "replay":
        cfg["replay"] = json.load(open(a.replay_file))

    env = dict(os.environ)
    # the key must never ride argv (sudo logs COMMAND= to the journal —
    # the Ws1 closeout found OLLAMA_API_KEY in journald via
    # `/usr/bin/env KEY=… python3 …`). When the launch env lacks it,
    # read the protected secret file delivered by deliver-secret.
    if not env.get("OLLAMA_API_KEY"):
        for cand in ("/run/hh/secrets/OLLAMA_API_KEY",
                     "/var/lib/hh/ws1/ollama-key"):
            try:
                env["OLLAMA_API_KEY"] = open(cand).read().strip()
                if env["OLLAMA_API_KEY"]:
                    break
            except OSError:
                continue
    proc = subprocess.Popen([a.proxy_bin], stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, env=env)
    try:
        rep = ctl(proc, cfg)
        if "error" in rep:
            eprint("configure failed:", rep); sys.exit(2)
        listen = rep.get("ok", {}).get("listen") or a.listen
        grants = {}
        for spec in a.slots.split(","):
            sid, micros, effort = spec.split(":")
            rep = ctl(proc, {"op": "register_slot", "slot_id": sid,
                             "effort": effort,
                             "slice_micros": int(micros)})
            if "error" in rep:
                eprint(f"register_slot {sid} failed:", rep); sys.exit(2)
            ok = rep.get("ok") or {}
            grants[sid] = {"slot_token": ok.get("token") or ok.get("slot_token"),
                           "slice_micros": int(micros), "effort": effort}
        os.makedirs(os.path.dirname(a.session_out), exist_ok=True)
        sess = {"schema": "hh-w2-proxy-session/1",
                "run_id": a.run_id, "listen": listen,
                "store_root": a.store_root,
                "mode": a.mode,
                "grants": grants,
                "registered_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                              time.gmtime())}
        with open(a.session_out, "w") as f:
            json.dump(sess, f, indent=1, sort_keys=True)
        print(json.dumps({"listen": listen, "mode": a.mode,
                          "slots": sorted(grants)}), flush=True)
        # hold the control channel open until the sentinel lands
        waited = 0
        while not os.path.exists(a.sentinel):
            if waited >= a.max_wait_seconds:
                eprint("session: sentinel timeout — closing honestly")
                break
            time.sleep(a.poll_seconds); waited += a.poll_seconds
        # Close honestly (R6.52 repair): only acknowledged revocations
        # enter `revoked`; every seal/revoke/stop failure is recorded
        # per id and the shim exits nonzero — a caller can never read a
        # closed session as clean without the proxy's own `ok`.
        sealed, seal_failed = {}, {}
        if a.seal_recordings:
            for sid in grants:
                try:
                    rep = ctl(proc, {"op": "seal_recording", "slot_id": sid})
                except SystemExit as e:
                    rep = {"error": str(e)}
                sealed[sid] = (rep.get("ok") or rep) \
                    if "error" not in rep else {"error": rep["error"]}
                if "error" in rep:
                    seal_failed[sid] = rep["error"]
                    eprint(f"seal_recording {sid}:", rep)
        revoked, revoke_failed = [], {}
        for sid in grants:
            try:
                rep = ctl(proc, {"op": "revoke_slot", "slot_id": sid})
            except SystemExit as e:
                rep = {"error": str(e)}
            if "error" in rep or "ok" not in rep:
                revoke_failed[sid] = rep.get("error") or "unacknowledged"
                eprint(f"revoke_slot {sid}:", rep)
            else:
                revoked.append(sid)
        try:
            rep = ctl(proc, {"op": "stop"})
            stop = "ok" if "ok" in rep and "error" not in rep \
                else f"failed{{{rep.get('error') or 'unacknowledged'}}}"
        except SystemExit:
            stop = "failed{control_channel_closed}"
        time.sleep(1.0)
        end = {"schema": "hh-w2-proxy-session-end/1",
               "run_id": a.run_id,
               "revoked": sorted(revoked),
               "sealed": sealed,
               "ended_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
               "waited_seconds": waited,
               "stop": stop}
        if revoke_failed:
            end["revoke_failed"] = revoke_failed
        if seal_failed:
            end["seal_failed"] = seal_failed
        with open(a.session_out + ".end", "w") as f:
            json.dump(end, f, indent=1, sort_keys=True)
        clean = not (revoke_failed or seal_failed) and stop == "ok"
        print(json.dumps({"session": "closed" if clean else "closed_incomplete",
                          "waited_seconds": waited,
                          "sealed": sorted(sealed)}), flush=True)
        if not clean:
            sys.exit(3)
    finally:
        try:
            proc.terminate()
        except Exception:
            pass

if __name__ == "__main__":
    main()
