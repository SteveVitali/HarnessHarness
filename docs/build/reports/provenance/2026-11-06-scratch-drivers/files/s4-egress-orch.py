#!/usr/bin/env python3
"""S4 egress supplement orchestrator — reruns ONLY the prior cohort's
inconclusive slots under the bounded egress profile.

Bounded admission (recorded verbatim per slot, never a silent bypass):
- `egress_profile: declared_deps` — the slot's cell runs the scoped
  helper variant `/tmp/s4-helper-egress` (hh_helper.rs verbatim +
  the `network` knob) with `--network bridge`; the containment policy
  honestly declares `net.mode = public` for those cells.
- Slots whose tasks don't need egress keep `--network none` — in this
  supplement every rerun task declares egress deps (the uv/pytest
  verifier bootstrap at minimum), so all 140 are admitted.
- The SAME frozen manifest (protocol_id 9edc5327, seed v3) is consumed
  verbatim — slot_refs, arm bindings, lanes, rounds identical. These
  are named supplement attempts (`supplement_epoch:
  egress-profile-rerun`); each record carries `prior_disposition`
  verbatim. The original inconclusive records stay untouched.

Usage: python3 s4-egress-orch.py [start_task_idx] [end_task_idx] [task_glob]
"""
import json
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path.home() / ".local/share/uv/tools/terminal-bench/lib/python3.12/site-packages"))
import yaml  # noqa: E402
from terminal_bench.parsers.pytest_parser import PytestParser  # noqa: E402

DATASET = Path.home() / "tb-work/dataset"
WORK = Path.home() / "tb-work/s4-egress"
SLOTS_DIR = WORK / "slots"
SPECS_DIR = WORK / "specs"
CELL = Path("/tmp/s4-cell/target/debug/s4-cell")
HELPER_STOCK = "/Users/stevenvitali/MetaHarness-harnessharness/target/debug/hh-helper"
HELPER_EGRESS = "/tmp/s4-helper-egress/target/debug/s4-helper-egress"
MANIFEST = Path("/tmp/parity-manifest-v3.json")
PRIOR = Path("/Users/stevenvitali/MetaHarness-harnessharness/docs/build/reports/2026-11-05-s4-confirmation.json")
PARSER = PytestParser()
SSH = ["ssh", "-i",
       str(Path.home() / ".local/share/containers/podman/machine/machine"),
       "-o", "StrictHostKeyChecking=no", "-p", "62382", "core@127.0.0.1"]
ANSI_OSC = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
ANSI_CSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
ANSI_MISC = re.compile(r"\x1b(?:[()][0-2]|[=>#][0-9]?|[@-Z\\-_]|[\^_][^\x1b\x07]*(?:\x07|\x1b\\))")


def strip_ansi(s: str) -> str:
    s = ANSI_OSC.sub("", s)
    s = ANSI_CSI.sub("", s)
    s = ANSI_MISC.sub("", s)
    s = s.replace("\x1b", "")
    return "\n".join(line.split("\r")[-1] for line in s.split("\n"))


def sh(args, timeout=None, input_bytes=None):
    t0 = time.time()
    try:
        p = subprocess.run(args, capture_output=True, timeout=timeout,
                           input=input_bytes)
        return {"rc": p.returncode,
                "stdout": p.stdout.decode("utf-8", "replace"),
                "stderr": p.stderr.decode("utf-8", "replace"),
                "duration_sec": round(time.time() - t0, 2),
                "timeout": False}
    except subprocess.TimeoutExpired as e:
        return {"rc": None,
                "stdout": (e.stdout or b"").decode("utf-8", "replace"),
                "stderr": (e.stderr or b"").decode("utf-8", "replace"),
                "duration_sec": round(time.time() - t0, 2),
                "timeout": True}


