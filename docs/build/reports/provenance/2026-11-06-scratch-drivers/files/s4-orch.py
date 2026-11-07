#!/usr/bin/env python3
"""S4 confirmation orchestrator — drives the 300-slot frozen cohort.

Per task: podman build the client image -> derive `s4-<task>:latest`
(the cell is --network none, so the packages the task's own
solution/verifier scripts install are pre-staged at build, recorded
verbatim) -> run each of the task's 10 slots through /tmp/s4-cell (the
HH subject machinery) -> PytestParser the verdict logs -> per-slot
records. Between tasks: derived+client images pruned; buildx/system
prune when VM free < 8 GiB.

Usage: python3 s4-orch.py [start_task_idx] [end_task_idx]
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
WORK = Path.home() / "tb-work/s4-confirm"
SLOTS_DIR = WORK / "slots"
SPECS_DIR = WORK / "specs"
CELL = Path("/tmp/s4-cell/target/debug/s4-cell")
MANIFEST = Path("/tmp/parity-manifest-v3.json")
PARSER = PytestParser()
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
    r = sh(["podman", "machine", "ssh", "df", "-BG", "/var"])
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
        sh(["docker", "buildx", "prune", "-af"], timeout=600)
    return free


# ── per-task config derivation (mechanical, verbatim) ────────────────

PKG_FLAG = re.compile(r"^--?[a-zA-Z]")


def extract_pkgs(text, kind):
    """Extract package args from `pip install X` / `apt-get install X`
    lines (flags stripped). Verbatim mechanical extraction."""
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
    """Read task.yaml + compose + solution + run-tests -> config."""
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
    # default pytest target = what the task's run-uv-pytest tail invokes
    pytest_target = "/tests/test_outputs.py -rA"
    if rts.exists():
        pm = re.search(r"pytest\s+(\$TEST_DIR/\S+|/tests/\S+)([^\n]*)", rts.read_text())
        if pm:
            tgt = re.sub(r"\$TEST_DIR", "/tests", pm.group(1))
            pytest_target = f"{tgt} -rA"
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
        "max_agent_timeout_sec": float(cfg.get("max_agent_timeout_sec", 360.0)),
        "max_test_timeout_sec": float(cfg.get("max_test_timeout_sec", 60.0)),
        "task_yaml": {k: cfg[k] for k in ("difficulty", "instruction", "parser_name") if k in cfg},
    }


# Per-task vendored verifier overrides (the derived image's pytest on
# the task's own test file — run-tests.sh's uv/pip bootstrap bypassed,
# recorded). swe-bench tasks apply their test_patch.diff (written by
# the verbatim attempt's heredoc before its apt leg dies offline) and
# put the cloned repo on PYTHONPATH (pip install -e can't run
# isolated).
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


# ── image build + derive ─────────────────────────────────────────────

def build_images(task_id, cfg, log):
    """Build the client image, then derive the s4 layer. Returns
    (derived_image_tag, build_record) or (None, record) on failure."""
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
    # offline probes — what actually landed
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

HELPER = "/Users/stevenvitali/MetaHarness-harnessharness/target/debug/hh-helper"


def run_slot(spec, task_id, cfg):
    spec_path = SPECS_DIR / f"{spec['slot_ref'].replace('slot:', 's')}.json"
    spec_path.write_text(json.dumps(spec))
    import os
    env = dict(os.environ)
    env["HH_HELPER_BIN"] = HELPER
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
             "duration_sec": round(time.time() - t0, 2), "timeout": True}
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


def receipt_from(rec, ws_name=None):
    """Pick the verdict channel and parse via tb's PytestParser."""
    cands = []
    if rec.get("verdict_verbatim.log.sha256"):
        cands.append(("verbatim", "verdict_verbatim.log"))
    if rec.get("verdict.log.sha256"):
        cands.append(("vendored_offline", "verdict.log"))
    # the driver may not have run either channel
    return cands


