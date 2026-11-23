#!/usr/bin/env python3
"""p4x-main-driver.py — the P4-X window driver (R7.3; ticket 329,
GATE-G35).

The schedule→slot orchestration glue for the phase-4 exploration
window: it mints the session proxy's slot grants
(ws1-proxy-session.py, live mode), runs the `p4x-lane-probe` leg
before the first native slot (REQ-XH-124 — every P4X arm rides the
native `hh-l1-drive --mode live` lane, so the hold binds all eight
executable arms), then dispatches the seeded `random_permuted`
schedule — X0's 3 tasks × 1 replicate × 8 arms and X1's 11 band-A
tasks × k=5 × 8 arms in five rounds — at the declaration's C=4 slot
concurrency with the (task, replicate) unit as the schedule atom
(its 8 members dispatch contiguously in arm order, admitting
member-wise as C slots free — a literal same-instant admit cannot
hold 8 > C and is not what the seeded order requires), with the
round-2 continuation checkpoint folded to `hh-fp-checkpoint/1`.

The arm surface is the sealed P4 level set: `baseline` plus V1–V7,
each a drive-time `document` emitted by `p4_admissibility emit-docs`
(V1/V2 pin effort low/max on the slot grant; V5 drives
`--pack-reasoning drop`; V3/V4/V6/V7 ride their sealed level docs).
V8 and V9 never enter the plan — they are recorded verbatim as
`withheld{inert_at_admissible_window}` per the frozen
preregistration (gen-3 `2026-11-21-fp-p4x-prereg-gen3`,
5db50be845c2…). The frozen stop set is budget /
infrastructure / integrity / catalog_identity only: the
continuation checkpoint never drops a level — a projection that
does not fit stops the batch and the remaining slots record
`withheld{stop:budget}` honestly.

Per-slot evidence: the drive's own out dir + the proxy's durable
WAL + the commit-clone verify leg (cell.rs ST5 ordering — every
P4X slot is a native slot). Secrets ride env only: the slot tokens
are per-slot files and ride the doc's declared
`credential_ref: env:OPENAI_API_KEY` spelling (the sealed identity
is fixed — the env name is part of it; an `overrides[]` pointer to
`credential_ref` is outside inject.rs's tunable coordinate space
and never applies). Values never enter records (`provided: yes/no`
only).

usage (on the compute VM):
  p4x-main-driver.py --cfg cfg.json --w1-bin hh-w1 \
    --l1-drive-bin hh-l1-drive --tasks t1,…,t11 --replicates 5 \
    --docs-dir docs/ --mswea-pack pack/ \
    --proxy-session ws1-proxy-session.py \
    --lane-file lane.json --authorization-file auth.json \
    --pricing pricing.json --store-root STORE --run-id RUN \
    --listen 127.0.0.1:PORT --model M --slot-slice-micros 340000 \
    --concurrency 4 --slot-prefix p4x \
    --seed p4-x1-schedule-r7.2-2026-11-20 \
    --model-budget-micros 157760000 \
    --slot-deadline-s 3738 --harvest-reserve-s 21600 \
    --dead-man-epoch EPOCH --window P4X1 --out RUN_ROOT
"""
import argparse, hashlib, json, os, subprocess, sys, time, urllib.request

# The frozen executable arm-set — V8/V9 never enter it
# (`withheld{inert_at_admissible_window}`, fp-p4x-prereg-gen3
# 5db50be845c2…).
ARMS = ["baseline", "V1", "V2", "V3", "V4", "V5", "V6", "V7"]
WITHHELD_ARMS = ["V8", "V9"]
# Per-arm drive spellings (the sealed level deltas): V1 pins effort
# low, V2 pins effort max on the slot grant; every other arm takes the
# session's declared high. V5's reasoning-replay drop is spelled on
# the drive's --pack-reasoning flag (provider_specific_fields else).
ARM_EFFORT = {"V1": "low", "V2": "max"}
ARM_PACK = {"V5": "drop"}
DEFAULT_EFFORT = "high"
DEFAULT_PACK = "provider_specific_fields"
# fp-e projected per-trial priors (µ$/trial, ms/trial) — used until
# ≥10 measured trials of an arm land (the continuation rule's prior).
P_HAT_PRIOR = {a: 340_000 for a in ARMS}     # §10.4 declared top point
T_HAT_PRIOR = {a: 477_792 for a in ARMS}
PROBE_SLICE_MICROS = 250_000

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