def vm_free_gib():
    """VM /var free GiB via the recorded SSH recipe (the machine's name
    is `podman`, not podman-machine-default — `podman machine ssh`
    silently answered nothing in the frozen run; fixed here)."""
    r = sh(SSH + ["df", "-BG", "/var"], timeout=30)
    out = r["stdout"] + r["stderr"]
    m = re.findall(r"(\d+)G\s+\d+%\s+/var", out)
    if m:
        return int(m[-1])
    m = re.findall(r"/dev/\S+\s+\d+G\s+\d+G\s+(\d+)G", out)
    return int(m[-1]) if m else None


def disk_hygiene(log):
    free = vm_free_gib()
    log.append({"event": "disk_check", "vm_free_gib": free})
    if free is not None and free < 8:
        log.append({"event": "prune", "reason": "free<8GiB"})
        sh(["podman", "system", "prune", "-af", "--volumes"], timeout=600)
        try:
            sh(["docker", "buildx", "prune", "-af"], timeout=600)
        except Exception:
            pass
        sh(["podman", "builder", "prune", "-af"], timeout=600)
    return free


# ── per-task config derivation (verbatim from s4-orch.py) ───────────

PKG_FLAG = re.compile(r"^--?[a-zA-Z]")


def extract_pkgs(text, kind):
    out = []
    pat = (r"(?:pip3?|python3? -m pip)\s+install\s+([^;&|\\\n]+)" if kind == "pip"
           else r"apt(?:-get)?\s+install\s+([^;&|\\\n]+)")
    for m in re.finditer(pat, text):
        for tok in m.group(1).split():
            if PKG_FLAG.match(tok) or tok in ("install",):
                continue
            out.append(tok)
    return out


def ctr_name(task_id):
    return f"tb__{task_id.replace('.', '-')}"


def task_config(task_id):
    td = DATASET / task_id
    cfg = yaml.safe_load((td / "task.yaml").read_text()) or {}
    compose = yaml.safe_load((td / "docker-compose.yaml").read_text()) or {}
    services = compose.get("services") or {}
    client = services.get("client") or {}
    bld = client.get("build") or {}
    ctx = bld.get("context", ".") if isinstance(bld, dict) else (bld or ".")
    dfile = bld.get("dockerfile", "Dockerfile") if isinstance(bld, dict) else "Dockerfile"
    plat = client.get("platform")
    if not plat:
        df = (td / ctx / dfile)
        head = df.read_text() if df.exists() else ""
        m = re.search(r"FROM\s+--platform=(\S+)", head)
        if m:
            plat = m.group(1)
    env = []
    for item in client.get("environment") or []:
        if isinstance(item, str) and "=" in item:
            k, v = item.split("=", 1)
            v = re.sub(r"\$\{T_BENCH_TEST_DIR\}", "/tests", v)
            env.append([k, v])
    sol_sh = td / "solution.sh"
    sol_yaml = td / "solution.yaml"
    solution_kind = "sh" if sol_sh.exists() else ("yaml" if sol_yaml.exists() else "none")
    bake_text = ""
    if sol_sh.exists():
        bake_text += sol_sh.read_text()
    if sol_yaml.exists():
        for c in yaml.safe_load(sol_yaml.read_text()) or []:
            bake_text += (c.get("command") or "") + "\n"
    rts = td / "run-tests.sh"
    if rts.exists():
        bake_text += rts.read_text()
    bake_pip = sorted(set(extract_pkgs(bake_text, "pip")) | {"pytest", "requests", "psutil"})
    bake_apt = sorted(set(extract_pkgs(bake_text, "apt")))
    pytest_target = "/tests/test_outputs.py -rA"
    if rts.exists():
        pm = re.search(r"pytest\s+(\$TEST_DIR/\S+|/tests/\S+)([^\n]*)", rts.read_text())
        if pm:
            tgt = re.sub(r"\$TEST_DIR", "/tests", pm.group(1))
            pytest_target = f"{tgt} -rA"
    # declared egress deps — the mechanical scan the admission is
    # bounded by: the task's solution/verifier fetch at runtime
    # (apt/pip/uv bootstrap/git clone/curl|wget to an external host or
    # a sibling service/HF download). Offline-only tasks keep `none`.
    dep_sigs = {
        "apt": bool(re.search(r"apt(?:-get)?\s+(?:update|install)", bake_text)),
        "pip": bool(re.search(r"(?:pip3?|python3? -m pip|uv add|uv pip)\s+install|uv add\b", bake_text)),
        "uv_bootstrap": "setup-uv-pytest.sh" in bake_text or "astral.sh/uv" in bake_text,
        "git_clone": bool(re.search(r"git\s+clone", bake_text)),
        "fetch": bool(re.search(r"(?:curl|wget)\s+[^\n]*https?://", bake_text)),
        "hf_hub": bool(re.search(r"huggingface|hf_hub|from_pretrained|snapshot_download", bake_text)),
        "remote_rpc": bool(re.search(r"https?://node\d|https?://[a-z0-9.-]+:\d+", bake_text)),
    }
    return {
        "task_id": task_id,
        "ctx": str(td / ctx),
        "dockerfile": dfile,
        "platform": plat,
        "client_env": env,
        "solution_kind": solution_kind,
        "multi_service": len(services) > 1,
        "compose_services": list(services),
        "bake_pip": bake_pip,
        "bake_apt": bake_apt,
        "pytest_target": pytest_target,
        "declared_deps": dep_sigs,
        "needs_egress": any(dep_sigs.values()),
        "max_agent_timeout_sec": float(cfg.get("max_agent_timeout_sec", 360.0)),
        "max_test_timeout_sec": float(cfg.get("max_test_timeout_sec", 60.0)),
        "task_yaml": {k: cfg[k] for k in ("difficulty", "instruction", "parser_name") if k in cfg},
    }


