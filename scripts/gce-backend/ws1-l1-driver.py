#!/usr/bin/env python3
"""ws1-l1-driver.py — the Ws1 L1 legs on-VM driver (R6.45q wiring).

Drives spec §7.4's L1.0/L1.1/L1.2 over the gate-A partition's
trajectories against the exact frozen H-set (set digest c451ed97…).

Leg wiring (committed surfaces only):

- L1.0 — O twice against REPLAY(S) in fresh cells: two `hh-w1
  arm-run` cells of the original mini-swe-agent arm, model endpoint a
  replay-mode ws1-proxy-session serving the trajectory's recording
  (lenient — O diverges naturally; the receipts carry the verdicts).
  L1.0's O runs are live-exec by spec §7.4 — only H's L1.1 leg replays
  observations. Receipts → the O drive's request sequence; the copied
  trajectory → parsed/executed actions + termination.
- L1.1 — `hh_l1_drive --mode obs_replay`: the frozen HIR opened in
  EmbedService, its exec surfaces rebound to the shared
  ObsReplayExecutor fed `hh-w1 l1-raws` output, model_port bound to a
  STRICT replay listener. First divergence fails.
- L1.2 — `hh_l1_drive --mode live`: same drive, live exec surfaces,
  lenient replay listener; the fold gates only requests ≤ s(S).

The comparison fold is `hh-w1 l1-fold` over the emitted L1Drive docs —
`hh-l1-diff/1` per gated divergence + `hh-l1-conformance/1` bound to
the exact H. Every extraction failure emits a typed row; nothing
fabricates a member or a conformance verdict.

usage:
  ws1-l1-driver.py --cfg cfg.json --corpus-dir DIR --gate-a-list F \
    --h-set-dir DIR --layers-file F --admission mini.json \
    --proxy-bin BIN --w1-bin hh-w1 --l1-drive-bin hh_l1_drive \
    --lane-file lane.json --authorization-file auth.json \
    --canon-id hh-l1-canon/<v> --out-dir DIR --run-root DIR
"""
import argparse, base64, hashlib, json, os, subprocess, sys, time

def eprint(*a): print(*a, file=sys.stderr, flush=True)

def read_json(p):
    with open(p) as f:
        return json.load(f)

def run(cmd, env=None, timeout=3600):
    p = subprocess.run(cmd, capture_output=True, text=True,
                       env=env, timeout=timeout)
    return p.returncode, p.stdout, p.stderr

def blob_bytes(store_root, ref):
    """Resolve a proxy blob ref to bytes. Refs spell as
    `blob:sha256:<hex>` or plain paths; walk the store when needed."""
    if not ref:
        return None
    cand = []
    if ":" in ref:
        # `blob:sha256:<hex>` or bare `sha256:<hex>` — the blob pool
        # files are named by the bare digest
        hexd = ref.rsplit(":", 1)[-1]
        direct = os.path.join(store_root, "blobs", hexd)
        if os.path.exists(direct):
            cand.append(direct)
        for dp, _, fns in os.walk(store_root):
            for fn in fns:
                if hexd in fn:
                    cand.append(os.path.join(dp, fn))
    elif os.path.exists(ref):
        cand.append(ref)
    else:
        for dp, _, fns in os.walk(store_root):
            for fn in fns:
                if ref in fn or ref in os.path.join(dp, fn):
                    cand.append(os.path.join(dp, fn))
    for c in cand:
        try:
            with open(c, "rb") as f:
                return f.read()
        except OSError:
            continue
    return None

def slot_receipts(store_root, slot_id):
    """The slot's hh.model_io/1 receipts, ordered by slot_call_seq.
    Receipts are durable wal refs, not standalone .json files — walk
    `runs/*/events.wal`, resolve each model.call terminal event's
    `response_ref` against the blob pool, and keep receipts bound to
    this slot. A re-driven leg leaves earlier attempt runs beside the
    current one — bind to the latest attempt's WAL (newest mtime), the
    session that served the drive being folded."""
    runs_d = os.path.join(store_root, "runs")
    wals = []
    if os.path.isdir(runs_d):
        for dp, _, fns in os.walk(runs_d):
            for fn in fns:
                if fn == "events.wal":
                    wals.append(os.path.join(dp, fn))
    wals.sort(key=lambda w: os.path.getmtime(w))
    wals = wals[-1:]
    rows = []
    for wal in wals:
        try:
            with open(wal) as f:
                for line in f:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        ev = json.loads(line)
                    except Exception:
                        continue
                    ev = ev.get("v") or ev   # wal wraps: {"k":"e","v":{…}}
                    cls = ev.get("class") or ""
                    if "model.call" not in cls:
                        continue
                    ref = (ev.get("payload") or {}).get("response_ref")
                    if not ref:
                        continue
                    raw = blob_bytes(store_root, ref)
                    if not raw:
                        continue
                    try:
                        d = json.loads(raw)
                    except Exception:
                        continue
                    px = d.get("proxy") or {}
                    if d.get("schema") != "hh.model_io/1" or not px:
                        continue
                    if px.get("slot_id") != slot_id:
                        continue
                    rows.append((px.get("slot_call_seq") or 0, d))
        except OSError:
            continue
    rows.sort(key=lambda t: t[0])
    return [d for _, d in rows]