def plan_schedule(tasks, replicates, arms, seed, prefix):
    """The `random_permuted` schedule: units grouped by (task,
    replicate); each unit carries every executable arm's member
    (coupled launch); rounds are the replicate index; within a round,
    units order by the seeded key — the recompute build_schedule
    performs. X0's single-replicate slots carry no -r suffix."""
    rounds = []
    for r in range(1, replicates + 1):
        units = []
        for t in tasks:
            suffix = f"-r{r}" if replicates > 1 else ""
            members = [{"slot_id": f"{prefix}-{a}-{t}{suffix}",
                        "arm": a, "task": t, "replicate": r}
                       for a in arms]
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
    """`hh-lane-probe/1` — one receipted call through the loopback
    listener at the session's declared high, before the first native
    slot (REQ-XH-124; holds.rs's committed shape)."""
    probe_id = "p4x-lane-probe"
    grant = (sess.get("grants") or {}).get(probe_id) or {}
    token = grant.get("slot_token")
    if not token:
        rec = {"schema": "hh-lane-probe/1", "stage": "lane-probe",
               "window": a.window, "gate": "G-P4-X",
               "disposition": "failed{no_slot_grant}",
               "evidence_label": "live-executed",
               "recorded_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                            time.gmtime())}
        return rec, False
    body = {"model": a.model, "max_tokens": 8, "stream": False,
            "reasoning_effort": DEFAULT_EFFORT,
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
           "window": a.window, "gate": "G-P4-X",
           "slot": {"token_ref": f"slot:{probe_id}",
                    "slice_micros": PROBE_SLICE_MICROS},
           "plan": {"calls": 1, "cap_micros": PROBE_SLICE_MICROS},
           "calls": [{"probe_id": "lane-probe-high", "surface": "openai.chat",
                      "route": "/v1/chat/completions", "disposition": disp,
                      "receipt_ref": last.get("response_ref") or "",
                      "cost_micros": 0}],
           "precedes": "first_native_slot",
           "disposition": "pass" if disp == "pass" else "failed",
           "evidence_label": "live-executed",
           "recorded_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                        time.gmtime())}
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

def task_verify_deadline_s(tasks_dir, task, cfg):
    """The D15 mirror in the driver's own narrow reader (task_toml.rs):
    `[verifier].timeout_sec` out of the task's own task.toml when it
    declares one, else the cfg's `verify_deadline_ms` bound recorded
    `cfg_default`. A malformed declared value is a typed error, never
    a silent default."""
    toml_path = os.path.join(tasks_dir, task, "task.toml")
    try:
        in_verifier = False
        with open(toml_path) as tf:
            lines = list(tf)
        for line in lines:
            s = line.split("#", 1)[0].strip()
            if s.startswith("["):
                in_verifier = s == "[verifier]"
                continue
            if in_verifier and s.startswith("timeout_sec"):
                v = float(s.split("=", 1)[1].strip())
                if v <= 0:
                    raise ValueError(
                        f"{toml_path}: [verifier].timeout_sec {v} "
                        "is not positive")
                return v, "task_toml"
    except FileNotFoundError:
        pass
    return ((cfg.get("verify_deadline_ms") or 600000) / 1000.0,
            "cfg_default")


def image_workdir(image):
    """`podman image inspect … .Config.WorkingDir` (empty declares →
    the caller's cfg fallback, never a fabricated `/`)."""
    if not image:
        return None
    try:
        p = subprocess.run(["podman", "image", "inspect", image,
                            "--format", "{{.Config.WorkingDir}}"],
                           capture_output=True, text=True, timeout=30)
        w = p.stdout.strip()
        return w if p.returncode == 0 and w else None
    except Exception:
        return None


def run_verifier(a, m, slot_dir):
    """Post-drive verify — the commit-clone form (R6.56 continuation;
    cell.rs ST5's ordering for the native lane, which is every P4X
    arm). After the drive settles, the still-live cell is `podman
    commit`ed into a verify image and a verify clone runs with: the
    declared verify egress — `bridge`, honestly recorded; the task's
    `tests/` staged at `/tests` read-only (tests enter the cell only
    at verify time); `/logs/verifier` bind-mounted to the slot's
    `collect/`; and cwd = the task image's declared WORKDIR (cfg
    fallback). Both cells are then removed (honest teardown)."""
    ws = os.path.abspath(os.path.join(slot_dir, "ws"))
    hit = None
    for c in podman_containers():
        if any(os.path.abspath(mm) == ws for mm in c["mounts"]):
            hit = c
            break
    if not hit:
        return {"disposition": "verifier_unavailable{env_closed_or_unmounted}"}
    cid = hit["id"]
    vimg = None
    try:
        tests_src = os.path.join(a.tasks_dir, m["task"], "tests")
        if not (os.path.isdir(tests_src)
                and os.path.isfile(os.path.join(tests_src, "test.sh"))):
            return {"disposition":
                    "verifier_unavailable{no_test_in_package}",
                    "detail": tests_src}
        # ST1 mirror — `/tests` present before the verify phase is a
        # phase-boundary breach, never silently overwritten.
        p = subprocess.run(["podman", "exec", cid, "test", "-e", "/tests"],
                           capture_output=True, timeout=30)
        if p.returncode == 0:
            return {"disposition":
                    "verifier_unavailable{phase_boundary:tests_present}"}
        verify_s, src = task_verify_deadline_s(
            a.tasks_dir, m["task"], a.cfg_obj)
        tag = hashlib.sha256(m["slot_id"].encode()).hexdigest()[:16]
        vimg = f"hh-verify-{tag}"
        vname = f"hh-verify-run-{tag}"
        p = subprocess.run(["podman", "commit", cid, vimg],
                           capture_output=True, text=True, timeout=300)
        if p.returncode != 0:
            return {"disposition": "verifier_unavailable{commit_failed}",
                    "stderr_tail": p.stderr[-2000:]}
        collect = os.path.join(slot_dir, "collect")
        os.makedirs(collect, exist_ok=True)
        img_ref = a.task_images.get(m["task"])
        if isinstance(img_ref, dict):
            img_ref = img_ref.get("image")
        workdir = (image_workdir(img_ref)
                   or a.cfg_obj.get("workdir_default") or "/app")
        run_argv = ["podman", "run", "--rm", "--name", vname,
                    "--network", "bridge",
                    "-v", f"{ws}:/work:rw",
                    "-v", f"{tests_src}:/tests:ro",
                    "-v", f"{collect}:/logs/verifier:rw",
                    "--workdir", workdir,
                    vimg, "bash", "/tests/test.sh"]
        try:
            p = subprocess.run(run_argv, capture_output=True, text=True,
                               timeout=verify_s)
            row = {"disposition": "recorded", "returncode": p.returncode,
                   "stdout_tail": p.stdout[-4000:],
                   "stderr_tail": p.stderr[-4000:],
                   "verify_cell": "commit_clone",
                   "verify_egress": "bridge",
                   "workdir": workdir,
                   "deadline_s": verify_s, "deadline_source": src}
        except subprocess.TimeoutExpired:
            subprocess.run(["podman", "stop", "-t", "5", vname],
                           capture_output=True)
            subprocess.run(["podman", "rm", "-f", vname],
                           capture_output=True)
            row = {"disposition":
                   "verifier_unavailable{verifier_timeout}",
                   "verify_cell": "commit_clone",
                   "deadline_s": verify_s, "deadline_source": src}
        # The reward + CTRF bytes landed host-side through the collect
        # mount — report the reward file verbatim like the W4 leg did.
        for name in ("reward.txt", "reward.json"):
            rp = os.path.join(collect, name)
            if os.path.isfile(rp):
                with open(rp) as f:
                    row["reward_file"] = f.read().strip()[:2000]
                break
        return row
    except Exception as e:
        return {"disposition": "verifier_unavailable{exec_error}",
                "detail": str(e)}
    finally:
        subprocess.run(["podman", "rm", "-f", cid], capture_output=True)
        if vimg:
            subprocess.run(["podman", "rmi", "-f", vimg],
                           capture_output=True)

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
    if isinstance(image, dict):
        image = image.get("image")
    if not image:
        logf.write("withheld{corpus_infra:image_absent}\n")
        logf.flush()
        return None, logf
    # Every P4X arm is the native lane: `hh-l1-drive --mode live`
    # with the session's declared `local_container` env — the task
    # image as the cell, model calls on the proxy's loopback with
    # the slot token bound via the doc's sealed
    # `credential_ref: env:OPENAI_API_KEY`.
    doc = os.path.join(a.docs_dir, f"{arm}.json")
    if not os.path.isfile(doc):
        logf.write(f"withheld{{corpus_infra:arm_doc_absent:{arm}}}\n")
        logf.flush()
        return None, logf
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
    with open(doc) as f:
        body = read_json(doc)
    with open(docf, "w") as f:
        f.write(canonical(body))
    # the slot dir keeps the arm's sealed identity beside the doc.
    with open(os.path.join(slot_dir, "arm.json"), "w") as f:
        json.dump({"arm_id": arm, "task": m["task"],
                   "replicate": m["replicate"],
                   "effort": ARM_EFFORT.get(arm, DEFAULT_EFFORT),
                   "pack_reasoning": ARM_PACK.get(arm, DEFAULT_PACK)},
                  f, indent=1, sort_keys=True)
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
           "--pack-reasoning", ARM_PACK.get(arm, DEFAULT_PACK),
           "--mode", "live", "--max-submits", "1",
           "--out", slot_dir]
    env = dict(os.environ)
    # The sealed doc's credential_ref is `env:OPENAI_API_KEY` — the
    # slot token must ride that env name (the attempted overrides[]
    # rewrite to env:HH_L1_API_KEY is outside the tunable coordinate
    # space and is inert; recorded here so the slot evidence keeps
    # showing the attempt verbatim).
    env["OPENAI_API_KEY"] = token
    eprint(f"{sid}: launch ({arm}) credential_ref=env:OPENAI_API_KEY")
    return subprocess.Popen(cmd, stdout=logf, stderr=subprocess.STDOUT,
                            env=env), logf

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cfg", required=True)
    ap.add_argument("--w1-bin", required=True)
    ap.add_argument("--l1-drive-bin", required=True)
    ap.add_argument("--tasks", required=True)
    ap.add_argument("--replicates", type=int, default=5)
    ap.add_argument("--docs-dir", required=True,
                    help="dir holding <arm>.json drive docs — "
                         "baseline.json + V1..V7 from "
                         "`p4_admissibility emit-docs`")
    ap.add_argument("--mswea-pack", required=True)
    ap.add_argument("--proxy-session", required=True)
    ap.add_argument("--lane-file", required=True)
    ap.add_argument("--authorization-file", required=True)
    ap.add_argument("--pricing", required=True)
    ap.add_argument("--store-root", required=True)
    ap.add_argument("--run-id", required=True)
    ap.add_argument("--listen", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--slot-slice-micros", type=int, required=True)
    ap.add_argument("--concurrency", type=int, default=4)
    ap.add_argument("--slot-deadline-s", type=int, default=3738)
    ap.add_argument("--harvest-reserve-s", type=int, default=21_600)
    ap.add_argument("--dead-man-epoch", type=float, required=True)
    ap.add_argument("--checkpoint-round", type=int, default=2)
    ap.add_argument("--slot-prefix", default="p4x",
                    help="slot-id prefix — p4x0 for X0, p4x for X1")
    ap.add_argument("--seed", required=True,
                    help="the frozen permutation seed for this leg "
                         "(p4-x0-schedule-r7.2-2026-11-20 / "
                         "p4-x1-schedule-r7.2-2026-11-20)")
    ap.add_argument("--model-budget-micros", type=int,
                    default=157_760_000)
    ap.add_argument("--out", required=True)
    ap.add_argument("--window", default="P4X1",
                    help="window label on emitted records (P4X0 / P4X1)")
    ap.add_argument("--plan-only", action="store_true")
    a = ap.parse_args()

    tasks = a.tasks.split(",")
    cfg = read_json(a.cfg)
    a.cfg_obj = cfg
    a.tasks_dir = cfg["tasks_dir"]
    a.task_images = read_json(cfg["images_file"])
    a.model_upstream = int(a.listen.rsplit(":", 1)[1])
    a.docs_dir = a.docs_dir
    arms = list(ARMS)
    for arm in arms:
        doc = os.path.join(a.docs_dir, f"{arm}.json")
        if not os.path.isfile(doc):
            eprint(f"missing drive doc: {doc}")
            return 2
    rounds = plan_schedule(tasks, a.replicates, arms, a.seed,
                           a.slot_prefix)

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
    slot_lines = [f"p4x-lane-probe:{PROBE_SLICE_MICROS}:{DEFAULT_EFFORT}"]
    for units in rounds:
        for u in units:
            for m in u["members"]:
                eff = ARM_EFFORT.get(m["arm"], DEFAULT_EFFORT)
                slot_lines.append(
                    f"{m['slot_id']}:{a.slot_slice_micros}:{eff}")
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
    window = {"schema": "hh-fp-window/1", "window": a.window,
              "gate": "G-P4-X",
              "run_id": a.run_id, "seed": a.seed,
              "slot_prefix": a.slot_prefix,
              "arms": arms,
              # the frozen withheld level-set, verbatim — never in the
              # plan, never priced, never driven.
              "withheld_arms": [
                  {"level_id": w,
                   "disposition":
                   "withheld{inert_at_admissible_window}"}
                  for w in WITHHELD_ARMS],
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
              "lane_probe": None}

    probe_rec, probe_ok = run_lane_probe(a, sess, a.out)
    window["lane_probe"] = probe_rec
    native_held = not probe_ok
    # the native-lane hold binds every P4X arm — all eight ride the
    # native lane (REQ-XH-124 names the lane, not one arm).
    held_arms = {a_: "withheld{admission:native_lane_hold}"
                 for a_ in arms} if native_held else {}

    running = {}      # slot_id -> (proc, logf, member, slot_dir, started)
    infra_native = [] # first-3 native-slot infra outcomes
    stopped = None    # a frozen stop-class disposition, when the
                      # budget checkpoint fails the batch
    measured_costs = {a_: [] for a_ in arms}
    measured_walls = {a_: [] for a_ in arms}

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
            row["verifier"] = run_verifier(a, m, sd)
            evs = read_events(sess.get("store_root") or a.store_root)
            row["cost_micros"] = slot_cost_micros(evs, sid)
            with open(os.path.join(sd, "slot-result.json"), "w") as f:
                json.dump(row, f, indent=1, sort_keys=True)
            results[sid] = row
            measured_costs[m["arm"]].append(row["cost_micros"])
            measured_walls[m["arm"]].append(row["wall_ms"])
            # every arm is the native lane — the first-3 infra probe
            # reads any settled slot (REQ-XH-124 names the lane).
            if len(infra_native) < 3:
                infra_native.append(rc != 0 or row["cost_micros"] == 0)
            done.append(sid)
        for sid in done:
            del running[sid]

    def checkpoint(rounds_done):
        """hh-fp-checkpoint/1 — project remaining slots from measured
        (or prior) p̂/t̂. The frozen stop set has no level dropping: a
        projection that does not fit stops the batch (budget class)
        and the checkpoint says so honestly."""
        evs = read_events(sess.get("store_root") or a.store_root)
        consumed = sum(slot_cost_micros(evs, m["slot_id"])
                       for units in rounds for u in units
                       for m in u["members"])
        remaining_units = [u for units in rounds[rounds_done:]
                           for u in units]
        rem = [m for u in remaining_units for m in u["members"]
               if m["slot_id"] not in results]
        def est(vals, prior):
            if len(vals) >= 10:
                import statistics
                mean = statistics.mean(vals)
                p90 = sorted(vals)[int(0.9 * (len(vals) - 1))]
                return 1.2 * max(1.5 * mean, p90)
            return prior
        cost = sum(est(measured_costs[m["arm"]],
                       P_HAT_PRIOR[m["arm"]]) for m in rem)
        tmax = max((est(measured_walls[a_], T_HAT_PRIOR[a_])
                    for a_ in arms), default=1)
        # member-slot makespan — the remaining slots at the slowest
        # arm's t̂ spread over C slot capacity (the P4X unit is 8
        # members > C; units dispatch member-wise in order, so the
        # makespan is a slot count, not a unit count).
        wall_ms = len(rem) * tmax / max(1, a.concurrency)
        deadline_ms = max(0, (a.dead_man_epoch - time.time()
                              - a.harvest_reserve_s) * 1000)
        fits = (cost + consumed <= a.model_budget_micros
                and wall_ms <= deadline_ms)
        ckpt = {"schema": "hh-fp-checkpoint/1", "label":
                f"{a.slot_prefix}-round-{a.checkpoint_round}",
                "consumed_micros": consumed,
                "projection": {"remaining_cost_micros": int(cost),
                               "remaining_wall_ms": int(wall_ms),
                               "room_ms": int(deadline_ms)},
                # the frozen design drops nothing — a misfit stops
                # the batch at the budget class, verbatim.
                "drops": [],
                "batch_fits": fits,
                "stop": None if fits else "stop{budget}",
                "recorded_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                             time.gmtime())}
        with open(os.path.join(a.out, "checkpoint-r2.json"), "w") as f:
            json.dump(ckpt, f, indent=1, sort_keys=True)
        window["checkpoint"] = ckpt
        return fits

    # ── the schedule drive ────────────────────────────────────────────
    for ri, units in enumerate(rounds, start=1):
        if ri == a.checkpoint_round + 1 and stopped is None:
            if not checkpoint(a.checkpoint_round):
                stopped = "withheld{stop:budget}"
        for u in units:
            live_members = list(u["members"])
            if stopped is not None:
                for m in live_members:
                    if m["slot_id"] not in results:
                        withhold(u, m, stopped)
                continue
            if not slot_deadline_fits():
                for m in live_members:
                    if m["slot_id"] not in results:
                        withhold(u, m, "withheld{window_deadline}")
                continue
            for m in live_members:
                if m["arm"] in held_arms:
                    withhold(u, m, held_arms[m["arm"]])
            live_members = [m for m in live_members
                            if m["arm"] not in held_arms]
            if not live_members:
                continue
            # unit-ordered dispatch under C — the unit stays the
            # schedule atom: all of its members dispatch, contiguously
            # in arm order, before any member of a later unit (a unit
            # never interleaves with another). A P4X unit carries 8
            # members > C=4, so members admit one at a time as a slot
            # frees — capacity gates in-flight slots, never the unit's
            # membership. A member whose own deadline no longer fits
            # withholds honestly; already-launched members settle.
            for m in live_members:
                while len(running) >= a.concurrency:
                    reap()
                    # the native-lane hold: first-3 native slots
                    # settled, any infra-failed → withhold every arm
                    # (REQ-XH-124)
                    if not native_held and len(infra_native) >= 3 \
                            and any(infra_native):
                        native_held = True
                        held_arms = {
                            a_: "withheld{admission:native_lane_hold}"
                            for a_ in arms}
                    time.sleep(2.0)
                if stopped is not None:
                    withhold(u, m, stopped)
                    continue
                if m["arm"] in held_arms:
                    withhold(u, m, held_arms[m["arm"]])
                    continue
                if not slot_deadline_fits():
                    withhold(u, m, "withheld{window_deadline}")
                    continue
                sd = os.path.join(slots_dir, m["slot_id"])
                p, lf = launch_member(a, sess, m, sd)
                if p is None:
                    withhold(u, m,
                             "withheld{corpus_infra:launch}")
                else:
                    running[m["slot_id"]] = (p, lf, m, sd, time.time())
                # post-launch infra probe + harvest of finished slots
                if not native_held and len(infra_native) >= 3 \
                        and any(infra_native):
                    native_held = True
                    held_arms = {
                        a_: "withheld{admission:native_lane_hold}"
                        for a_ in arms}
                reap()
    while running:
        reap()
        time.sleep(3.0)

    window["ended_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                       time.gmtime())
    window["slot_results"] = results
    window["native_lane_hold"] = native_held
    if stopped is not None:
        window["stop_disposition"] = stopped
    with open(os.path.join(a.out, f"{a.slot_prefix}-window.json"),
              "w") as f:
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
                      "stop": stopped,
                      "window": os.path.join(
                          a.out, f"{a.slot_prefix}-window.json")}))
    return 0

if __name__ == "__main__":
    sys.exit(main())