def vendored_cmd(task_id, cfg):
    if task_id == "swe-bench-langcodes":
        return ("cd /app/langcodes && patch --fuzz=5 -p1 -i /app/test_patch.diff && "
                "PYTHONPATH=/app/langcodes TEST_DIR=/tests python3 -m pytest -rA -v "
                "langcodes/tests/test_language.py > /work/verdict.log 2>&1; "
                "echo $? > /work/verdict.rc")
    if task_id == "swe-bench-astropy-2":
        return ("cd /app/astropy && patch --fuzz=5 -p1 -i /app/test_patch.diff && "
                "PYTHONPATH=/app/astropy TEST_DIR=/tests python3 -m pytest -rA -v "
                "astropy/io/ascii/tests/test_qdp.py > /work/verdict.log 2>&1; "
                "echo $? > /work/verdict.rc")
    return None


# ── image build + derive (verbatim pipeline from the frozen run) ────

def build_images(task_id, cfg, log):
    slug = ctr_name(task_id)
    client_tag = f"localhost/{slug}__client:latest"
    derived_tag = f"localhost/s4-{slug}:latest"
    plat = cfg["platform"] or "linux/arm64"
    rec = {"client_tag": client_tag, "derived_tag": derived_tag,
           "platform": plat, "bake_pip": cfg["bake_pip"],
           "bake_apt": cfg["bake_apt"]}
    b = sh(["podman", "build", "--platform", plat,
            "-f", str(Path(cfg["ctx"]) / cfg["dockerfile"]),
            "-t", client_tag, cfg["ctx"]], timeout=14400)
    rec["client_build"] = {"rc": b["rc"], "duration_sec": b["duration_sec"],
                           "stderr_tail": b["stderr"][-400:],
                           "stdout_tail": b["stdout"][-400:]}
    if b["rc"] != 0:
        rec["derived_build"] = None
        return None, rec
    insp = sh(["podman", "image", "inspect", client_tag,
               "--format", "{{.Id}} {{.Digest}}"])
    rec["client_image_id_digest"] = insp["stdout"].strip()
    apt = " ".join(cfg["bake_apt"])
    pip = " ".join(cfg["bake_pip"])
    df = f"""FROM {client_tag}
USER root
RUN set -x; \\
    if command -v apt-get >/dev/null 2>&1; then \\
      apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y {apt} || true; \\
    fi; \\
    PIP=$(command -v pip || command -v pip3 || true); \\
    if [ -n "$PIP" ]; then \\
      $PIP install --no-cache-dir --break-system-packages {pip} || \\
      $PIP install --no-cache-dir {pip} || true; \\
    fi; \\
    true
"""
    dfd = WORK / "dockerfiles" / slug
    dfd.mkdir(parents=True, exist_ok=True)
    (dfd / "Dockerfile").write_text(df)
    b2 = sh(["podman", "build", "--platform", plat,
             "-f", str(dfd / "Dockerfile"), "-t", derived_tag, str(dfd)],
            timeout=14400)
    rec["derived_build"] = {"rc": b2["rc"], "duration_sec": b2["duration_sec"],
                            "stderr_tail": b2["stderr"][-400:],
                            "dockerfile": df}
    if b2["rc"] != 0:
        return None, rec
    insp = sh(["podman", "image", "inspect", derived_tag,
               "--format", "{{.Id}} {{.Digest}}"])
    rec["derived_image_id_digest"] = insp["stdout"].strip()
    pr = sh(["podman", "run", "--rm", derived_tag, "sh", "-c",
             "python3 -c 'import pytest,sys; print(\"pytest\", pytest.__version__, sys.version.split()[0])'"],
            timeout=120)
    rec["probe_pytest"] = {"rc": pr["rc"], "out": pr["stdout"].strip() or pr["stderr"].strip()[-200:]}
    pn = sh(["podman", "run", "--rm", derived_tag, "sh", "-c",
             "echo net-check; ip route 2>/dev/null | head -2"], timeout=60)
    rec["probe_env"] = {"rc": pn["rc"], "out": pn["stdout"].strip()[:200]}
    return derived_tag, rec