def drive_from_receipts(store_root, slot_id, turn0=1):
    """Assemble an L1Drive `requests` list + verdicts from the slot's
    replay receipts (capture point 2 upstream-bound bodies)."""
    reqs, verdicts = [], []
    for d in slot_receipts(store_root, slot_id):
        px = d["proxy"]
        dn = px.get("downstream") or {}
        up = px.get("upstream") or {}
        body = blob_bytes(store_root, dn.get("body_ref"))
        ubody = blob_bytes(store_root, up.get("request_body_ref"))
        if ubody is None:
            # refused/undispatched calls carry no upstream capture —
            # in replay the forwarded body is the downstream body
            ubody = body
        ct = None
        for k, v in (dn.get("headers_kept") or {}).items():
            if k.lower() == "content-type":
                ct = v
        reqs.append({
            "turn": turn0 + len(reqs),
            "method": dn.get("method") or "",
            "path": dn.get("path") or "",
            "content_type": ct,
            "body_b64": base64.b64encode(body or b"").decode(),
            "upstream_body_b64": base64.b64encode(ubody or b"").decode(),
        })
        verdicts.append(px.get("replay") or {})
    return reqs, verdicts

def endpoint_first(verdicts):
    """The strict/lenient endpoint's first divergence as [turn,path].
    `exact`/`canonical_equal` are the match verdicts the proxy mints —
    everything else is a refusal or divergence."""
    for i, v in enumerate(verdicts):
        verdict = v.get("verdict")
        if verdict and verdict not in ("match", "matched", "ok", "pass",
                                       "exact", "canonical_equal"):
            return [i + 1, v.get("first_diff_path") or verdict]
    return None

def extra_request(verdicts, recorded_len):
    """A served request beyond the recorded set (exhaustion)."""
    for i, v in enumerate(verdicts):
        verdict = str(v.get("verdict") or "")
        if i + 1 > recorded_len or "exhaust" in verdict or \
                verdict.startswith("refused"):
            return True
    return False

SUBMIT_MARKER = "COMPLETE_TASK_AND_SUBMIT_FINAL_OUTPUT"

def response_commands(resp_body):
    """tool_call commands in a recorded response body {utf8: json}."""
    out = []
    raw = (resp_body or {}).get("utf8")
    if not raw:
        return out
    try:
        doc = json.loads(raw)
    except Exception:
        return out
    for ch in doc.get("choices") or []:
        msg = ch.get("message") or {}
        for tc in msg.get("tool_calls") or []:
            fn = tc.get("function") or {}
            args = fn.get("arguments")
            if isinstance(args, str):
                try:
                    args = json.loads(args)
                except Exception:
                    args = {}
            out.append({"command": (args or {}).get("command") or "",
                        "tool_call_id": tc.get("id") or ""})
    return out

def echo_literal(cmd):
    """`echo <bare literal>` → the literal (ADR-0450); None otherwise."""
    c = (cmd or "").strip()
    if not c.startswith("echo "):
        return None
    rest = c[5:]
    if not rest or rest.startswith("-") or \
            any(ch in rest for ch in '|><;&$`"\'\\'):
        return None
    return rest

def terminal_submission(recording):
    """True when the recording's last call proposes the declared
    submit-marker echo (ADR-0450 — the marker is the terminal)."""
    calls = recording.get("calls") or []
    if not calls:
        return False
    resp = (calls[-1].get("released") or {}).get("body") or \
        (calls[-1].get("upstream_response") or {}).get("body")
    return any(echo_literal(a["command"]) == SUBMIT_MARKER
               for a in response_commands(resp))

def terminal_echo_raw(recording, raws):
    """ADR-0450 item 3 — the deterministic `echo` output the corpus
    never recorded (the marker convention intercepts it), typed
    `reconstructed`. None when absent or already served."""
    calls = recording.get("calls") or []
    if not calls:
        return None
    resp = (calls[-1].get("released") or {}).get("body") or \
        (calls[-1].get("upstream_response") or {}).get("body")
    for a in response_commands(resp):
        lit = echo_literal(a["command"])
        if lit is None:
            continue
        want = lit + "\n"
        if raws and (raws[-1] or {}).get("output") == want:
            return None
        return {"returncode": 0, "output": want,
                "exception_info": None,
                "reconstructed": "echo_literal"}
    return None

def channel_submission(payload):
    """The channel-typed submission member (ADR-0450 item 2): the
    `marker` sentinel for marker-convention submissions (the minted
    `submission_ref` is an opaque id — deliberately not compared), the
    payload string itself for content submissions."""
    if payload is None:
        return None
    if isinstance(payload, str) and payload:
        return payload
    return "marker"

