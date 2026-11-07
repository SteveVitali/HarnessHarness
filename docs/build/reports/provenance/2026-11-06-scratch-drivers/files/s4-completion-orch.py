#!/usr/bin/env python3
"""S4-COMPLETION orchestrator — re-executes ONLY the 50 slots the egress
supplement left inconclusive, on the GCE podman backend, under the same
frozen protocol (protocol_id 9edc5327, seed v3, no re-pick).

Named supplement epoch: `s4-completion-gce`. Every row carries
`prior_disposition` verbatim (the egress run's disposition).

Backend deltas vs s4-egress-orch.py (recorded, never silent):
- `podman` CLI targets the remote rootful podman 4.3.1 via
  CONTAINER_HOST=unix:///tmp/podman-gce/podman.sock (the recorded SSH
  forward); remote disk hygiene via `gcloud compute ssh` df -BG /.
- Images build linux/amd64 natively (the backend's arch; the prior
  local backend built linux/arm64 under applehv).
- The helper is the stock repo hh-helper carrying the R5.56 --network
  flag (first-class variant — the scratch /tmp/s4-helper-egress is
  superseded). The cell spec's declared_deps/policy_net_mode ride the
  R5.56 admission; the ten astropy specs carry the R5.57
  verifier_admission members (360 -> 900, verbatim from
  hh-verifier-bound-admission/1).
- `remote_host` on each spec tells s4-cell to mirror the staged /work
  tree to the byte-identical path on the VM via tar-over-gcloud-ssh.
- fix-git's client build is attempted once; the verbatim rc128
  (deleted upstream) records all ten slots
  `inconclusive{upstream_deleted}` — never executed, never fabricated.

Usage: python3 s4-completion-orch.py [task_glob ...]
"""
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path.home() / ".local/share/uv/tools/terminal-bench/lib/python3.12/site-packages"))
import yaml  # noqa: E402
from terminal_bench.parsers.pytest_parser import PytestParser  # noqa: E402

DATASET = Path.home() / "tb-work/dataset"
WORK = Path.home() / "tb-work/s4-completion"
SLOTS_DIR = WORK / "slots"
SPECS_DIR = WORK / "specs"
CELL = Path("/tmp/s4-cell/target/debug/s4-cell")
HELPER_STOCK = "/Users/stevenvitali/MetaHarness-harnessharness/target/debug/hh-helper"
MANIFEST = Path("/tmp/parity-manifest-v3.json")
PRIOR = Path("/Users/stevenvitali/MetaHarness-harnessharness/docs/build/reports/2026-11-05-s4-egress-cohort.json")
PARSER = PytestParser()
EPOCH = "s4-completion-gce"
REMOTE = {"remote_host": "podman-s4-backend",
          "remote_project": "harnessharness-evidence-8d7a99",
          "remote_zone": "us-central1-a"}
GCE_SSH = ["gcloud", "compute", "ssh", REMOTE["remote_host"],
           f"--project={REMOTE['remote_project']}",
           f"--zone={REMOTE['remote_zone']}", "--quiet"]
ANSI_OSC = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
ANSI_CSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
ANSI_MISC = re.compile(r"\x1b(?:[()][0-2]|[=>#][0-9]?|[@-Z\\-_]|[\^_][^\x1b\x07]*(?:\x07|\x1b\\))")

os.environ["CONTAINER_HOST"] = "unix:///tmp/podman-gce/podman.sock"


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
    """Remote VM / free GiB via `gcloud compute ssh` df (the backend's
    boot disk — the prior local recipe's podman machine is not used)."""
    r = sh(GCE_SSH + ["--command=df -BG / | tail -1"], timeout=60)
    out = r["stdout"] + r["stderr"]
    m = re.findall(r"(\d+)G\s+\d+%\s+/", out)
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
        sh(["podman", "builder", "prune", "-af"], timeout=600)
    return free


# ── per-task config derivation (verbatim from s4-egress-orch.py) ────

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
    # bounded by (same signature table as the egress supplement,
    # verbatim).
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
        "declared_deps_list": sorted(k for k, v in dep_sigs.items() if v),
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