# ── solution stream for yaml tasks ───────────────────────────────────

def stream_path(task_id):
    td = DATASET / task_id
    cmds = yaml.safe_load((td / "solution.yaml").read_text()) or []
    stream = ""
    for c in cmds:
        stream += c.get("command", "")
        if c.get("append_enter", True):
            stream += "\n"
    p = WORK / "stage" / task_id
    p.mkdir(parents=True, exist_ok=True)
    f = p / "stream.sh"
    f.write_text(stream)
    return f


# ── slot drive ───────────────────────────────────────────────────────

def run_slot(spec, task_id, cfg):
    spec_path = SPECS_DIR / f"{spec['slot_ref'].replace('slot:', 's')}.json"
    spec_path.write_text(json.dumps(spec))
    import os
    env = dict(os.environ)
    env["HH_HELPER_BIN"] = spec.get("helper_bin") or HELPER_STOCK
    t0 = time.time()
    try:
        p = subprocess.run([str(CELL), str(spec_path)], capture_output=True,
                           timeout=3300, env=env)
        r = {"rc": p.returncode,
             "stdout": p.stdout.decode("utf-8", "replace"),
             "stderr": p.stderr.decode("utf-8", "replace"),
             "duration_sec": round(time.time() - t0, 2), "timeout": False}
    except subprocess.TimeoutExpired as e:
        r = {"rc": None,
             "stdout": (e.stdout or b"").decode("utf-8", "replace"),
             "stderr": (e.stderr or b"").decode("utf-8", "replace"),
             "duration_sec": round(time.time() - t0, 2),
             "timeout": True}
    out = r["stdout"].strip()
    try:
        rec = json.loads(out.splitlines()[-1])
    except Exception as e:
        rec = {"slot_ref": spec["slot_ref"], "task_id": task_id,
               "driver_error": f"no_json:{e}",
               "driver_stdout_tail": out[-800:], "driver_stderr_tail": r["stderr"][-800:],
               "disposition": "failed{driver:no_json_output}"}
    rec["_driver_rc"] = r["rc"]
    rec["_driver_duration_sec"] = r["duration_sec"]
    return rec