def o_rec_drive(recording):
    """The recording-derived L1.1 reference drive (ADR-0450): the
    corpus the strict proxy enforced — the requests O actually sent,
    and the recorded terminal class (`submitted` on a marker tail,
    `corpus_boundary` on a crash-truncated recording)."""
    reqs = []
    for i, c in enumerate(recording.get("calls") or []):
        dn = c.get("downstream") or {}
        upr = c.get("upstream_request") or {}
        upb = (upr.get("body") or dn.get("body") or {})
        reqs.append({
            "turn": i + 1,
            "method": dn.get("method") or "",
            "path": dn.get("path") or "",
            # request content_type is not retained by the recording —
            # the corpus-uniform receipt value is adopted (ADR-0450).
            "content_type": "application/json",
            "body_b64": base64.b64encode(
                ((dn.get("body") or {}).get("utf8") or "").encode()
            ).decode(),
            "upstream_body_b64": base64.b64encode(
                (upb.get("utf8") or "").encode()
            ).decode(),
        })
    submitted = terminal_submission(recording)
    return {
        "requests": reqs,
        # the per-turn action channel is uninstrumented on both sides
        # today — request-level byte equality carries command fidelity.
        "parsed_actions": [],
        "executed_actions": [],
        "termination": "submitted" if submitted else
                       "exception{corpus_boundary}",
        "submission": channel_submission("" if submitted else None),
        "extra_request": False,
        "refusal": None,
    }

def o_drive(store_root, slot_id, traj, recorded_len):
    reqs, verdicts = drive_from_receipts(store_root, slot_id)
    acts = []
    for step in (traj.get("steps") or traj.get("actions") or []):
        cmd = step.get("command") or step.get("action") or ""
        acts.append([{"command": cmd, "tool_call_id": ""}])
    # mini-swe-agent's trajectory nests the terminal under `info` —
    # `exit_status: Submitted` + the (possibly empty) `submission`.
    info = traj.get("info") or {}
    submitted = traj.get("submitted")
    if submitted is None:
        submitted = traj.get("submission")
    if submitted is None:
        submitted = info.get("submission")
    exit_status = traj.get("exit_status") or info.get("exit_status") or ""
    if submitted is None and str(exit_status).lower() == "submitted":
        submitted = ""
    term = "submitted" if submitted is not None else \
        ("exception{%s}" % (traj.get("termination") or "unspecified"))
    return {
        "requests": reqs,
        "parsed_actions": acts,
        "executed_actions": acts,
        "termination": term,
        "submission": channel_submission(submitted),
        "extra_request": extra_request(verdicts, recorded_len),
        "refusal": None,
    }

def h_drive(store_root, slot_id, events, recorded_len):
    reqs, verdicts = drive_from_receipts(store_root, slot_id)
    parsed, executed = [], []
    term, submission, refusal = "exception{unfinished}", None, None
    stop_reason = None
    for ev in events:
        cls = ev.get("class") or ""
        pl = ev.get("payload") or {}
        if "action" in cls and "parsed" in cls:
            parsed.append([{"command": c.get("command") or "",
                            "tool_call_id": c.get("tool_call_id") or ""}
                           for c in (pl.get("calls") or [])])
        elif "action" in cls and ("dispatch" in cls or "exec" in cls):
            executed.append([{"command": c.get("command") or "",
                              "tool_call_id": c.get("tool_call_id") or ""}
                             for c in (pl.get("calls") or [])])
        elif cls == "control.decision" and pl.get("submission_ref"):
            # a recorded `submission_ref` on the decision is the
            # terminal submission (ADR-0450 — channel-typed).
            term = "submitted"
            submission = "marker"
        elif cls in ("lifecycle.turn.finished",
                     "lifecycle.run.finished"):
            stop_reason = pl.get("stop_reason") or {}
        elif "refus" in cls or "error" in cls:
            refusal = {"rule": cls, "detail": json.dumps(pl)[:200]}
    if term != "submitted" and stop_reason:
        ec = stop_reason.get("error_class") or {}
        fam = ec.get("family") or stop_reason.get("kind") or "unspecified"
        term = "exception{%s}" % fam
    return {
        "requests": reqs,
        "parsed_actions": parsed,
        "executed_actions": executed,
        "termination": term,
        "submission": submission,
        "extra_request": extra_request(verdicts, recorded_len),
        "refusal": refusal,
    }, endpoint_first(verdicts)

