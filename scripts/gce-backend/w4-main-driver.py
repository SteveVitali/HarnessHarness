#!/usr/bin/env python3
"""w4-main-driver.py — the W4 main-rounds driver (R6.52; spec §3.9,
ADR-0452).

The schedule→slot orchestration glue the W4 window never had: it mints
the session proxy's slot grants (ws1-proxy-session.py, live mode), runs
the `w4-lane-probe` leg before the first HH-native slot (REQ-XH-124),
then dispatches the pinned `composed` k=5 coupled-pair schedule into the
already-landed per-slot executors — `hh-w1 arm-run` for the hosted O
side (mini-swe-agent) and `hh-l1-drive --mode live` for the HH-native
lane (hh_native baseline + V5 `rsn-drop` level) — at the declaration's
C=4 slot concurrency, pair-atomic under the dead-man cut-off, with the
round-2 continuation checkpoint (cost+wall, bottom-of-drop-order first)
folded to `hh-fp-checkpoint/1`.

Per-slot evidence: the drive's own out dir + the proxy's durable WAL
(every upstream-bound call receipted on the slot token). The HH-native
verifier rides the still-live container the drive's `local_container`
env leaves behind (the helper's `PreserveUntil` custody, 24 h): the
driver `podman exec`s `/tests/test.sh` into the container named under
the slot's own store root hash, then removes it honestly.

Pi and Claude Code never appear in the plan — `not_run{admission:AG3}`
(ADR-0452; no PP0 evidence). Secrets ride env only: the slot tokens are
per-slot files, `HH_L1_API_KEY` names the credential env the model_port
`credential_ref` binds — values never enter records (`provided: yes/no`
only).

usage (on the compute VM):
  w4-main-driver.py --cfg cfg.json --w1-bin hh-w1 --l1-drive-bin hh-l1-drive \
    --tasks t1,…,t10 --admission-mini mini.json --hir-doc mswea.hir.json \
    --v5-doc v5-level.json --mswea-pack pack/ --proxy-session ws1-proxy-session.py \
    --lane-file lane.json --authorization-file auth.json --pricing pricing.json \
    --store-root STORE --run-id RUN --listen 127.0.0.1:PORT \
    --model M --slot-slice-micros N --concurrency 4 \
    --slot-deadline-s 3738 --harvest-reserve-s 21600 \
    --dead-man-epoch EPOCH --seed SEED --out RUN_ROOT
"""
import argparse, hashlib, json, os, subprocess, sys, time, urllib.request

ARMS = ["mini-swe-agent", "hh_native", "v5"]          # drop order: v5 last…
DROP_ORDER = ["v5", "claude-code", "pi"]            # …protected pair never in it
PROTECTED = ["mini-swe-agent", "hh_native"]
# fp-e projected per-trial priors (µ$/trial, ms/trial) — used until ≥10
# measured trials of an arm land (the continuation rule's own prior leg).
P_HAT_PRIOR = {"mini-swe-agent": 386_163, "hh_native": 374_000, "v5": 374_000}
T_HAT_PRIOR = {"mini-swe-agent": 477_792, "hh_native": 477_792, "v5": 477_792}
PROBE_SLICE_MICROS = 50_000

def eprint(*a): print(*a, file=sys.stderr)

def sha256_hex(b): return hashlib.sha256(b).hexdigest()

def canonical(j):
    """The wire codec's canonical spell — sorted keys, tight separators."""
    return json.dumps(j, sort_keys=True, separators=(",", ":"))

def seeded_key(seed, parts):
    """Mirrors build_schedule's `seeded_key` — sha256 over `seed|p1|p2…`."""
    return sha256_hex("|".join([seed] + [str(p) for p in parts]).encode())

def read_json(p):
    with open(p) as f:
        return json.load(f)

