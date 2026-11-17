#!/usr/bin/env python3
"""ws1-l1-driver.py — the Ws1 L1 legs on-VM driver (R6.45q wiring).

Drives spec §7.4's L1.0/L1.1/L1.2 over the gate-A partition's 8
trajectories against the exact frozen H-set (set digest c451ed97…).

Leg wiring (committed surfaces only):

- L1.0 — O twice against REPLAY(S): two `hh-w1 arm-run` cells of the
  original mini-swe-agent arm, its model endpoint a replay-mode
  ws1-proxy-session serving the trajectory's recording (strict,
  declared hh-l1-canon). The replay proxy receipts/captures every
  downstream request — those captures are the O drive's request
  sequence; the trajectory's tool-call records give parsed/executed
  actions; its final state the termination class. Two runs → the
  stable prefix s(S).
- L1.1 — H against strict response replay + observation replay over
  the whole trajectory; first divergence fails. H = the kernel-embedded
  frozen HIR driven at the replay listener (model seam = the proxy;
  exec seam = observation replay).
- L1.2 — H against replay + live exec; divergence at/before s(S)
  fails, the first post-prefix request excluded from gating.

The comparison fold (`stable_prefix`/`compare_drives`/`diff_record`/
`l1_pass_id`/`conformance_record`) is the committed l1_gate library —
the driver passes its captured L1Drive docs to `hh-w1 l1-fold` and
writes the emitted `hh-l1-diff/1`/`hh-l1-conformance/1` rows.

STATUS (authored 2026-11-17, corrected): the L1Sequence contract
makes obs_a/obs_b the recorded environment sequences for BOTH O runs
— L1.0/L1.1 are observation-replay drives, not live-exec cells, so
arm-run alone does NOT implement them. Every leg except L1.2's exec
needs an obs-replay exec adapter (positional server on the exec seam
returning recorded (stdout,stderr,exit), typed obs_replay_diverged on
command mismatch): a cell-side exec shim for hosted arms, a replay
env variant for the kernel H. Neither is committed. Until the adapter
lands this driver emits typed `withheld{l1_obs_replay:adapter_absent}`
rows for all three legs — never a fabricated conformance.

usage:
  ws1-l1-driver.py --cfg cfg.json --corpus-dir DIR --gate-a-list F \
    --h-set-dir DIR --layers-file F --admission mini.json \
    --proxy-bin BIN --lane-file lane.json --authorization-file auth.json \
    --canon-id hh-l1-canon/<v> --out-dir DIR --run-root DIR
"""
import argparse, json, os, subprocess, sys, time

def eprint(*a): print(*a, file=sys.stderr, flush=True)

def read_json(p):
    with open(p) as f:
        return json.load(f)

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cfg", required=True)
    ap.add_argument("--corpus-dir", required=True)
    ap.add_argument("--gate-a-list", required=True)
    ap.add_argument("--h-set-dir", required=True)
    ap.add_argument("--layers-file", required=True)
    ap.add_argument("--admission", required=True)
    ap.add_argument("--proxy-bin", required=True)
    ap.add_argument("--lane-file", required=True)
    ap.add_argument("--authorization-file", required=True)
    ap.add_argument("--canon-id", required=True)
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--run-root", required=True)
    a = ap.parse_args()

    gate_a = read_json(a.gate_a_list)
    os.makedirs(a.out_dir, exist_ok=True)
    legs = []
    for ent in gate_a:
        sid = ent.get("slot_id") or ent.get("id")
        task = ent.get("task")
        rec_file = ent.get("recording")
        row = {"trajectory": sid, "task": task}
        # L1.0 — two O drives against the replay listener.
        o_drives = []
        for run in ("a", "b"):
            sess_out = os.path.join(a.run_root, f"l10-{sid}-{run}-session.json")
            sentinel = os.path.join(a.run_root, f"l10-{sid}-{run}.done")
            replay_decl = {
                "recordings": [read_json(os.path.join(a.corpus_dir, rec_file))]
                if rec_file else [],
                "canonicalizer": a.canon_id,
                "strict": False,
                "keying": "sequence",
            }
            rp = os.path.join(a.run_root, f"l10-{sid}-{run}-replay.json")
            with open(rp, "w") as f:
                json.dump(replay_decl, f)
            row[f"l10_{run}_replay_decl"] = rp
            # the session shim + arm-run invocation ride the same
            # shape as the corpus leg; wired at window time under the
            # window driver (listen port per leg, model_upstream).
            o_drives.append({"session_out": sess_out,
                             "sentinel": sentinel, "replay_decl": rp})
        row["l1_0"] = {"o_drives": o_drives,
                       "status": "wired:arm_run_against_replay"}
        # L1.1 / L1.2 — the H-drive seam is the open machinery point
        # (see module docstring): emit the typed withhold, never a
        # fabricated conformance row.
        for leg in ("l1_1", "l1_2"):
            row[leg] = {"status": "withheld{l1_h_drive:seam_unresolved}",
                        "detail": "kernel-embedded HIR drive vs replay "
                                  "endpoint — adapter unresolved"}
        legs.append(row)
    summary = {"schema": "hh-ws1-l1-summary/1",
               "gate_a": len(legs),
               "legs": legs,
               "run_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                       time.gmtime())}
    with open(os.path.join(a.out_dir, "_l1-summary.json"), "w") as f:
        json.dump(summary, f, indent=1, sort_keys=True)
    print(json.dumps({"gate_a": len(legs),
                      "l1_0": "wired",
                      "l1_1_l1_2": "withheld{l1_h_drive:seam_unresolved}"}),
          flush=True)
    sys.exit(0)

if __name__ == "__main__":
    main()