def start_replay(a, slot_id, recording, strict):
    """Open a replay-mode session for one leg; returns (proc-free
    sentinel path, session_out, listen)."""
    leg_dir = os.path.join(a.run_root, slot_id)
    os.makedirs(leg_dir, exist_ok=True)
    rec_decl = dict(recording)
    rec_decl["slot_id"] = slot_id   # bind the leg's slot to this
                                    # trajectory's recorded sequence
    decl = {"recordings": [rec_decl], "canonicalizer": a.canon_id,
            "strict": strict, "keying": "sequence"}
    decl_p = os.path.join(leg_dir, "replay-decl.json")
    with open(decl_p, "w") as f:
        json.dump(decl, f)
    sess_out = os.path.join(leg_dir, "session.json")
    sentinel = os.path.join(leg_dir, "done")
    listen = f"127.0.0.1:{a.base_port}"
    a.base_port += 1
    # stale session/sentinel files from a prior drive of this leg
    # would satisfy the readiness poll before the new proxy binds
    for stale in (sess_out, sentinel):
        try:
            os.unlink(stale)
        except OSError:
            pass
    shim_log = os.path.join(leg_dir, "shim-stderr.txt")
    cmd = [a.session_shim, "--proxy-bin", a.proxy_bin,
           "--store-root", os.path.join(leg_dir, "store"),
           # unique run-id per leg attempt — a prior attempt's dead
           # writer lease would block `resume` on the same run dir
           "--run-id", f"{slot_id}-a{int(time.time() % 100000)}",
           "--listen", listen,
           "--lane-file", a.lane_file,
           "--authorization-file", a.authorization_file,
           "--slots", f"{slot_id}:0:high",
           "--mode", "replay", "--replay-file", decl_p,
           "--session-out", sess_out, "--sentinel", sentinel,
           "--poll-seconds", "2"]
    with open(shim_log, "w") as lf:
        proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL,
                                stderr=lf)
    # wait for the session doc (grants live) — bounded; a fresh
    # store init can take a minute+
    for _ in range(900):
        if os.path.exists(sess_out):
            break
        if proc.poll() is not None:
            raise RuntimeError(f"replay session {slot_id} exited")
        time.sleep(0.2)
    # and until the listener actually accepts — the doc lands before
    # the bind on some builds
    import socket
    port = int(listen.rsplit(":", 1)[-1])
    for _ in range(300):
        if proc.poll() is not None:
            raise RuntimeError(f"replay session {slot_id} exited")
        try:
            with socket.create_connection(("127.0.0.1", port), 0.5):
                break
        except OSError:
            time.sleep(0.2)
    return proc, sentinel, sess_out, listen

def close_replay(proc, sentinel):
    try:
        open(sentinel, "w").write("done")
    except OSError:
        pass
    try:
        proc.wait(timeout=60)
    except Exception:
        proc.terminate()

def recorded_len(rec):
    return len(rec.get("calls") or [])

def recorded_model(rec):
    """The model id the recorded downstream bodies carried — the
    drive's provider_model_id binds it (replay keys on sequence; the
    name is identity evidence, not a lookup)."""
    for c in rec.get("calls") or []:
        body = ((c.get("downstream") or {}).get("body") or {})
        raw = body.get("utf8")
        if raw:
            try:
                m = (json.loads(raw).get("model") or "").strip()
                if m:
                    return m
            except Exception:
                pass
    return "hh-replay"

# The window-authored HIR's slot refs are placeholders the first live
# open surfaces: `version_id:"latest"` (a pinned id that can never
# resolve — the selector spelling is `version_selector`), the
# `hh/react_minimal` underscore typo (`variant_for` binds the impl as
# `hh/react-minimal`), and `hh/openai-chat`, a name never published —
# the registered model_port variant is `hh/gateway_http` (the lane
# decodes dialect/endpoint from the slot's params). The bound doc also
# fills the endpoint/provider_model_id/credential_ref members the lane
# decode requires (`env:` indirection — values resolve at arm from the
# drive's exported env, never literals). Every substitution is
# recorded in `_bound.json` next to the derived doc; the frozen `hir`
# member itself is never touched.
_VARIANT_RENAMES = {"hh/react_minimal": "hh/react-minimal",
                    "hh/openai-chat": "hh/gateway_http"}
_MODEL_PORT_PARAMS = {
    "endpoint": "env:HH_L1_ENDPOINT",
    "provider_model_id": "env:HH_L1_MODEL",
    "credential_ref": "env:OPENAI_API_KEY",
}