def read_events(store_root):
    evs = []
    for dp, _, fns in os.walk(store_root):
        for fn in fns:
            if fn != "events.wal":
                continue
            for line in open(os.path.join(dp, fn), errors="replace"):
                line = line.strip()
                if not line:
                    continue
                try:
                    rec = json.loads(line)
                except Exception:
                    continue
                if isinstance(rec, dict) and rec.get("k") == "e":
                    rec = rec.get("v") or {}
                evs.append(rec)
    return evs

def plan_schedule(tasks, replicates, arms, seed):
    """The `composed` k=5 schedule: units grouped by (task, replicate);
    each unit carries every included arm's member (coupled launch);
    rounds are the replicate index; within a round, units order by the
    seeded key — the recompute build_schedule performs."""
    rounds = []
    for r in range(1, replicates + 1):
        units = []
        for t in tasks:
            members = [{"slot_id": f"w4-{a}-{t}-r{r}", "arm": a,
                        "task": t, "replicate": r} for a in arms]
            units.append({"task": t, "replicate": r,
                          "order_key": seeded_key(seed, [t, r]),
                          "members": members})
        units.sort(key=lambda u: u["order_key"])
        rounds.append(units)
    return rounds

def slot_cost_micros(evs, slot_id):
    total = 0
    for e in evs:
        if e.get("class") != "measurement.cost.attributed":
            continue
        p = e.get("payload") or {}
        if slot_id in json.dumps(p):
            total += (p.get("money") or {}).get("micro_units", 0)
    return total