def parse_verdict(rec, ws_dir: Path):
    """Read the slot's verdict logs (collected into the slot's ws) and
    decide receipt + disposition."""
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
        # channel produced bytes but no parseable pytest summary
        tail = pane[-300:]
        if channel == "verbatim":
            continue  # fall through to the vendored record
        return {"channel": channel, "verdict": None,
                "verdict_reason": f"no_pytest_summary:{tail.strip()[-160:]}"}
    return None


# ── main ─────────────────────────────────────────────────────────────

def main():
    start = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    end = int(sys.argv[2]) if len(sys.argv) > 2 else 30
    manifest = json.loads(MANIFEST.read_text())
    selection = sorted(manifest["selection"], key=lambda t: t["position"])
    schedule = manifest["schedule"]
    tranche_log = []
    (WORK / "logs").mkdir(parents=True, exist_ok=True)
    SLOTS_DIR.mkdir(parents=True, exist_ok=True)
    SPECS_DIR.mkdir(parents=True, exist_ok=True)
    run_rec = {"started_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
               "protocol_id": manifest["protocol_id"]}

    for pos, sel in enumerate(selection):
        if pos < start or pos >= end:
            continue
        task_id = sel["task_id"]
        tdir = DATASET / task_id
        task_slots = [s for s in schedule if s["task_id"] == task_id]
        task_slots.sort(key=lambda s: (s["round"], 0 if s["arm_id"].endswith("o-tb-oracle") else 1))
        print(f"[{pos:2d}/30] {task_id} slots={len(task_slots)}", flush=True)
        if not tdir.exists():
            print("    task dir missing -> inconclusive slots", flush=True)
        cfg = task_config(task_id)
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
            if image is None:
                rec = {"schema": "hh-s4-slot/1", "slot_ref": s["slot_ref"],
                       "task_id": task_id, "arm_id": s["arm_id"],
                       "strategy": strategy, "round": s["round"],
                       "first": s["first"], "lane_id": s["lane_id"],
                       "cell_attempted": False,
                       "attempts": [], "calls": 0,
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
            }
            vc = vendored_cmd(task_id, cfg)
            if vc:
                spec["vendored_cmd"] = vc
            if cfg["multi_service"]:
                spec["multi_service"] = True
            # stage task files into the slot's ws naming the driver
            # expects (driver copies from task_dir paths)
            rec = run_slot(spec, task_id, cfg)
            # verdict parse — read the slot's ws dir
            ws_dir = Path(rec.get("ws_dir", "")) if rec.get("ws_dir") else None
            slot_ws = None
            # find the ws dir: work_root/ws/<attempt-id>
            wr = Path(WORK) / "ws"
            for a in rec.get("attempts", []):
                cand = wr / a
                if cand.exists():
                    slot_ws = cand
            receipt = parse_verdict(rec, slot_ws) if slot_ws else None
            if receipt:
                rec["receipt"] = receipt
            # egress-bound work? scan the solution log for network
            # failure signatures — the held-out task could not
            # genuinely run under the cell's --network none.
            egress = None
            if slot_ws is not None and (slot_ws / "solution.log").exists():
                slog = (slot_ws / "solution.log").read_bytes().decode("utf-8", "replace")
                for sig in ("Temporary failure resolving", "Could not resolve",
                            "Failed to fetch", "Connection refused",
                            "Network is unreachable", "No route to host",
                            "Could not connect", "name resolution"):
                    if sig in slog:
                        egress = sig
                        break
            rec["egress_evidence"] = egress
            if not rec.get("disposition"):
                if egress and rec.get("work_exit") not in ("0", None):
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
        # per-task teardown of images
        for tag in (brec.get("derived_tag"), brec.get("client_tag")):
            if tag:
                sh(["podman", "rmi", "-f", tag], timeout=300)
    run_rec["ended_at"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    (WORK / "logs" / "orch-log.json").write_text(
        json.dumps({"run": run_rec, "log": tranche_log}, indent=2))


if __name__ == "__main__":
    main()