def _bind_slot_variants(slots, subs):
    for name, binding in (slots or {}).items():
        arms = []
        if isinstance(binding, dict):
            arms = [binding["one"]] if "one" in binding else \
                (binding.get("many") or [])
        for arm in arms:
            var = arm.get("variant")
            if not isinstance(var, dict):
                continue
            if var.get("version_id") == "latest":
                del var["version_id"]
                var["version_selector"] = "latest"
                subs.append(f"{name}: version_id:latest -> "
                            f"version_selector:latest")
            vid = var.get("variant_id")
            if vid in _VARIANT_RENAMES:
                var["variant_id"] = _VARIANT_RENAMES[vid]
                subs.append(f"{name}: {vid} -> {var['variant_id']}")
            if name == "model_port":
                params = arm.setdefault("params", {})
                for k, v in _MODEL_PORT_PARAMS.items():
                    if k not in params:
                        params[k] = v
                        subs.append(f"model_port.params.{k} = {v}")
                tr = params.get("transcript_render")
                if isinstance(tr, dict):
                    for tk, tv in list(tr.items()):
                        if tv == "keep":
                            tr[tk] = "verbatim"
                            subs.append("model_port.params."
                                        "transcript_render.%s: keep "
                                        "-> verbatim" % tk)
                        elif isinstance(tv, dict):
                            for tk2, tv2 in list(tv.items()):
                                if tv2 == "keep":
                                    tv[tk2] = "verbatim"
                                    subs.append("model_port.params."
                                                "transcript_render.%s."
                                                "%s: keep -> verbatim"
                                                % (tk, tk2))
                    # window-authored member typos → admitted
                    # spellings (gateway_model::transcript_render):
                    # on_format_error/omit_assistant →
                    # format_error_turn/omit; text_only_turns →
                    # text_only_turn; tool_args → tool_arguments.
                    if "on_format_error" in tr:
                        v = tr.pop("on_format_error")
                        nv = "omit" if v == "omit_assistant" else v
                        tr["format_error_turn"] = nv
                        subs.append("model_port.params."
                                    "transcript_render.on_format_error:"
                                    "%s -> format_error_turn:%s"
                                    % (v, nv))
                    if "text_only_turns" in tr:
                        tr["text_only_turn"] = \
                            tr.pop("text_only_turns")
                        subs.append("model_port.params."
                                    "transcript_render."
                                    "text_only_turns -> "
                                    "text_only_turn")
                    if "tool_args" in tr:
                        tr["tool_arguments"] = tr.pop("tool_args")
                        subs.append("model_port.params."
                                    "transcript_render.tool_args -> "
                                    "tool_arguments")