def parse_verdict(rec, ws_dir: Path):
    for channel, fname in (("verbatim", "verdict_verbatim.log"),
                           ("vendored_offline", "verdict.log")):
        p = ws_dir / fname
        if not p.exists():
            continue
        raw = p.read_bytes().decode("utf-8", "replace")
        pane = strip_ansi(raw)
        parsed = None
        try:
            res = PARSER.parse(pane)
            parsed = {k: v.value for k, v in res.items()}
        except Exception:
            parsed = None
        if parsed:
            verdict = ("resolved" if all(v == "passed" for v in parsed.values())
                       else "unresolved")
            reason = "all_tests_passed" if verdict == "resolved" else "some_tests_failed"
            return {"channel": channel, "verdict": verdict,
                    "verdict_reason": reason, "parsed": parsed,
                    "raw_bytes": len(raw), "stripped_bytes": len(pane),
                    "ansi_stripped": raw != pane}
        tail = pane[-300:]
        if channel == "verbatim":
            continue
        return {"channel": channel, "verdict": None,
                "verdict_reason": f"no_pytest_summary:{tail.strip()[-160:]}"}
    return None


# ── main ─────────────────────────────────────────────────────────────

def main():
    start = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    end = int(sys.argv[2]) if len(sys.argv) > 2 else 30
    glob = sys.argv[3] if len(sys.argv) > 3 else None
    manifest = json.loads(MANIFEST.read_text())
    prior = json.loads(PRIOR.read_text())
    prior_disp = {s["slot_ref"]: s["disposition"] for s in prior["slots"]}
    selection = sorted(manifest["selection"], key=lambda t: t["position"])
    schedule = manifest["schedule"]
    tranche_log = []
    (WORK / "logs").mkdir(parents=True, exist_ok=True)
    SLOTS_DIR.mkdir(parents=True, exist_ok=True)
    SPECS_DIR.mkdir(parents=True, exist_ok=True)
    run_rec = {"started_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
               "protocol_id": manifest["protocol_id"],
               "supplement_epoch": "egress-profile-rerun",
               "egress_profile": "declared_deps"}

    for pos, sel in enumerate(selection):
        if pos < start or pos >= end:
            continue
        task_id = sel["task_id"]
        if glob and glob not in task_id:
            continue
        tdir = DATASET / task_id
        # supplement scope: only this task's prior-INCONCLUSIVE slots
        task_slots = [s for s in schedule
                      if s["task_id"] == task_id
                      and str(prior_disp.get(s["slot_ref"], "")).startswith("inconclusive")]
        task_slots.sort(key=lambda s: (s["round"], 0 if s["arm_id"].endswith("o-tb-oracle") else 1))
        if not task_slots:
            continue
        print(f"[{pos:2d}/30] {task_id} supplement_slots={len(task_slots)}", flush=True)
        cfg = task_config(task_id)
        needs_egress = cfg["needs_egress"]
        print(f"    declared_deps={cfg['declared_deps']} -> "
              f"{'egress:bridge' if needs_egress else 'none'}", flush=True)
        disk_hygiene(tranche_log)
        image, brec = build_images(task_id, cfg, tranche_log)
        (WORK / "logs" / f"{pos:02d}-{task_id.replace('/', '_')}-build.json").write_text(
            json.dumps({"task_id": task_id, "config": {k: v for k, v in cfg.items()
                                                      if k != "task_yaml"},
                        "task_yaml": cfg["task_yaml"],
                        "build": brec}, indent=2))
        for s in task_slots:
            slug = f"{pos:02d}-{task_id}-r{s['round']}-{s['arm_id'].split(':')[1]}"
            out_file = SLOTS_DIR / f"{slug}.json"
            if out_file.exists():
                print(f"    slot {slug} recorded, skip", flush=True)
                continue
            strategy = ("evict_oldest" if s["arm_id"].endswith("o-tb-oracle")
                        else "clear_tool_results")
            prior_d = prior_disp[s["slot_ref"]]
            if image is None:
                rec = {"schema": "hh-s4-slot/1", "slot_ref": s["slot_ref"],
                       "task_id": task_id, "arm_id": s["arm_id"],
                       "strategy": strategy, "round": s["round"],
                       "first": s["first"], "lane_id": s["lane_id"],
                       "cell_attempted": False,
                       "attempts": [], "calls": 0,
                       "prior_disposition": prior_d,
                       "supplement_epoch": "egress-profile-rerun",
                       "egress_profile": "declared_deps" if needs_egress else "none",
                       "cell_network": "bridge" if needs_egress else "none",
                       "policy_net_mode": "public" if needs_egress else "none",
                       "disposition": "inconclusive{image_unavailable:client_build_failed}"}
                out_file.write_text(json.dumps(rec, indent=2))
                print(f"    slot {slug} -> inconclusive image", flush=True)
                continue
            spec = {
                "slot_ref": s["slot_ref"], "task_id": task_id,
                "arm_id": s["arm_id"], "strategy": strategy,
                "round": s["round"], "first": s["first"],
                "lane_id": s["lane_id"], "image": image,
                "task_dir": str(tdir), "work_root": str(WORK),
                "solution_kind": cfg["solution_kind"],
                "max_agent_timeout_sec": int(cfg["max_agent_timeout_sec"]),
                "max_test_timeout_sec": int(cfg["max_test_timeout_sec"]),
                "pytest_target": cfg["pytest_target"],
                "solution_file": (str(stream_path(task_id)) if cfg["solution_kind"] == "yaml"
                                  else str(tdir / "solution.sh")),
                "projected_env": cfg["client_env"] + [["TEST_DIR", "/tests"]],
                # ── the bounded admission, verbatim ──
                "egress_profile": "declared_deps" if needs_egress else "none",
                "helper_bin": HELPER_EGRESS if needs_egress else HELPER_STOCK,
                "policy_net_mode": "public" if needs_egress else "none",
            }
            vc = vendored_cmd(task_id, cfg)
            if vc:
                spec["vendored_cmd"] = vc
            if cfg["multi_service"]:
                spec["multi_service"] = True
            rec = run_slot(spec, task_id, cfg)
            rec["prior_disposition"] = prior_d
            rec["supplement_epoch"] = "egress-profile-rerun"
            ws_dir = None
            wr = Path(WORK) / "ws"
            for a in rec.get("attempts", []):
                cand = wr / a
                if cand.exists():
                    ws_dir = cand
            receipt = parse_verdict(rec, ws_dir) if ws_dir else None
            if receipt:
                rec["receipt"] = receipt
            egress = None
            if ws_dir is not None and (ws_dir / "solution.log").exists():
                slog = (ws_dir / "solution.log").read_bytes().decode("utf-8", "replace")
                for sig in ("Temporary failure resolving", "Could not resolve",
                            "Failed to fetch", "Connection refused",
                            "Network is unreachable", "No route to host",
                            "Could not connect", "name resolution"):
                    if sig in slog:
                        egress = sig
                        break
            rec["egress_evidence"] = egress
            if not rec.get("disposition"):
                # the isolation signature only holds under --network
                # none — under declared_deps a refused connection is a
                # workload outcome, not cell isolation (honest record:
                # r1-o-tb-oracle's localhost:5000 refusal was the
                # oracle's own flask race, corrected with a
                # disposition_recomputed note).
                if (egress and rec.get("cell_network") == "none"
                        and rec.get("work_exit") not in ("0", None)):
                    rec["disposition"] = (f"inconclusive{{environment:cell_network_isolated:{egress}}}")
                elif receipt and receipt.get("verdict"):
                    rec["disposition"] = "executed"
                else:
                    rec["disposition"] = "inconclusive{verifier_unavailable:no_parseable_verdict}"
            out_file.write_text(json.dumps(rec, indent=2))
            disp = rec.get("disposition", "?")
            v = (rec.get("receipt") or {}).get("verdict", "-")
            print(f"    slot {slug} -> {disp} {v} wall={rec.get('wall_ms')}ms",
                  flush=True)
        for tag in (brec.get("derived_tag"), brec.get("client_tag")):
            if tag:
                sh(["podman", "rmi", "-f", tag], timeout=300)
    run_rec["ended_at"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    (WORK / "logs" / "orch-log.json").write_text(
        json.dumps({"run": run_rec, "log": tranche_log}, indent=2))


if __name__ == "__main__":
    main()