def http_call(listen, route, body, token, timeout=600):
    req = urllib.request.Request(
        f"http://{listen}{route}", data=json.dumps(body).encode(),
        headers={"content-type": "application/json",
                 "authorization": f"Bearer {token}"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            r.read()
            return r.status
    except urllib.error.HTTPError as he:
        he.read()
        return he.code
    except Exception:
        return -1

def run_lane_probe(a, sess, out_dir):
    """`hh-lane-probe/1` — one receipted call per pinned effort path on
    the probe slot token through the loopback listener, before the
    first HH-native slot (REQ-XH-124; holds.rs's committed shape)."""
    probe_id = "w4-lane-probe"
    grant = (sess.get("grants") or {}).get(probe_id) or {}
    token = grant.get("slot_token")
    if not token:
        rec = {"schema": "hh-lane-probe/1", "stage": "lane-probe",
               "window": "W4", "gate": "FP-E",
               "disposition": "failed{no_slot_grant}",
               "evidence_label": "live-executed",
               "recorded_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
        return rec, False
    body = {"model": a.model, "max_tokens": 8, "stream": False,
            "reasoning_effort": a.effort,
            "messages": [{"role": "user", "content":
                          "HH-native lane probe — answer with the single token ok."}]}
    status = http_call(a.listen, "/v1/chat/completions", body, token)
    time.sleep(1.0)  # the receipt lands durable before the fold
    evs = read_events(sess.get("store_root") or a.store_root)
    calls = [e for e in evs
             if e.get("class") in ("model.call.completed", "model.call.failed")]
    last = (calls[-1].get("payload") or {}) if calls else {}
    disp = "pass" if 200 <= status < 300 else f"unreceipted{{http:{status}}}"
    rec = {"schema": "hh-lane-probe/1", "stage": "lane-probe",
           "window": "W4", "gate": "FP-E",
           "slot": {"token_ref": f"slot:{probe_id}",
                    "slice_micros": PROBE_SLICE_MICROS},
           "plan": {"calls": 1, "cap_micros": PROBE_SLICE_MICROS},
           "calls": [{"probe_id": "lane-probe-high", "surface": "openai.chat",
                      "route": "/v1/chat/completions", "disposition": disp,
                      "receipt_ref": last.get("response_ref") or "",
                      "cost_micros": 0}],
           "precedes": "first_hh_native_slot",
           "disposition": "pass" if disp == "pass" else "failed",
           "evidence_label": "live-executed",
           "recorded_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())}
    with open(os.path.join(out_dir, "lane-probe.json"), "w") as f:
        json.dump(rec, f, indent=1, sort_keys=True)
    return rec, disp == "pass"

def podman_containers():
    try:
        p = subprocess.run(["podman", "ps", "-a", "--format", "{{.ID}}"],
                           capture_output=True, text=True, timeout=30)
        ids = [l.strip() for l in p.stdout.splitlines() if l.strip()]
        out = []
        if not ids:
            return out
        q = subprocess.run(["podman", "inspect"] + ids,
                           capture_output=True, text=True, timeout=60)
        for c in json.loads(q.stdout):
            mounts = [m.get("Source", "") for m in c.get("Mounts", [])]
            out.append({"id": c.get("Id", ""), "name": c.get("Name", ""),
                        "mounts": mounts})
        return out
    except Exception:
        return []

def run_verifier(slot_dir):
    """Post-drive verify: the drive's `local_container` env leaves the
    podman container alive under `PreserveUntil` custody; find the one
    bind-mounting this slot's workspace, exec `/tests/test.sh`, then
    remove it (honest teardown — never a leaked billable)."""
    ws = os.path.abspath(os.path.join(slot_dir, "ws"))
    hit = None
    for c in podman_containers():
        if any(os.path.abspath(m) == ws for m in c["mounts"]):
            hit = c
            break
    if not hit:
        return {"disposition": "verifier_unavailable{env_closed_or_unmounted}"}
    cid = hit["id"]
    try:
        p = subprocess.run(
            ["podman", "exec", cid, "bash", "/tests/test.sh"],
            capture_output=True, text=True, timeout=600)
        row = {"disposition": "recorded", "returncode": p.returncode,
               "stdout_tail": p.stdout[-4000:], "stderr_tail": p.stderr[-4000:]}
        # the TB reward file — read it out of the same container when present
        q = subprocess.run(
            ["podman", "exec", cid, "bash", "-c",
             "cat /logs/verifier/reward.txt 2>/dev/null || "
             "cat /logs/verifier/reward.json 2>/dev/null || true"],
            capture_output=True, text=True, timeout=60)
        if q.stdout.strip():
            row["reward_file"] = q.stdout.strip()[:2000]
    except Exception as e:
        row = {"disposition": "verifier_unavailable{exec_error}",
               "detail": str(e)}
    subprocess.run(["podman", "rm", "-f", cid], capture_output=True)
    return row

def launch_member(a, sess, m, slot_dir):
    """Spawn one slot member; returns (proc, logfile_handle)."""
    sid = m["slot_id"]
    arm = m["arm"]
    grant = (sess.get("grants") or {}).get(sid) or {}
    token = grant.get("slot_token")
    os.makedirs(slot_dir, exist_ok=True)
    logf = open(os.path.join(slot_dir, "driver.log"), "w")
    if not token:
        logf.write("withheld{corpus_infra:grant_absent}\n")
        logf.flush()
        return None, logf
    tok_file = os.path.join(slot_dir, "slot-token")
    with open(tok_file, "w") as f:
        f.write(token)
    os.chmod(tok_file, 0o600)
    image = a.task_images.get(m["task"])
    if not image:
        logf.write("withheld{corpus_infra:image_absent}\n")
        logf.flush()
        return None, logf
    if arm == "mini-swe-agent":
        cmd = [a.w1_bin, "arm-run", "--cfg", a.cfg, "--task", m["task"],
               "--image", image, "--admission", a.admission_mini,
               "--slot-id", sid, "--slot-token-file", tok_file,
               "--model-upstream", str(a.model_upstream),
               "--slot-slice-micros", str(a.slot_slice_micros),
               "--out", slot_dir]
        env = None
    else:
        # The HH-native lane (hh_native + V5): `hh-l1-drive --mode live`
        # with the session's declared `local_container` env — the task
        # image as the cell, model calls on the proxy's loopback with
        # the slot token bound via `credential_ref: env:HH_L1_API_KEY`.
        doc = a.hir_doc if arm == "hh_native" else a.v5_doc
        stage = os.path.join(slot_dir, "ws")
        os.makedirs(stage, exist_ok=True)
        inst = os.path.join(slot_dir, "instruction.md")
        src_inst = os.path.join(a.tasks_dir, m["task"], "instruction.md")
        if not os.path.exists(src_inst):
            src_inst = os.path.join(a.tasks_dir, f"{m['task']}.md")
        with open(src_inst) as f:
            with open(inst, "w") as g:
                g.write(f.read())
        docf = os.path.join(slot_dir, "definition.json")
        src = read_json(doc)
        if src.get("schema") == "hh-fp-sweep-level/1":
            # the V5 level record — the bound doc lives under
            # `sealed.document`; the slot dir keeps the level identity.
            if (src.get("level_id") != "V5" or
                    src.get("factor_id") !=
                    "transcript_render.assistant_replay.reasoning"):
                logf.write("withheld{corpus_infra:v5_level_mismatch}\n")
                logf.flush()
                return None, logf
            body = (src.get("sealed") or {}).get("document")
            with open(os.path.join(slot_dir, "level.json"), "w") as f:
                json.dump({k: src.get(k) for k in
                           ("schema", "level_id", "level_name",
                            "factor_id", "level_ref", "baseline_ref",
                            "override_spellings")}, f, indent=1,
                          sort_keys=True)
        else:
            body = src
        with open(docf, "w") as f:
            f.write(canonical(body))
        ovf = os.path.join(slot_dir, "overrides.json")
        with open(ovf, "w") as f:
            json.dump([{"pointer":
                        "/assembly/slots/model_port/one/params/credential_ref",
                        "value": "env:HH_L1_API_KEY"}], f)
        conn = {"class": "local_container", "image": image,
                "exec_allow": ["/"], "workspace_roots": [stage]}
        cmd = [a.l1_drive_bin, "--store-root", os.path.join(slot_dir, "store"),
               "--workspace-root", stage, "--holder", "hh-native",
               "--definition", docf, "--override", ovf,
               "--environment", json.dumps({"kind": "connection_info",
                                            "connection_info": conn}),
               "--instruction", inst,
               "--endpoint", f"http://127.0.0.1:{a.model_upstream}/v1",
               "--model", a.model, "--mswea-pack", a.mswea_pack,
               "--pack-reasoning",
               "drop" if arm == "v5" else "provider_specific_fields",
               "--mode", "live", "--max-submits", "1",
               "--out", slot_dir]
        env = dict(os.environ)
        env["HH_L1_API_KEY"] = token
    eprint(f"{sid}: launch ({arm})")
    return subprocess.Popen(cmd, stdout=logf, stderr=subprocess.STDOUT,
                            env=env), logf

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cfg", required=True)
    ap.add_argument("--w1-bin", required=True)
    ap.add_argument("--l1-drive-bin", required=True)
    ap.add_argument("--tasks", required=True)
    ap.add_argument("--replicates", type=int, default=5)
    ap.add_argument("--admission-mini", required=True)
    ap.add_argument("--hir-doc", required=True)
    ap.add_argument("--v5-doc", required=True)
    ap.add_argument("--mswea-pack", required=True)
    ap.add_argument("--proxy-session", required=True)
    ap.add_argument("--lane-file", required=True)
    ap.add_argument("--authorization-file", required=True)
    ap.add_argument("--pricing", required=True)
    ap.add_argument("--store-root", required=True)
    ap.add_argument("--run-id", required=True)
    ap.add_argument("--listen", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--effort", default="high")
    ap.add_argument("--slot-slice-micros", type=int, required=True)
    ap.add_argument("--concurrency", type=int, default=4)
    ap.add_argument("--slot-deadline-s", type=int, default=3738)
    ap.add_argument("--harvest-reserve-s", type=int, default=21_600)
    ap.add_argument("--dead-man-epoch", type=float, required=True)
    ap.add_argument("--checkpoint-round", type=int, default=2)
    ap.add_argument("--seed", default="fp-w4-schedule-r6.33-2026-11-10")
    ap.add_argument("--model-budget-micros", type=int, default=64_188_120)
    ap.add_argument("--out", required=True)
    ap.add_argument("--plan-only", action="store_true")
    a = ap.parse_args()

    tasks = a.tasks.split(",")
    cfg = read_json(a.cfg)
    a.tasks_dir = cfg["tasks_dir"]
    a.task_images = read_json(cfg["images_file"])
    a.model_upstream = int(a.listen.rsplit(":", 1)[1])
    rounds = plan_schedule(tasks, a.replicates, ARMS, a.seed)

    if a.plan_only:
        print(json.dumps({"rounds": [
            [{"task": u["task"], "replicate": u["replicate"],
              "order_key": u["order_key"],
              "members": [m["slot_id"] for m in u["members"]]}
             for u in units] for units in rounds]}, indent=1))
        return 0

    os.makedirs(a.out, exist_ok=True)
    slots_dir = os.path.join(a.out, "slots")
    os.makedirs(slots_dir, exist_ok=True)

    # ── the session proxy — slot grants before the first call ────────
    slot_lines = [f"w4-lane-probe:{PROBE_SLICE_MICROS}:{a.effort}"]
    for units in rounds:
        for u in units:
            for m in u["members"]:
                slot_lines.append(f"{m['slot_id']}:{a.slot_slice_micros}:{a.effort}")
    sess_file = os.path.join(a.out, "proxy-session.json")
    sentinel = os.path.join(a.out, "proxy.sentinel")
    ps_cmd = ["python3", a.proxy_session, "--proxy-bin",
              cfg.get("proxy_bin", "hh-model-proxy"),
              "--lane-file", a.lane_file,
              "--authorization-file", a.authorization_file,
              "--store-root", a.store_root,
              "--run-id", a.run_id, "--listen", a.listen,
              "--slots", ",".join(slot_lines), "--mode", "live",
              "--seal-recordings",
              "--session-out", sess_file, "--sentinel", sentinel,
              "--max-wait-seconds", "86400"]
    shim = subprocess.Popen(ps_cmd,
                            stdout=open(os.path.join(
                                a.out, "proxy-session.log"), "w"),
                            stderr=subprocess.STDOUT, text=True)
    t0 = time.time()
    sess = None
    while time.time() - t0 < 120:
        if os.path.exists(sess_file):
            try:
                sess = read_json(sess_file)
            except Exception:
                sess = None
            if sess and sess.get("grants"):
                break
        if shim.poll() is not None:
            break
        time.sleep(0.5)
    if not sess or not sess.get("grants"):
        shim.kill()
        eprint("proxy session never provisioned its slot grants")
        return 2

    results = {}
    window = {"schema": "hh-fp-window/1", "window": "W4", "gate": "FP-E",
              "run_id": a.run_id, "seed": a.seed,
              "arms": ARMS, "drop_order": DROP_ORDER,
              "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                          time.gmtime()),
              "dead_man_epoch": a.dead_man_epoch,
              "concurrency": a.concurrency,
              "schedule_rows": [
                  {"round": ri, "task": u["task"],
                   "replicate": u["replicate"],
                   "order_key": u["order_key"],
                   "members": [m["slot_id"] for m in u["members"]]}
                  for ri, units in enumerate(rounds, start=1)
                  for u in units],
              "withheld": [], "checkpoint": None,
              "lane_probe": None, "not_run": {
                  "pi": "not_run{admission:AG3}",
                  "claude-code": "not_run{admission:AG3}"}}

    probe_rec, probe_ok = run_lane_probe(a, sess, a.out)
    window["lane_probe"] = probe_rec
    native_held = not probe_ok
    held_native = {a_: "withheld{admission:native_lane_hold}"
                   for a_ in ("hh_native", "v5")} if native_held else {}

    running = {}      # slot_id -> (proc, logf, member, slot_dir, started)
    infra_native = [] # first-3 native-slot infra outcomes
    dropped = set()
    measured_costs = {a_: [] for a_ in ARMS}
    measured_walls = {a_: [] for a_ in ARMS}

    def slot_deadline_fits():
        return (time.time() + a.slot_deadline_s + a.harvest_reserve_s
                < a.dead_man_epoch)

    def withhold(unit, member, why):
        results[member["slot_id"]] = {"result": why}
        window["withheld"].append({"slot_id": member["slot_id"],
                                   "arm": member["arm"],
                                   "task": member["task"],
                                   "replicate": member["replicate"],
                                   "disposition": why})

    def reap():
        done = []
        for sid, (p, lf, m, sd, st) in list(running.items()):
            rc = p.poll()
            if rc is None:
                continue
            lf.close()
            row = {"slot_id": sid, "arm": m["arm"], "task": m["task"],
                   "replicate": m["replicate"], "rc": rc,
                   "wall_ms": int((time.time() - st) * 1000)}
            if m["arm"] in ("hh_native", "v5"):
                row["verifier"] = run_verifier(sd)
            evs = read_events(sess.get("store_root") or a.store_root)
            row["cost_micros"] = slot_cost_micros(evs, sid)
            with open(os.path.join(sd, "slot-result.json"), "w") as f:
                json.dump(row, f, indent=1, sort_keys=True)
            results[sid] = row
            measured_costs[m["arm"]].append(row["cost_micros"])
            measured_walls[m["arm"]].append(row["wall_ms"])
            # the lane-hold probe measures the hh_native lane only — a v5
            # slot's own doc/participant failure (e.g. the sealed level
            # doc's selector defect surfacing as RefUnresolved in ~2 s,
            # settling long before the first hh_native slot) must not
            # stand in as a native-lane infra failure and withhold the
            # protected pair (REQ-XH-124 names the native lane).
            if m["arm"] == "hh_native" and len(infra_native) < 3:
                infra_native.append(rc != 0 or row["cost_micros"] == 0)
            done.append(sid)
        for sid in done:
            del running[sid]

    def checkpoint(rounds_done):
        """hh-fp-checkpoint/1 (cost fields only) — project remaining
        slots from measured (or prior) p̂/t̂; drop bottom-of-order."""
        evs = read_events(sess.get("store_root") or a.store_root)
        consumed = sum(slot_cost_micros(evs, m["slot_id"])
                       for units in rounds for u in units
                       for m in u["members"])
        remaining_units = [u for units in rounds[rounds_done:]
                           for u in units]
        cur_arms = [a_ for a_ in ARMS if a_ not in dropped]
        def fits(arms_live):
            rem = [m for u in remaining_units for m in u["members"]
                   if m["arm"] in arms_live and
                   m["slot_id"] not in results]
            def est(vals, prior):
                if len(vals) >= 10:
                    import statistics
                    mean = statistics.mean(vals)
                    p90 = sorted(vals)[int(0.9 * (len(vals) - 1))]
                    return 1.2 * max(1.5 * mean, p90)
                return prior
            cost = sum(est(measured_costs[m["arm"]],
                           P_HAT_PRIOR[m["arm"]]) for m in rem)
            # makespan ≈ remaining units × max member t̂ / units-in-flight
            tmax = max((est(measured_walls[a_], T_HAT_PRIOR[a_])
                        for a_ in arms_live), default=1)
            size = max(1, len([a_ for a_ in arms_live
                               if a_ not in held_native]))
            units_in_flight = max(1, a.concurrency // size)
            wall_ms = len(remaining_units) * tmax / units_in_flight
            deadline_ms = max(0, (a.dead_man_epoch - time.time()
                                  - a.harvest_reserve_s) * 1000)
            return (cost + consumed <= a.model_budget_micros
                    and wall_ms <= deadline_ms), cost, wall_ms, deadline_ms
        ok, cost, wall, room = fits(cur_arms)
        drops = []
        while not ok and any(a_ in DROP_ORDER for a_ in cur_arms):
            victim = next(a_ for a_ in DROP_ORDER if a_ in cur_arms)
            cur_arms.remove(victim)
            dropped.add(victim)
            drops.append(victim)
            ok, cost, wall, room = fits(cur_arms)
        ckpt = {"schema": "hh-fp-checkpoint/1", "label": "w4-round-2",
                "consumed_micros": consumed,
                "projection": {"remaining_cost_micros": int(cost),
                               "remaining_wall_ms": int(wall),
                               "room_ms": int(room)},
                "drops": drops,
                "protected_pair_fits": ok,
                "recorded_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                             time.gmtime())}
        with open(os.path.join(a.out, "checkpoint-r2.json"), "w") as f:
            json.dump(ckpt, f, indent=1, sort_keys=True)
        window["checkpoint"] = ckpt
        return ok

    # ── the schedule drive ────────────────────────────────────────────
    for ri, units in enumerate(rounds, start=1):
        if ri == a.checkpoint_round + 1:
            checkpoint(a.checkpoint_round)
        for u in units:
            live_members = [m for m in u["members"]
                            if m["arm"] not in dropped]
            if not slot_deadline_fits():
                for m in live_members:
                    if m["slot_id"] not in results:
                        withhold(u, m, "withheld{window_deadline}")
                continue
            for m in live_members:
                if m["arm"] in held_native:
                    withhold(u, m, held_native[m["arm"]])
            live_members = [m for m in live_members
                            if m["arm"] not in held_native]
            if not live_members:
                continue
            # unit-atomic capacity admit — a pair never splits
            while len(running) + len(live_members) > a.concurrency:
                reap()
                time.sleep(2.0)
            # Capacity waits consume the same admission window.
            if not slot_deadline_fits():
                for m in live_members:
                    withhold(u, m, "withheld{window_deadline}")
                continue
            for m in live_members:
                sd = os.path.join(slots_dir, m["slot_id"])
                p, lf = launch_member(a, sess, m, sd)
                if p is None:
                    withhold(u, m,
                             "withheld{corpus_infra:launch}")
                else:
                    running[m["slot_id"]] = (p, lf, m, sd, time.time())
            # the native-lane hold: first-3 native slots settled, any
            # infra-failed → withhold hh_native + v5 (REQ-XH-124)
            if not native_held and len(infra_native) >= 3:
                if any(infra_native):
                    native_held = True
                    held_native = {
                        a_: "withheld{admission:native_lane_hold}"
                        for a_ in ("hh_native", "v5")}
            reap()
    while running:
        reap()
        time.sleep(3.0)

    window["ended_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    window["slot_results"] = results
    window["native_lane_hold"] = native_held
    with open(os.path.join(a.out, "w4-window.json"), "w") as f:
        json.dump(window, f, indent=1, sort_keys=True)
    # close the session honestly — the sentinel tells the shim to seal
    # recordings, revoke the grants, and stop the proxy.
    try:
        with open(sentinel, "w") as f:
            f.write("done")
        shim.wait(timeout=120)
    except Exception:
        try:
            shim.terminate()
        except Exception:
            pass
    n = len(results)
    recorded = sum(1 for r in results.values()
                   if isinstance(r, dict) and "rc" in r)
    print(json.dumps({"slots": n, "recorded": recorded,
                      "withheld": len(window["withheld"]),
                      "native_lane_hold": native_held,
                      "window": os.path.join(a.out, "w4-window.json")}))
    return 0

if __name__ == "__main__":
    sys.exit(main())