def bind_hir(src_p, dst_p):
    """Derive the session-openable doc from the frozen `hir` member —
    the recorded substitutions above applied member-wise. Returns the
    substitution list (also written to `<dst>.bound.json`)."""
    doc = read_json(src_p)
    subs = []
    _bind_slot_variants((doc.get("assembly") or {}).get("slots"), subs)
    for n in doc.get("nodes") or []:
        nat = ((n.get("semantic") or {}).get("body") or {})
        _bind_slot_variants((nat.get("native") or {}).get("slots"), subs)
    with open(dst_p, "w") as f:
        json.dump(doc, f)
    with open(dst_p + ".bound.json", "w") as f:
        json.dump({"schema": "hh-ws1-hir-bind/1",
                   "source": src_p,
                   "source_sha256": hashlib.sha256(
                       open(src_p, "rb").read()).hexdigest(),
                   "substitutions": subs}, f, indent=1, sort_keys=True)
    return subs

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cfg", required=True)
    ap.add_argument("--corpus-dir", required=True)
    ap.add_argument("--gate-a-list", required=True)
    ap.add_argument("--h-set-dir", required=True)
    ap.add_argument("--layers-file", required=True)
    ap.add_argument("--admission", required=True)
    ap.add_argument("--proxy-bin", required=True)
    ap.add_argument("--w1-bin", required=True)
    ap.add_argument("--l1-drive-bin", required=True)
    ap.add_argument("--session-shim",
                    default=os.path.join(os.path.dirname(
                        os.path.abspath(__file__)),
                        "ws1-proxy-session.py"))
    ap.add_argument("--lane-file", required=True)
    ap.add_argument("--authorization-file", required=True)
    ap.add_argument("--canon-id", required=True)
    ap.add_argument("--instruction-dir", default=None)
    ap.add_argument("--mswea-pack", default=None,
                    help="dir with system_template.tmpl + "
                         "instance_template.tmpl — injected into every "
                         "hh_l1_drive invocation (the repaired profile "
                         "layer the authored HIR's placeholder pin lacks)")
    ap.add_argument("--coverage", default=None,
                    help="fold coverage JSON (corpus leg output)")
    ap.add_argument("--census-covered", type=int, default=0)
    ap.add_argument("--legs", default="l10_a,l10_b,l1_1,l1_2",
                    help="comma subset of legs to drive; skipped legs "
                         "fold their durable prior-drive stores")
    ap.add_argument("--base-port", type=int, default=44100)
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--run-root", required=True)
    a = ap.parse_args()

    gate_a = read_json(a.gate_a_list)
    os.makedirs(a.out_dir, exist_ok=True)
    os.makedirs(a.run_root, exist_ok=True)
    layers = (read_json(a.layers_file).get("mini-swe-agent") or {}) \
        if os.path.exists(a.layers_file) else {}
    h_set = read_json(os.path.join(a.h_set_dir, "h_set.json")) \
        if os.path.exists(os.path.join(a.h_set_dir, "h_set.json")) else {}
    hir_doc = None
    for m in h_set.get("members") or []:
        if (m.get("kind") or "") == "hir" or \
                "hir" in (m.get("name") or ""):
            p = os.path.join(a.h_set_dir, m.get("path") or m.get("name"))
            if os.path.exists(p):
                hir_doc = p
    # fall back: the binary-set's bare `hir` member (canonical name,
    # no extension), then any .json under the set that parses as a
    # HIR doc
    if hir_doc is None:
        p = os.path.join(a.h_set_dir, "hir")
        if os.path.isfile(p):
            try:
                d = read_json(p)
                if isinstance(d, dict) and \
                        ("assembly" in d or
                         (d.get("schema") or "").startswith("hh-hir")):
                    hir_doc = p
            except Exception:
                pass
    if hir_doc is None:
        for dp, _, fns in os.walk(a.h_set_dir):
            for fn in fns:
                if fn.endswith(".json"):
                    d = read_json(os.path.join(dp, fn))
                    if (d.get("schema") or "").startswith("hh-hir") or \
                            "hir" in fn:
                        hir_doc = os.path.join(dp, fn)
                        break
            if hir_doc:
                break
    h_identity = h_set.get("identity") or {
        "participant_ref": h_set.get("participant_ref") or "H",
        "version": h_set.get("version") or "frozen",
        "digest": h_set.get("set_digest") or ""}
    # The frozen `hir` member carries window-authored placeholder
    # refs — derive the bound doc once (recorded substitutions; the
    # frozen member is never written) and drive every H leg off it.
    hir_bound = None
    if hir_doc is not None:
        hir_bound = os.path.join(a.run_root, "hir-bound.json")
        try:
            subs = bind_hir(hir_doc, hir_bound)
            eprint("hir-bind: %d substitutions -> %s" % (len(subs), hir_bound))
        except Exception as e:
            eprint("hir-bind failed: %s" % e)
            hir_bound = None
    o_identity = {"participant_ref": "mini-swe-agent",
                  "version": "original", "digest": ""}
    corpus_digest = ""
    cov_path = os.path.join(a.corpus_dir, "_corpus.json")
    if os.path.exists(cov_path):
        corpus_digest = (read_json(cov_path).get("corpus_digest") or "")
    coverage = read_json(a.coverage) if a.coverage else None
    leg_set = set(l.strip() for l in a.legs.split(",") if l.strip())
    seqs = []
    legs = []
    for ent in gate_a:
        sid = ent.get("slot_id") or ent.get("id")
        task = ent.get("task")
        rec_file = ent.get("recording")
        row = {"trajectory": sid, "task": task}
        rec_p = os.path.join(a.corpus_dir, rec_file) if rec_file else None
        if (not rec_p or not os.path.exists(rec_p)) and \
                os.path.isdir(a.corpus_dir):
            # resolve the sealed recording by its slot_id member
            for fn in os.listdir(a.corpus_dir):
                if not fn.endswith(".json"):
                    continue
                try:
                    d = read_json(os.path.join(a.corpus_dir, fn))
                except Exception:
                    continue
                if d.get("slot_id") == sid and \
                        d.get("schema") == "hh.model_proxy.recording/1":
                    rec_p = os.path.join(a.corpus_dir, fn)
                    break
        recording = read_json(rec_p) if rec_p and os.path.exists(rec_p) \
            else {"calls": []}
        n_calls = recorded_len(recording)
        instr_p = None
        if a.instruction_dir and os.path.isdir(a.instruction_dir):
            for fn in sorted(os.listdir(a.instruction_dir)):
                if task and task in fn:
                    instr_p = os.path.join(a.instruction_dir, fn)
        # ── raws for L1.1's obs replay ───────────────────────────
        raws_p = os.path.join(a.run_root, f"{sid}-raws.json")
        # staged faithful raws (rebuilt from the durable captured
        # bodies — `l1-raws` on the crash-recovered recording replayed
        # its lossy capture) are used as-is; otherwise extract
        if os.path.exists(raws_p):
            rc, so, se = 0, "", "staged"
        else:
            rc, so, se = run([a.w1_bin, "l1-raws", "--recording", rec_p,
                              "--out", raws_p]) if rec_p else \
                (1, "", "no rec")
        if rc != 0:
            row["l1_1"] = {"status": "machinery{raws_extract}",
                           "detail": se[:200]}
        else:
            # ADR-0450 item 3 — the terminal `echo` the marker
            # convention intercepts leaves no recorded observation;
            # the queue gains the deterministic output, typed
            # `reconstructed` (covers staged raws too).
            try:
                raws = read_json(raws_p) or []
                tr = terminal_echo_raw(recording, raws)
                if tr is not None:
                    raws.append(tr)
                    with open(raws_p, "w") as f:
                        json.dump(raws, f)
            except Exception:
                pass
        # ── L1.0: two O drives ───────────────────────────────────
        o_docs = []
        for run_tag in ("a", "b"):
            slot = f"l10-{sid}-{run_tag}"
            leg_key = f"l10_{run_tag}"
            if leg_key not in leg_set:
                # fold the prior drive's durable store — the leg was
                # already exercised; re-reading it keeps the fold
                # complete without re-driving
                cell_dir = os.path.join(a.run_root, f"{slot}-cell")
                traj = {}
                for dp, _, fns in os.walk(cell_dir):
                    for fn in fns:
                        if fn.endswith(".traj.json") or \
                                fn.endswith("trajectory.json"):
                            traj = read_json(os.path.join(dp, fn))
                store = os.path.join(a.run_root, slot, "store")
                o_docs.append(o_drive(store, slot, traj, n_calls))
                row[leg_key] = {"status": "reused{durable}"}
                continue
            try:
                proc, sentinel, sess_out, listen = start_replay(
                    a, slot, recording, strict=False)
                grants = read_json(sess_out).get("grants") or {}
                token = (grants.get(slot) or {}).get("slot_token")
                cell_dir = os.path.join(a.run_root, f"{slot}-cell")
                os.makedirs(cell_dir, exist_ok=True)
                tok_file = os.path.join(cell_dir, "slot-token")
                with open(tok_file, "w") as f:
                    f.write(token or "")
                os.chmod(tok_file, 0o600)
                image = (layers.get(task) or "")
                cmd = [a.w1_bin, "arm-run", "--cfg", a.cfg,
                       "--task", task, "--image", image,
                       "--admission", a.admission,
                       "--slot-id", slot,
                       "--slot-token-file", tok_file,
                       "--model-upstream", listen.split(":")[-1],
                       "--slot-slice-micros", "0",
                       "--out", cell_dir]
                rc, so, se = run(cmd)
                traj = {}
                for dp, _, fns in os.walk(cell_dir):
                    for fn in fns:
                        if fn.endswith(".traj.json") or \
                                fn.endswith("trajectory.json"):
                            traj = read_json(os.path.join(dp, fn))
                close_replay(proc, sentinel)
                store = os.path.join(a.run_root, slot, "store")
                o_docs.append(o_drive(store, slot, traj, n_calls))
                row[f"l10_{run_tag}"] = {"status": "exited{%d}" % rc
                                        if rc == 0 else
                                        "machinery{arm_run:rc=%d}" % rc}
            except Exception as e:
                o_docs.append({"requests": [], "parsed_actions": [],
                               "executed_actions": [],
                               "termination": "exception{drive}",
                               "submission": None,
                               "extra_request": False, "refusal": None})
                row[f"l10_{run_tag}"] = {"status": "machinery{l10}",
                                        "detail": str(e)[:200]}
        while len(o_docs) < 2:
            o_docs.append({"requests": [], "parsed_actions": [],
                           "executed_actions": [],
                           "termination": "exception{drive}",
                           "submission": None, "extra_request": False,
                           "refusal": None})
        # ── L1.1 / L1.2: H drives ────────────────────────────────
        h_docs, ep_firsts = {}, {}
        for leg, mode, strict in (("l1_1", "obs_replay", True),
                                  ("l1_2", "live", False)):
            slot = f"{leg.replace('_', '')}-{sid}"
            if leg not in leg_set:
                store = os.path.join(a.run_root, slot, "store")
                drive_dir = os.path.join(a.run_root, f"{slot}-drive")
                ev_p = os.path.join(drive_dir, "events.json")
                events = read_json(ev_p) if os.path.exists(ev_p) else []
                h_docs[leg], ep_firsts[leg] = h_drive(
                    store, slot, events, n_calls)
                row[leg] = {"status": "reused{durable}"}
                continue
            if hir_bound is None:
                h_docs[leg] = {"requests": [], "parsed_actions": [],
                               "executed_actions": [],
                               "termination": "exception{no_hir}",
                               "submission": None,
                               "extra_request": False, "refusal": None}
                row[leg] = {"status": "machinery{h_set:hir_absent}"}
                ep_firsts[leg] = None
                continue
            try:
                proc, sentinel, sess_out, listen = start_replay(
                    a, slot, recording, strict=strict)
                grants = read_json(sess_out).get("grants") or {}
                token = (grants.get(slot) or {}).get("slot_token")
                drive_dir = os.path.join(a.run_root, f"{slot}-drive")
                # a re-drive re-registers the (possibly rebound)
                # definition — a retained drive store would refuse
                # the conflicting version body
                import shutil
                shutil.rmtree(drive_dir, ignore_errors=True)
                env = dict(os.environ)
                env["HH_LOCAL_BASE_URL"] = f"http://{listen}/v1"
                env["HH_LOCAL_MODEL"] = ""
                # the slot token rides the HIR's declared
                # credential_ref env binding — never argv
                cred_envs = set(["OPENAI_API_KEY"])
                try:
                    hd = read_json(hir_bound)
                    params = (((hd.get("assembly") or {}).get("slots") or {})
                              .get("model_port") or {}).get("one", {}) \
                        .get("params") or {}
                    ref = params.get("credential_ref") or ""
                    if ref.startswith("env:"):
                        cred_envs.add(ref[4:])
                except Exception:
                    pass
                for name in cred_envs:
                    if token:
                        env[name] = token
                # the cell's own containment statement — mswea drives
                # admit arbitrary unscoped bash (the surface's `command`
                # arg is scope_unknown; `/` is the anchored prefix that
                # covers any scoped path too). No pack ⇒ legacy bare
                # record (no armed exec surface anyway).
                conn = {"class": "local_host"}
                if a.mswea_pack:
                    conn["exec_allow"] = ["/"]
                env_rec = {"kind": "connection_info",
                           "connection_info": conn}
                cmd = [a.l1_drive_bin,
                       "--store-root", os.path.join(drive_dir, "store"),
                       "--workspace-root", os.path.join(drive_dir, "ws"),
                       "--holder", "hh-ws1-l1",
                       "--definition", hir_bound,
                       "--environment", json.dumps(env_rec),
                       "--instruction", instr_p or "/dev/null",
                       "--endpoint", f"http://{listen}/v1",
                       "--model", recorded_model(recording),
                       "--mode", mode,
                       "--max-submits", "4",
                       "--out", drive_dir]
                if a.mswea_pack:
                    cmd += ["--mswea-pack", a.mswea_pack]
                if mode == "obs_replay" and os.path.exists(raws_p):
                    cmd += ["--obs", raws_p]
                rc, so, se = run(cmd, env=env, timeout=7200)
                os.makedirs(drive_dir, exist_ok=True)
                for name, data in (("drive-stdout.txt", so),
                                   ("drive-stderr.txt", se)):
                    if data:
                        with open(os.path.join(drive_dir, name),
                                  "w") as f:
                            f.write(data)
                events = []
                ev_p = os.path.join(drive_dir, "events.json")
                if os.path.exists(ev_p):
                    events = read_json(ev_p)
                close_replay(proc, sentinel)
                store = os.path.join(a.run_root, slot, "store")
                h_docs[leg], ep_firsts[leg] = h_drive(
                    store, slot, events, n_calls)
                if rc != 0:
                    # the drive never armed — stamp the machinery
                    # refusal on the folded H drive so an empty
                    # `requests`/`exception{unfinished}` doc reads as
                    # the open/arm failure it is (the leg's own
                    # statuses carry the same fact; a vacuous-gate
                    # "pass" must never stand unqualified)
                    h_docs[leg]["refusal"] = {
                        "rule": "machinery{l1_drive:rc=%d}" % rc,
                        "detail": (se or "")[:200],
                    }
                row[leg] = {"status": "exited{%d}" % rc if rc == 0
                            else "machinery{l1_drive:rc=%d}" % rc,
                            "detail": (se or "")[:200]}
            except Exception as e:
                h_docs[leg] = {"requests": [], "parsed_actions": [],
                               "executed_actions": [],
                               "termination": "exception{drive}",
                               "submission": None,
                               "extra_request": False, "refusal": None}
                ep_firsts[leg] = None
                row[leg] = {"status": "machinery{%s}" % leg,
                            "detail": str(e)[:200]}
        seqs.append({
            "seq_id": sid,
            "o_a": o_docs[0], "o_b": o_docs[1],
            # ADR-0450 — the recording-derived L1.1 reference; o_a/o_b
            # stay the L1.0 prefix inputs and l1_2's comparator.
            "o_rec": o_rec_drive(recording),
            "h1": h_docs.get("l1_1"), "h2": h_docs.get("l1_2"),
            "endpoint_first_1": ep_firsts.get("l1_1"),
            "endpoint_first_2": ep_firsts.get("l1_2"),
        })
        legs.append(row)
    # ── the fold ─────────────────────────────────────────────────
    fold_dir = os.path.join(a.out_dir, "fold")
    spec = {
        "canon_id": a.canon_id,
        "surface": "openai-chat",
        "cell_facts": [],
        "corpus_digest": corpus_digest,
        "h_id": h_identity,
        "o_id": o_identity,
        "coverage": coverage or {},
        "census_covered": a.census_covered,
        "recorded_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                     time.gmtime()),
        "seqs": seqs,
    }
    spec_p = os.path.join(a.run_root, "l1-fold-spec.json")
    with open(spec_p, "w") as f:
        json.dump(spec, f)
    rc, so, se = run([a.w1_bin, "l1-fold", "--spec", spec_p,
                      "--out", fold_dir])
    fold_row = {"status": "exited{%d}" % rc if rc == 0
                else "machinery{l1_fold:rc=%d}" % rc,
                "detail": (se or "")[:200]}
    summary = {"schema": "hh-ws1-l1-summary/1",
               "gate_a": len(legs),
               "legs": legs,
               "fold": fold_row,
               "fold_dir": fold_dir,
               "run_at": time.strftime("%Y-%m-%dT%H:%M:%SZ",
                                       time.gmtime())}
    with open(os.path.join(a.out_dir, "_l1-summary.json"), "w") as f:
        json.dump(summary, f, indent=1, sort_keys=True)
    print(json.dumps({"gate_a": len(legs), "fold": fold_row}),
          flush=True)
    sys.exit(0 if rc == 0 else 1)

if __name__ == "__main__":
    main()