# The R5.57 admission's verbatim members (hh-verifier-bound-admission/1;
# ADR-0422) — stamped on each astropy spec by s4-cell's
# `admit_verifier_bound` call.
ASTROPY_VBOUND = {
    "task_id": "swe-bench-astropy-2",
    "enforced_before_sec": 360,
    "admitted_sec": 900,
    "basis": "inconclusive{verifier_unavailable:no_parseable_verdict} ×10 on "
             "the egress supplement (hh-s4-egress-supplement/1 — the ten "
             "swe-bench-astropy-2 slots; verbatim rc 127 + "
             "no_pytest_summary No module named pytest)",
    "prior_disposition": "inconclusive{verifier_unavailable:no_parseable_verdict}",
    "supplement_epoch": EPOCH,
    "authorization": "docs/build/reports/2026-11-06-r5.57-verifier-bound-admission.json "
                     "(hh-verifier-bound-admission/1; ADR-0422; ticket "
                     "docs/tickets/248_R5.57__verifier-bound.md)",
    "scope": "this task's verifier leg only — the tc-verdict (verbatim "
             "run-tests.sh) + tc-vendored dispatches of swe-bench-astropy-2's "
             "ten slots. No other task's bound moves; no other bound moves "
             "(max_agent_timeout_sec, staging/work bounds, the deadline "
             "ladder) — the admission touches machinery only, never the "
             "frozen manifest/protocol.",
}


# ── image build + derive (verbatim pipeline; remote podman, amd64) ──

def build_images(task_id, cfg, log):
    slug = ctr_name(task_id)
    client_tag = f"localhost/{slug}__client:latest"
    derived_tag = f"localhost/s4-{slug}:latest"
    plat = cfg["platform"] or "linux/amd64"
    rec = {"client_tag": client_tag, "derived_tag": derived_tag,
           "platform": plat, "bake_pip": cfg["bake_pip"],
           "bake_apt": cfg["bake_apt"], "backend": "gce:podman-s4-backend"}
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
    globs = sys.argv[1:]
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
               "supplement_epoch": EPOCH,
               "egress_profile": "declared_deps",
               "backend": "gce:podman-s4-backend (e2-standard-8, 8vCPU/32GiB/200GB, debian-12, podman 4.3.1 rootful)",
               "helper": "stock hh-helper --network (R5.56 first-class)"}

    for pos, sel in enumerate(selection):
        task_id = sel["task_id"]
        if globs and not any(g in task_id for g in globs):
            continue
        tdir = DATASET / task_id
        # supplement domain: only this task's prior-INCONCLUSIVE slots
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
        # fix-git's deleted upstream is not a resource bound — the
        # verbatim rc128 is the record; the slots were never executed.
        upstream_deleted = (task_id == "fix-git" and image is None
                            and (brec.get("client_build") or {}).get("rc") == 128)
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
                disp = ("inconclusive{upstream_deleted}" if upstream_deleted
                        else "inconclusive{image_unavailable:client_build_failed}")
                rec = {"schema": "hh-s4-slot/1", "slot_ref": s["slot_ref"],
                       "task_id": task_id, "arm_id": s["arm_id"],
                       "strategy": strategy, "round": s["round"],
                       "first": s["first"], "lane_id": s["lane_id"],
                       "cell_attempted": False,
                       "attempts": [], "calls": 0,
                       "prior_disposition": prior_d,
                       "supplement_epoch": EPOCH,
                       "egress_profile": "declared_deps" if needs_egress else "none",
                       "cell_network": "bridge" if needs_egress else "none",
                       "policy_net_mode": "public" if needs_egress else "none",
                       "backend": "gce:podman-s4-backend",
                       "client_build_rc": (brec.get("client_build") or {}).get("rc"),
                       "disposition": disp}
                out_file.write_text(json.dumps(rec, indent=2))
                print(f"    slot {slug} -> {disp}", flush=True)
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
                "declared_deps": cfg["declared_deps_list"],
                "helper_bin": HELPER_STOCK,
                "policy_net_mode": "public" if needs_egress else "none",
                # ── the GCE backend ──
                **REMOTE,
            }
            if task_id == "swe-bench-astropy-2":
                spec["verifier_admission"] = ASTROPY_VBOUND
            vc = vendored_cmd(task_id, cfg)
            if vc:
                spec["vendored_cmd"] = vc
            if cfg["multi_service"]:
                spec["multi_service"] = True
            rec = run_slot(spec, task_id, cfg)
            rec["prior_disposition"] = prior_d
            rec["supplement_epoch"] = EPOCH
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
                # workload outcome, not cell isolation.
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
