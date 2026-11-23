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

R6.52 repair (2026-11-23): every continuation decision — the checkpoint
after round 2 and at each later boundary (drained first), declared-cap
expected-cost admission and the per-arm native-lane holds — is the one
canonical module (`crates/hh-runner` continuation/holds) through the
`fp-continuation` adapter, over pinned FP-E inputs from
`--continuation-config` (`hh-w4-continuation-config/1`; refused before
any credential is minted when incomplete). No formula lives here. The
lane probe passes only on a durable receipt bound to the probe slot and
phase run; every planned slot closes as attempted or typed-withheld
(R6.8's closed set); teardown is clean only on the shim's own
acknowledgement.

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
    --fp-continuation-bin fp_continuation --continuation-config cont.json \
    --tasks t1,…,t10 --admission-mini mini.json --hir-doc mswea.hir.json \
    --v5-doc v5-level.json --mswea-pack pack/ --proxy-session ws1-proxy-session.py \
    --lane-file lane.json --authorization-file auth.json --pricing pricing.json \
    --store-root STORE --run-id RUN --listen 127.0.0.1:PORT \
    --model M --slot-slice-micros N --concurrency 4 \
    --slot-deadline-s 3738 --harvest-reserve-s 21600 \
    --dead-man-epoch EPOCH --seed SEED --out RUN_ROOT
"""
import argparse, hashlib, json, os, subprocess, sys, time, urllib.request

ARMS = ["mini-swe-agent", "hh_native", "v5"]   # Pi/CC: not_run{admission:AG3}
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

def _has_exact_str(obj, s):
    """True iff some string value anywhere in `obj` equals `s` exactly —
    never a substring match (a slot id is not a prefix test)."""
    if isinstance(obj, str):
        return obj == s
    if isinstance(obj, dict):
        return any(_has_exact_str(v, s) for v in obj.values())
    if isinstance(obj, list):
        return any(_has_exact_str(v, s) for v in obj)
    return False

def slot_cost_micros(evs, slot_id):
    """Σ metered µ$ attributed to exactly this slot (proxy
    `measurement.cost.attributed` rows)."""
    total = 0
    for e in evs:
        if e.get("class") != "measurement.cost.attributed":
            continue
        p = e.get("payload") or {}
        if _has_exact_str(p, slot_id):
            total += (p.get("money") or {}).get("micro_units", 0)
    return total

def orphaned_call_ids(evs):
    """`model.call.requested` ids with no `completed|failed` terminal —
    the calls the checkpoint must bound at `bounded.hi`."""
    req, term = set(), set()
    for e in evs:
        p = e.get("payload") or {}
        mc = p.get("model_call_id")
        if not mc:
            continue
        if e.get("class") == "model.call.requested":
            req.add(mc)
        elif e.get("class") in ("model.call.completed", "model.call.failed"):
            term.add(mc)
    return sorted(req - term)

# ── the one continuation module (R6.29 X-04/CC1) ─────────────────────
# The driver never re-implements p̂/t̂/cost-fit/W_p/caps/holds: every
# decision is `fp-continuation` (crates/hh-runner continuation_adapter)
# over the canonical rule. A refusal, an invocation failure or a
# mismatched reply stops admission — never a default.

CONT_ADAPTER_SCHEMA = "hh-fp-continuation-adapter/1"
CONT_INPUT_SCHEMA = "hh-fp-continuation-input/1"
CONT_CONFIG_SCHEMA = "hh-w4-continuation-config/1"
CONT_CONFIG_KEYS = ("schema", "root_cap_micros", "c", "slice_micros",
                    "priors_micros", "client_timeouts_ms", "priority",
                    "caps_micros", "cutoff_ms", "overhead_ms", "load_ms",
                    "source_refs")
NATIVE_ARMS = ("hh_native", "v5")
FIVE_ARMS = ("mini-swe-agent", "hh_native", "pi", "claude-code", "v5")

class ContinuationConfigError(ValueError):
    pass

class AdapterRefused(Exception):
    pass

def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)

def load_continuation_config(path):
    """Strict decode of the pinned continuation inputs (FP-E values:
    root cap, C, slice, priors, declared caps, priority, client
    timeouts, admission cut-off, overhead, load, and the source refs
    they were read from). Missing or unknown members refuse — a pinned
    input is never zero or a hard-coded fallback."""
    try:
        cfg = read_json(path)
    except Exception as e:
        raise ContinuationConfigError(f"unreadable: {type(e).__name__}")
    if not isinstance(cfg, dict):
        raise ContinuationConfigError("must be an object")
    missing = [k for k in CONT_CONFIG_KEYS if k not in cfg]
    unknown = sorted(set(cfg) - set(CONT_CONFIG_KEYS))
    if missing or unknown:
        raise ContinuationConfigError(f"missing {missing} unknown {unknown}")
    if cfg["schema"] != CONT_CONFIG_SCHEMA:
        raise ContinuationConfigError(f"schema must be {CONT_CONFIG_SCHEMA}")
    for k in ("root_cap_micros", "c", "slice_micros", "cutoff_ms",
              "overhead_ms", "load_ms"):
        if not _is_int(cfg[k]) or cfg[k] < 0:
            raise ContinuationConfigError(f"{k} must be a non-negative integer")
    if cfg["c"] < 1:
        raise ContinuationConfigError("c must be ≥ 1")
    for k, need in (("priors_micros", FIVE_ARMS), ("caps_micros", ()),
                    ("client_timeouts_ms", ())):
        v = cfg[k]
        if not isinstance(v, dict) or not all(_is_int(x) for x in v.values()):
            raise ContinuationConfigError(f"{k} must map names to integers")
        absent = [a for a in need if a not in v]
        if absent:
            raise ContinuationConfigError(f"{k} lacks {absent}")
    pr = cfg["priority"]
    if (not isinstance(pr, dict) or set(pr) != {"protected", "drop_order"}
            or not all(isinstance(pr[k], list) for k in pr)):
        raise ContinuationConfigError("priority must be {protected, drop_order}")
    for arm in ARMS:
        if arm not in pr["protected"] and arm not in cfg["caps_micros"]:
            raise ContinuationConfigError(
                f"caps_micros lacks the declared cap of unprotected arm {arm}")
    refs = cfg["source_refs"]
    if (not isinstance(refs, dict) or not refs
            or not all(isinstance(v, str) and v for v in refs.values())):
        raise ContinuationConfigError("source_refs must pin ≥ 1 record")
    return cfg

def call_adapter(bin_path, req, timeout=60):
    """One `fp-continuation` round trip. Any failure → AdapterRefused."""
    try:
        p = subprocess.run([bin_path], input=canonical(req),
                           capture_output=True, text=True, timeout=timeout)
    except Exception as e:
        raise AdapterRefused(f"invoke:{type(e).__name__}")
    try:
        body = json.loads(p.stdout)
    except Exception:
        raise AdapterRefused(f"output_unparsable:rc={p.returncode}")
    if not isinstance(body, dict):
        raise AdapterRefused("output_not_object")
    if p.returncode != 0 or "refused" in body:
        raise AdapterRefused(str(body.get("refused") or f"rc={p.returncode}"))
    if body.get("op") != req.get("op"):
        raise AdapterRefused("output_op_mismatch")
    return body

# ── durable receipt binding (the probe's pass evidence) ─────────────

_WAL_RECORDINGS = None

def _wal_recordings():
    """The existing receipt/blob-pool reader (ws1-wal-recordings.py) —
    reused, never a second blob-path convention."""
    global _WAL_RECORDINGS
    if _WAL_RECORDINGS is None:
        import importlib.util
        here = os.path.dirname(os.path.abspath(__file__))
        spec = importlib.util.spec_from_file_location(
            "ws1_wal_recordings", os.path.join(here, "ws1-wal-recordings.py"))
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        _WAL_RECORDINGS = mod
    return _WAL_RECORDINGS

def bound_receipts(evs, store_root, slot_id, run_id):
    """Every terminal model-call row whose receipt names exactly this
    slot under this phase run: `[(class, event payload, receipt)]`.
    A receipt that is unreadable, unparsable, unbound or bound to
    another slot/run is never attributed to `slot_id`."""
    rows = []
    for e in evs:
        cls = e.get("class")
        if cls not in ("model.call.completed", "model.call.failed"):
            continue
        p = e.get("payload") or {}
        ref = p.get("response_ref")
        if not ref or ":" not in ref:
            continue
        try:
            with open(_wal_recordings().blob_path(store_root, ref)) as f:
                rec = json.load(f)
        except Exception:
            continue
        px = (rec or {}).get("proxy") or {}
        if px.get("slot_id") == slot_id and px.get("phase_run_id") == run_id:
            rows.append((cls, p, rec))
    return rows

def probe_call_verdict(rows, store_root, model, effort, route):
    """`pass` only for a receipted, forwarded call whose downstream
    request is the planned probe call (model, effort, route). Anything
    else is a typed failure — HTTP status alone never passes."""
    if not rows:
        return "unreceipted{no_bound_receipt}", ""
    for cls, p, rec in rows:
        px = rec.get("proxy") or {}
        dn = px.get("downstream") or {}
        if cls != "model.call.completed" or p.get("disposition") != "forwarded":
            continue
        if dn.get("path") != route:
            continue
        try:
            with open(_wal_recordings().blob_path(
                    store_root, dn.get("body_ref") or "x:x")) as f:
                body = json.load(f)
        except Exception:
            continue
        if body.get("model") == model and body.get("reasoning_effort") == effort:
            return "pass", p.get("response_ref") or ""
    return "failed{receipt_mismatch}", rows[-1][1].get("response_ref") or ""

def run_lane_probe(a, sess, out_dir, http=None, events=None):
    """`hh-lane-probe/1` — one receipted call per pinned effort path on
    the probe slot token through the loopback listener, before the
    first HH-native slot (REQ-XH-124; holds.rs's committed shape). The
    pass verdict is bound to the durable receipt naming the probe slot
    and phase run — never inferred from an HTTP status."""
    http = http or http_call
    events = events or read_events
    probe_id = "w4-lane-probe"
    route = "/v1/chat/completions"
    store = sess.get("store_root") or a.store_root
    grant = (sess.get("grants") or {}).get(probe_id) or {}
    token = grant.get("slot_token")
    now = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    if not token:
        rec = {"schema": "hh-lane-probe/1", "stage": "lane-probe",
               "window": "W4", "gate": "FP-E",
               "disposition": "failed{no_slot_grant}",
               "evidence_label": "live-executed", "recorded_at": now}
        return rec, False
    body = {"model": a.model, "max_tokens": 8, "stream": False,
            "reasoning_effort": a.effort,
            "messages": [{"role": "user", "content":
                          "HH-native lane probe — answer with the single token ok."}]}
    status = http(a.listen, route, body, token)
    evs = events(store)
    rows = bound_receipts(evs, store, probe_id, a.run_id)
    disp, receipt = probe_call_verdict(rows, store, a.model, a.effort, route)
    if disp == "pass" and not (200 <= status < 300):
        disp = f"failed{{http:{status}}}"
    cost = slot_cost_micros(evs, probe_id)
    if cost > PROBE_SLICE_MICROS:
        disp = "failed{probe_cap_exceeded}"
    rec = {"schema": "hh-lane-probe/1", "stage": "lane-probe",
           "window": "W4", "gate": "FP-E",
           "slot": {"token_ref": f"slot:{probe_id}",
                    "slice_micros": PROBE_SLICE_MICROS},
           "plan": {"calls": 1, "cap_micros": PROBE_SLICE_MICROS},
           "calls": [{"probe_id": f"lane-probe-{a.effort}",
                      "surface": "openai.chat", "route": route,
                      "http_status": status, "disposition": disp,
                      "receipt_ref": receipt, "cost_micros": cost}],
           "precedes": "first_hh_native_slot",
           "disposition": "pass" if disp == "pass" else "failed",
           "evidence_label": "live-executed", "recorded_at": now}
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

class LaunchRefused(Exception):
    """A pre-launch prerequisite failed — nothing was started and no
    credential file was written; the slot is `withheld{infra}` with
    this cause (R6.8's closed set)."""

def write_secret_file(path, value):
    """Create an owned credential file atomically at 0600 — never a
    window where the token sits world-readable."""
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as f:
        f.write(value)

def launch_member(a, sess, m, slot_dir):
    """Spawn one slot member; returns (proc, logfile_handle, tok_file).
    Every prerequisite is validated before the token file exists;
    `LaunchRefused` means nothing started."""
    sid = m["slot_id"]
    arm = m["arm"]
    grant = (sess.get("grants") or {}).get(sid) or {}
    token = grant.get("slot_token")
    if not token:
        raise LaunchRefused("grant_absent")
    image = a.task_images.get(m["task"])
    if not image:
        raise LaunchRefused("image_absent")
    os.makedirs(slot_dir, exist_ok=True)
    tok_file = os.path.join(slot_dir, "slot-token")
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
        src_inst = os.path.join(a.tasks_dir, m["task"], "instruction.md")
        if not os.path.exists(src_inst):
            src_inst = os.path.join(a.tasks_dir, f"{m['task']}.md")
        if not os.path.exists(src_inst):
            raise LaunchRefused("instruction_absent")
        try:
            src = read_json(doc)
        except Exception:
            raise LaunchRefused("definition_unreadable")
        level = None
        if src.get("schema") == "hh-fp-sweep-level/1":
            # the V5 level record — the bound doc lives under
            # `sealed.document`; the slot dir keeps the level identity.
            if (src.get("level_id") != "V5" or
                    src.get("factor_id") !=
                    "transcript_render.assistant_replay.reasoning"):
                raise LaunchRefused("v5_level_mismatch")
            body = (src.get("sealed") or {}).get("document")
            level = {k: src.get(k) for k in
                     ("schema", "level_id", "level_name", "factor_id",
                      "level_ref", "baseline_ref", "override_spellings")}
        else:
            body = src
        stage = os.path.join(slot_dir, "ws")
        os.makedirs(stage, exist_ok=True)
        inst = os.path.join(slot_dir, "instruction.md")
        with open(src_inst) as f:
            with open(inst, "w") as g:
                g.write(f.read())
        if level is not None:
            with open(os.path.join(slot_dir, "level.json"), "w") as f:
                json.dump(level, f, indent=1, sort_keys=True)
        docf = os.path.join(slot_dir, "definition.json")
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
        tok_file = None
    if tok_file:
        write_secret_file(tok_file, token)
    logf = open(os.path.join(slot_dir, "driver.log"), "w")
    eprint(f"{sid}: launch ({arm})")
    try:
        proc = subprocess.Popen(cmd, stdout=logf, stderr=subprocess.STDOUT,
                                env=env)
    except Exception:
        logf.close()
        raise
    return proc, logf, tok_file

# ── the W4 admission state machine ──────────────────────────────────

def classify_first3(row):
    """A settled native-lane slot's hold observation: True = typed
    infrastructure failure (a receipted `infra` terminal on the slot),
    False = known non-infra (rc 0 with metered, receipted spend), None
    = unclassified — never read as False (holds further admission of
    that arm pending an honest typed decision)."""
    if row.get("infra") is True:
        return True
    if row.get("rc") == 0 and (row.get("cost_micros") or 0) > 0:
        return False
    return None

class Scheduler:
    """Drives the pinned schedule through admission, boundaries, holds
    and closure. Every rule decision is the canonical continuation
    module's (via `adapter`); this class only sequences them:

      * after round `checkpoint_round` and every later boundary: stop
        admitting, drain the prior round, fold its cost/infra fields,
        run the checkpoint, persist it, then apply every withholding
        and consume `continuing` as a whitelist;
      * each native-lane arm's first 3 **admitted** slots feed that
        arm's hold the moment each settles (completion order never
        replaces admission order; one arm's observation never holds
        the other);
      * unprotected arms pass declared-cap expected-cost admission;
      * O/H pairs admit or withhold together; the cut-off is rechecked
        after every capacity wait; a running slot past its declared
        bound is terminated and recorded attempted;
      * at closure the planned ids are the disjoint union of attempted
        and typed-withheld ids.

    Hooks (all injectable for offline controls): launch(m) → handle |
    raise LaunchRefused; poll(handle) → rc | None; terminate(handle) →
    rc; finish(m, handle, rc, started, end) → row; cost_of(sid) → µ$;
    orphans() → [model_call_id]; now(); sleep(s); persist(name, obj).
    """

    def __init__(self, rounds, cfg, adapter, *, launch, poll, terminate,
                 finish, cost_of, orphans, now, sleep, persist,
                 concurrency, checkpoint_round, slot_deadline_s,
                 harvest_reserve_s, dead_man_epoch, probe_passed,
                 probe_cost_micros=0):
        self.rounds = rounds
        self.cfg = cfg
        self.adapter = adapter
        self.launch, self.poll, self.terminate = launch, poll, terminate
        self.finish, self.cost_of, self.orphans = finish, cost_of, orphans
        self.now, self.sleep, self.persist = now, sleep, persist
        self.concurrency = concurrency
        self.checkpoint_round = checkpoint_round
        self.slot_deadline_s = slot_deadline_s
        self.harvest_reserve_s = harvest_reserve_s
        self.dead_man_epoch = dead_man_epoch
        self.probe_passed = probe_passed
        self.probe_cost_micros = probe_cost_micros
        self.planned = [m for units in rounds for u in units
                        for m in u["members"]]
        self.round_of = {m["slot_id"]: ri
                         for ri, units in enumerate(rounds, start=1)
                         for u in units for m in u["members"]}
        self.results = {}     # attempted: slot_id -> row
        self.withheld = {}    # slot_id -> withheld row
        self.running = {}     # slot_id -> (handle, member, started)
        self.arm_closed = {}  # arm -> withheld row template
        self.first3 = {a: [] for a in NATIVE_ARMS}
        self.first3_obs = {}
        self.holds = []
        self.checkpoints = []
        self.inference_refusals = []
        self.stopped = None
        self.whitelist = None
        self.protected = list(cfg["priority"]["protected"])

    # ── bookkeeping ────────────────────────────────────────────────
    def pending(self, m):
        sid = m["slot_id"]
        return (sid not in self.results and sid not in self.withheld
                and sid not in self.running)

    def withhold(self, m, disposition, cause, label=None):
        sid = m["slot_id"]
        if not self.pending(m):
            raise RuntimeError(f"{sid} already dispositioned")
        row = {"slot_id": sid, "arm": m["arm"], "task": m["task"],
               "replicate": m["replicate"], "disposition": disposition,
               "cause": cause}
        if label:
            row["label"] = label
        self.withheld[sid] = row

    def stop(self, disposition, cause):
        """Stop admitting everything; every unadmitted slot withholds."""
        if self.stopped is None:
            self.stopped = (disposition, cause)
        for m in self.planned:
            if self.pending(m):
                self.withhold(m, disposition, cause)

    def close_arm(self, arm, disposition, cause, label=None):
        if arm not in self.arm_closed:
            self.arm_closed[arm] = (disposition, cause, label)
        for m in self.planned:
            if m["arm"] == arm and self.pending(m):
                self.withhold(m, disposition, cause, label)

    def deadline_fits(self):
        return (self.now() + self.slot_deadline_s + self.harvest_reserve_s
                < self.dead_man_epoch)

    def call(self, req):
        req = dict(req, schema=CONT_ADAPTER_SCHEMA)
        return self.adapter(req)

    # ── settle + holds ─────────────────────────────────────────────
    def reap(self):
        changed = False
        for sid, (h, m, started) in list(self.running.items()):
            rc = self.poll(h)
            end = None
            if rc is None:
                if self.now() - started <= self.slot_deadline_s:
                    continue
                rc = self.terminate(h)
                end = "terminated{slot_deadline}"
            row = self.finish(m, h, rc, started, end)
            del self.running[sid]
            self.results[sid] = row
            if any(sid in ids for ids in self.first3.values()):
                self.first3_obs[sid] = classify_first3(row)
                changed = True
        if changed:
            self.evaluate_holds()

    def evaluate_holds(self):
        for arm in NATIVE_ARMS:
            if arm in self.arm_closed or arm not in [m["arm"] for m in self.planned]:
                continue
            settled = [s for s in self.first3[arm] if s in self.first3_obs]
            obs = [self.first3_obs[s] for s in settled]
            remaining = [m["slot_id"] for m in self.planned
                         if m["arm"] == arm and self.pending(m)]
            cause_bits = [f"{s}:{'infra' if o else 'unclassified'}"
                          for s, o in zip(settled, obs) if o is not False]
            if not self.probe_passed:
                cause_bits.insert(0, "lane_probe:failed")
            try:
                rep = self.call({"op": "native_lane_hold", "arm": arm,
                                 "probe_passed": self.probe_passed,
                                 "first3_infra": [o is not False for o in obs],
                                 "remaining": remaining,
                                 "cause": ",".join(cause_bits) or "none"})
            except AdapterRefused as e:
                self.stop("withheld{infra}", f"continuation_refused:{e}")
                return
            rec = rep.get("hold")
            if rec:
                self.holds.append(rec)
                self.close_arm(arm, rec["disposition"], rec["cause"],
                               rec.get("label"))

    def drain(self):
        while self.running:
            self.reap()
            if self.running:
                self.sleep(3.0)

    # ── continuation inputs (cost + infra fields only) ─────────────
    def continuation_input(self, after_round):
        arms = sorted({m["arm"] for m in self.planned})
        measured = {a: [] for a in arms}
        consumed_by_arm = {a: 0 for a in arms}
        for sid, row in sorted(self.results.items()):
            if "wall_ms" not in row:
                continue
            a = row["arm"]
            measured[a].append({"slot": sid,
                                "cost_micros": int(row.get("cost_micros") or 0),
                                "wall_ms": int(row["wall_ms"]),
                                "infra_failed": row.get("infra") is True})
            consumed_by_arm[a] += int(row.get("cost_micros") or 0)
        in_flight = {a: [] for a in arms}
        flight_total = 0
        for sid, (_, m, _) in sorted(self.running.items()):
            c = int(self.cost_of(sid))
            in_flight.setdefault(m["arm"], []).append({"slot": sid, "consumed_micros": c})
            flight_total += c
        remaining = {a: [m["slot_id"] for m in self.planned
                         if m["arm"] == a and self.pending(m)
                         and self.round_of[m["slot_id"]] > after_round]
                     for a in arms}
        return {
            "schema": CONT_INPUT_SCHEMA, "scope": "w4_checkpoint",
            "after_round": after_round, "measured": measured,
            "remaining": remaining, "consumed_by_arm": consumed_by_arm,
            "in_flight": in_flight,
            "consumed_micros": sum(consumed_by_arm.values()) + flight_total
                               + int(self.probe_cost_micros),
            "orphaned_hi_micros": len(self.orphans()) * self.cfg["slice_micros"],
            "root_cap_micros": self.cfg["root_cap_micros"],
            "c": self.cfg["c"], "slice_micros": self.cfg["slice_micros"],
            "priors_micros": self.cfg["priors_micros"],
            "client_timeouts_ms": self.cfg["client_timeouts_ms"],
            "priority": self.cfg["priority"]}

    def wall_input(self, after_round):
        pairs = []
        for units in self.rounds[after_round:]:
            for u in units:
                by_arm = {m["arm"]: m for m in u["members"]}
                o, h = by_arm.get("mini-swe-agent"), by_arm.get("hh_native")
                if o and h and self.pending(o) and self.pending(h):
                    pairs.append({"o_slot": o["slot_id"], "h_slot": h["slot_id"]})
        room = (self.dead_man_epoch - self.harvest_reserve_s - self.now()) * 1000
        return {"d_limit_ms": max(0, int(room)),
                "cutoff_ms": self.cfg["cutoff_ms"],
                "overhead_ms": self.cfg["overhead_ms"],
                "load_ms": self.cfg["load_ms"], "pairs": pairs}

    def boundary(self, after_round):
        """The round-boundary checkpoint: drain, project, persist, apply."""
        self.drain()
        req = {"op": "checkpoint",
               "input": self.continuation_input(after_round),
               "wall": self.wall_input(after_round)}
        try:
            out = self.call(req)["outcome"]
        except (AdapterRefused, KeyError, TypeError) as e:
            self.stop("withheld{infra}", f"continuation_refused:{e}")
            return
        ckpt = dict(out, label=f"w4-round-{after_round}",
                    recorded_at=time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                              time.gmtime()))
        # Durable before it affects scheduling.
        self.persist(f"checkpoint-r{after_round}.json", ckpt)
        self.checkpoints.append(ckpt)
        cause = f"checkpoint:{out.get('scope')}"
        for act in out.get("withheld") or []:
            for sid in act.get("slots") or []:
                m = next((x for x in self.planned if x["slot_id"] == sid), None)
                if m is None:
                    self.stop("withheld{infra}",
                              f"continuation_foreign_slot:{sid}")
                    return
                if self.pending(m):
                    self.withhold(m, act["disposition"], cause, act.get("label"))
            if act.get("arm"):
                self.arm_closed.setdefault(
                    act["arm"], (act["disposition"], cause, act.get("label")))
        refusals = out.get("inference_refusals") or []
        self.inference_refusals.extend(refusals)
        if refusals:
            # Protected-pair breach: every remaining slot is withheld by
            # the actions above; no later admission or boundary runs.
            self.stopped = ("withheld{budget}", f"{cause}:protected_pair_breach")
            return
        cont = out.get("continuing") or {}
        self.whitelist = {s for ids in cont.values() for s in ids}
        for m in self.planned:
            if (self.pending(m) and self.round_of[m["slot_id"]] > after_round
                    and m["slot_id"] not in self.whitelist):
                # neither withheld nor continuing — an incoherent reply
                self.stop("withheld{infra}",
                          f"continuation_incoherent:{m['slot_id']}")
                return

    # ── admission ──────────────────────────────────────────────────
    def cap_admit(self, members):
        out = []
        for m in members:
            if m["arm"] in self.protected:
                out.append(m)
                continue
            try:
                rep = self.call({"op": "admit",
                                 "input": self.continuation_input(0),
                                 "arm": m["arm"],
                                 "caps_micros": self.cfg["caps_micros"]})
            except AdapterRefused as e:
                self.stop("withheld{infra}", f"continuation_refused:{e}")
                return []
            adm = rep.get("admission") or {}
            if adm.get("verdict") == "admit":
                out.append(m)
            elif adm.get("verdict") == "capped":
                self.close_arm(m["arm"], adm["disposition"],
                               f"cap:expected {adm['expected_micros']} > "
                               f"cap {adm['cap_micros']}", adm.get("label"))
            else:
                self.stop("withheld{infra}",
                          f"continuation_incoherent:admit:{adm.get('verdict')}")
                return []
        return out

    def live(self, members):
        """Pending members still admissible, withholding the rest."""
        out = []
        for m in members:
            if not self.pending(m):
                continue
            if self.stopped:
                self.withhold(m, *self.stopped)
            elif m["arm"] in self.arm_closed:
                self.withhold(m, *self.arm_closed[m["arm"]])
            else:
                out.append(m)
        return out

    def launch_unit(self, members):
        started = []
        for i, m in enumerate(members):
            sid = m["slot_id"]
            try:
                h = self.launch(m)
            except LaunchRefused as e:
                self.withhold(m, "withheld{infra}", f"launch_refused:{e}")
                self.abandon_unit(started, m, members[i + 1:])
                return
            except Exception as e:
                self.results[sid] = {"slot_id": sid, "arm": m["arm"],
                                     "task": m["task"],
                                     "replicate": m["replicate"],
                                     "launch": f"failed{{{type(e).__name__}}}"}
                self.abandon_unit(started, m, members[i + 1:])
                return
            self.running[sid] = (h, m, self.now())
            started.append(m)
            if m["arm"] in self.first3 and len(self.first3[m["arm"]]) < 3:
                self.first3[m["arm"]].append(sid)

    def abandon_unit(self, started, failed, unlaunched):
        """A unit member failed to launch: terminate and record its
        already-started partners (attempted, never retro-withheld); the
        members never started are `withheld{infra}` naming the failure."""
        for m in unlaunched:
            self.withhold(m, "withheld{infra}",
                          f"unit_launch_failed:{failed['slot_id']}")
        for m in started:
            h, _, st = self.running.pop(m["slot_id"])
            rc = self.terminate(h)
            self.results[m["slot_id"]] = self.finish(
                m, h, rc, st, f"terminated{{unit_launch_rollback:{failed['slot_id']}}}")

    def run(self):
        # A failed probe holds the native-lane arms before any admission.
        self.evaluate_holds()
        for ri, units in enumerate(self.rounds, start=1):
            if (ri > self.checkpoint_round and not self.stopped
                    and any(self.pending(m) for u in units for m in u["members"])):
                self.boundary(ri - 1)
            for u in units:
                members = self.live(u["members"])
                if not members:
                    continue
                if not self.deadline_fits():
                    for m in members:
                        self.withhold(m, "withheld{window_deadline}",
                                      "admission_cutoff")
                    continue
                members = self.cap_admit(members)
                members = self.live(members)
                if not members:
                    continue
                while len(self.running) + len(members) > self.concurrency:
                    self.reap()
                    if len(self.running) + len(members) > self.concurrency:
                        self.sleep(2.0)
                # Holds/caps may have changed and the wait consumed the
                # same admission window — decide again before starting.
                members = self.live(members)
                if not members:
                    continue
                if not self.deadline_fits():
                    for m in members:
                        self.withhold(m, "withheld{window_deadline}",
                                      "admission_cutoff_after_wait")
                    continue
                self.launch_unit(members)
                self.reap()
        self.drain()
        return self.closure()

    def closure(self):
        planned = [m["slot_id"] for m in self.planned]
        attempted, withheld = set(self.results), set(self.withheld)
        dup_planned = sorted({s for s in planned if planned.count(s) > 1})
        both = sorted(attempted & withheld)
        missing = sorted(set(planned) - attempted - withheld)
        foreign = sorted((attempted | withheld) - set(planned))
        return {"planned": len(planned), "attempted": len(attempted),
                "withheld": len(withheld), "duplicate_planned": dup_planned,
                "attempted_and_withheld": both, "missing": missing,
                "foreign": foreign,
                "ok": not (dup_planned or both or missing or foreign)}

def slot_infra(evs, store_root, slot_id, run_id):
    """True iff a receipted terminal row bound to this slot carries the
    proxy's typed `infra` member (REQ-XC-115); never inferred from rc."""
    return any(p.get("infra") is True
               for _, p, _ in bound_receipts(evs, store_root, slot_id, run_id))

def verify_session_end(sess_file, grants, shim_rc):
    """Clean teardown needs the shim's own acknowledgement: exit 0 and a
    `.end` record whose acknowledged revocations cover every grant with
    no recorded seal/revoke/stop failure. Anything else is
    `incomplete{…}` — never claimed clean."""
    why = []
    if shim_rc != 0:
        why.append(f"shim_rc:{shim_rc}")
    try:
        end = read_json(sess_file + ".end")
    except Exception:
        return {"status": "incomplete{session_end_absent}", "detail": why}
    revoked = set(end.get("revoked") or [])
    missing = sorted(set(grants) - revoked)
    if missing:
        why.append(f"unrevoked:{len(missing)}")
    for k in ("revoke_failed", "seal_failed"):
        if end.get(k):
            why.append(f"{k}:{len(end[k])}")
    if end.get("stop") not in (None, "ok"):
        why.append(f"stop:{end.get('stop')}")
    if "stop" not in end:
        why.append("stop:unacknowledged")
    return {"status": "clean" if not why else "incomplete{session_end}",
            "detail": why, "unrevoked": missing}

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cfg", required=True)
    ap.add_argument("--w1-bin", required=True)
    ap.add_argument("--l1-drive-bin", required=True)
    ap.add_argument("--fp-continuation-bin", required=True)
    ap.add_argument("--continuation-config", required=True)
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
    ap.add_argument("--shim-close-timeout-s", type=int, default=120)
    ap.add_argument("--out", required=True)
    ap.add_argument("--plan-only", action="store_true")
    a = ap.parse_args()

    tasks = a.tasks.split(",")
    rounds = plan_schedule(tasks, a.replicates, ARMS, a.seed)
    if a.plan_only:
        print(json.dumps({"rounds": [
            [{"task": u["task"], "replicate": u["replicate"],
              "order_key": u["order_key"],
              "members": [m["slot_id"] for m in u["members"]]}
             for u in units] for units in rounds]}, indent=1))
        return 0

    # ── pinned inputs validated before any credential is minted ──────
    try:
        ccfg = load_continuation_config(a.continuation_config)
    except ContinuationConfigError as e:
        eprint(f"continuation config refused: {e}")
        return 2
    if ccfg["slice_micros"] != a.slot_slice_micros or ccfg["c"] != a.concurrency:
        eprint("continuation config refused: slice/C differ from the declared session")
        return 2
    if not os.access(a.fp_continuation_bin, os.X_OK):
        eprint("fp-continuation binary is not executable")
        return 2
    cfg = read_json(a.cfg)
    a.tasks_dir = cfg["tasks_dir"]
    a.task_images = read_json(cfg["images_file"])
    a.model_upstream = int(a.listen.rsplit(":", 1)[1])
    with open(a.continuation_config, "rb") as f:
        ccfg_sha = sha256_hex(f.read())

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
    shim_log = open(os.path.join(a.out, "proxy-session.log"), "w")
    shim = subprocess.Popen(ps_cmd, stdout=shim_log,
                            stderr=subprocess.STDOUT, text=True)
    window = {"schema": "hh-fp-window/1", "window": "W4", "gate": "FP-E",
              "run_id": a.run_id, "seed": a.seed, "arms": ARMS,
              "started_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
              "dead_man_epoch": a.dead_man_epoch,
              "concurrency": a.concurrency,
              "continuation": {"config_sha256": ccfg_sha,
                               "source_refs": ccfg["source_refs"],
                               "priority": ccfg["priority"],
                               "rule": "crates/hh-runner continuation (fp-continuation)"},
              "schedule_rows": [
                  {"round": ri, "task": u["task"],
                   "replicate": u["replicate"], "order_key": u["order_key"],
                   "members": [m["slot_id"] for m in u["members"]]}
                  for ri, units in enumerate(rounds, start=1)
                  for u in units],
              "withheld": [], "checkpoints": [], "checkpoint": None,
              "lane_probe": None, "not_run": {
                  "pi": "not_run{admission:AG3}",
                  "claude-code": "not_run{admission:AG3}"}}
    sched = None
    rc_out = 0
    owned = {}   # slot_id -> Popen (owned children only)
    try:
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
            eprint("proxy session never provisioned its slot grants")
            window["teardown"] = {"status": "incomplete{no_session}"}
            return 2
        store = sess.get("store_root") or a.store_root

        probe_rec, probe_ok = run_lane_probe(a, sess, a.out)
        window["lane_probe"] = probe_rec

        def launch(m):
            p, lf, tok = launch_member(a, sess, m,
                                       os.path.join(slots_dir, m["slot_id"]))
            owned[m["slot_id"]] = p
            return (p, lf, tok)

        def poll(h):
            return h[0].poll()

        def terminate(h):
            p = h[0]
            p.terminate()
            try:
                return p.wait(timeout=30)
            except subprocess.TimeoutExpired:
                p.kill()
                return p.wait(timeout=30)

        def finish(m, h, rc, started, end):
            p, lf, tok = h
            try:
                lf.close()
            except Exception:
                pass
            sid = m["slot_id"]
            sd = os.path.join(slots_dir, sid)
            row = {"slot_id": sid, "arm": m["arm"], "task": m["task"],
                   "replicate": m["replicate"], "rc": rc,
                   "wall_ms": int((time.time() - started) * 1000)}
            if end:
                row["end"] = end
            if m["arm"] in NATIVE_ARMS and not end:
                row["verifier"] = run_verifier(sd)
            evs = read_events(store)
            row["cost_micros"] = slot_cost_micros(evs, sid)
            row["infra"] = slot_infra(evs, store, sid, a.run_id)
            if tok and os.path.exists(tok):
                os.remove(tok)
            with open(os.path.join(sd, "slot-result.json"), "w") as f:
                json.dump(row, f, indent=1, sort_keys=True)
            owned.pop(sid, None)
            return row

        def persist(name, obj):
            path = os.path.join(a.out, name)
            with open(path + ".tmp", "w") as f:
                json.dump(obj, f, indent=1, sort_keys=True)
                f.flush()
                os.fsync(f.fileno())
            os.replace(path + ".tmp", path)

        sched = Scheduler(
            rounds, ccfg,
            lambda req: call_adapter(a.fp_continuation_bin, req),
            launch=launch, poll=poll, terminate=terminate, finish=finish,
            cost_of=lambda sid: slot_cost_micros(read_events(store), sid),
            orphans=lambda: orphaned_call_ids(read_events(store)),
            now=time.time, sleep=time.sleep, persist=persist,
            concurrency=a.concurrency, checkpoint_round=a.checkpoint_round,
            slot_deadline_s=a.slot_deadline_s,
            harvest_reserve_s=a.harvest_reserve_s,
            dead_man_epoch=a.dead_man_epoch, probe_passed=probe_ok,
            probe_cost_micros=probe_rec.get("calls", [{}])[0].get("cost_micros", 0)
            if probe_rec.get("calls") else 0)
        window["closure"] = sched.run()
        if not window["closure"]["ok"]:
            rc_out = 3
    except Exception as e:
        eprint(f"driver failure: {type(e).__name__}: {e}")
        window["driver_failure"] = f"{type(e).__name__}"
        rc_out = 4
    finally:
        # Only recorded, owned children — never a broad process sweep.
        for sid, p in list(owned.items()):
            if p.poll() is None:
                p.terminate()
                try:
                    p.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    p.kill()
                window.setdefault("terminated_at_close", []).append(sid)
        if sched is not None:
            window["slot_results"] = dict(sched.results)
            window["withheld"] = [sched.withheld[k] for k in sorted(sched.withheld)]
            window["checkpoints"] = sched.checkpoints
            window["checkpoint"] = sched.checkpoints[0] if sched.checkpoints else None
            window["holds"] = sched.holds
            window["inference_refusals"] = sched.inference_refusals
            window["native_lane_hold"] = {
                arm: any(h.get("arm") == arm for h in sched.holds)
                for arm in NATIVE_ARMS}
        window["ended_at"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        # Close the session honestly: the sentinel tells the shim to seal
        # recordings, revoke the grants and stop the proxy; its own
        # acknowledgement decides whether teardown was clean.
        try:
            with open(sentinel, "w") as f:
                f.write("done")
            shim_rc = shim.wait(timeout=a.shim_close_timeout_s)
        except subprocess.TimeoutExpired:
            shim.terminate()
            try:
                shim.wait(timeout=30)
            except subprocess.TimeoutExpired:
                shim.kill()
            shim_rc = "timeout"
        grants = sorted(((read_json(sess_file) if os.path.exists(sess_file)
                          else {}).get("grants") or {}))
        if "teardown" not in window:
            window["teardown"] = verify_session_end(sess_file, grants, shim_rc)
        if window["teardown"]["status"] != "clean" and rc_out == 0:
            rc_out = 5
        shim_log.close()
        with open(os.path.join(a.out, "w4-window.json"), "w") as f:
            json.dump(window, f, indent=1, sort_keys=True)
    print(json.dumps({"slots": len(window.get("schedule_rows", [])) and
                      sum(len(r["members"]) for r in window["schedule_rows"]),
                      "attempted": len(window.get("slot_results") or {}),
                      "withheld": len(window.get("withheld") or []),
                      "closure_ok": (window.get("closure") or {}).get("ok"),
                      "teardown": window["teardown"]["status"],
                      "window": os.path.join(a.out, "w4-window.json")}))
    return rc_out

if __name__ == "__main__":
    sys.exit(main())
