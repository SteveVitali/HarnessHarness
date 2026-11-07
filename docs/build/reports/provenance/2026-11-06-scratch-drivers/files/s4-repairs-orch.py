#!/usr/bin/env python3
"""S4-30SLOT-REPAIRS orchestrator — re-executes ONLY the 30 slots the
s4-completion-gce supplement left inconclusive, on the same GCE podman
backend, under the same frozen protocol (protocol_id 9edc5327, seed v3,
no re-pick). Operator authorized all three repairs verbatim
("let's do these", operator chat 2026-11-06).

Named supplement epoch: `s4-30slot-repairs-gce`. Every row carries
`prior_disposition` verbatim (the completion run's disposition) plus the
per-task repair-class member verbatim:

- swe-bench-astropy-2 ×10 — MACHINERY repair
  (`machinery_repair: declared_workdir`): the recorded proximate cause
  of `inconclusive{verifier_unavailable:no_parseable_verdict}` was OUR
  driver's `cd /app` hardcode — the task image declares
  `WORKDIR /app/astropy` (the repo root the verbatim run-tests.sh's
  relative paths resolve against; the parity H-side run resolves this
  task). Repair: s4-cell now execs the oracle solution + both verifier
  channels under the spec's `task_workdir`, read verbatim from
  `podman image inspect` `.Config.WorkingDir` of the built cell image
  (empty spec = the prior `/app` default, recorded verbatim). Same
  class as the ANSI fix — a driver defect repaired, never a task change.
- qemu-alpine-ssh ×10 — ENV admission (`env_repair: archive_mirror`):
  EOL bullseye dropped off deb.debian.org (the verbatim rc-100 deb404).
  The recorded admission builds a mirror-patched base
  (`localhost/s4-mirror/debian-bullseye-slim` = bullseye-slim +
  `sed 's|deb.debian.org/debian|archive.debian.org/debian|g;
   s|security.debian.org/debian-security|archive.debian.org/debian-security|g'`
  — every default bullseye suite resolves on archive.debian.org,
  verified verbatim) and builds the client from the task Dockerfile
  with ONLY the `FROM` line rewritten to the patched base — the
  verbatim rewritten Dockerfile is recorded in the build record; no
  task file changes.
- fix-git ×10 — ENV admission (`env_repair: vendored_mirror`):
  `github.com/TheMikeMerrill/personal-site` is deleted (rc 128; upstream
  terminal-bench head still clones it — no task-version admission
  exists). The honest mirror search found the Software Heritage
  archive's full snapshot `swh:1:snp:b2cbd5476b5a00d133f224e990498de4e06bad01`
  (origin visit 8, 2026-04-04T11:00:17Z, status full): the vault
  git-bare cook is vendored verbatim at
  `~/tb-work/vendored/fix-git/personal-site.git` (all branches; pinned
  commit `d7d3e4ba9350f634d92d39ace2b471433ee57d50` verified present in
  refs/heads/master history — `off the job market woo`). The recorded
  admission builds the client from the task Dockerfile with ONLY these
  added lines: `COPY vendor/personal-site.git /app/vendor/` +
  `git config --system url.insteadOf` redirecting the deleted clone URL
  to the archived mirror for the `RUN bash /app/setup.sh` leg only
  (config unset right after) — every task file byte-identical, the
  redirect recorded verbatim. NOT a task-semantics change: the clone
  materializes the same upstream objects.

All other machinery identical to s4-completion-orch.py (remote podman
via CONTAINER_HOST SSH forward, amd64 builds, R5.56 first-class
`--network bridge` helper + declared_deps admission, the R5.57
astropy verifier-bound admission retained verbatim, remote /work
mirror via tar-over-gcloud-ssh, sequential cells, same stop bounds).
A slot whose repair still can't genuinely run records
`inconclusive{reason}` verbatim — never a fabricated verdict.

Usage: python3 s4-repairs-orch.py [task_glob ...]
"""
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path.home() / ".local/share/uv/tools/terminal-bench/lib/python3.12/site-packages"))
import yaml  # noqa: E402
from terminal_bench.parsers.pytest_parser import PytestParser  # noqa: E402

DATASET = Path.home() / "tb-work/dataset"
WORK = Path.home() / "tb-work/s4-repairs"
SLOTS_DIR = WORK / "slots"
SPECS_DIR = WORK / "specs"
CELL = Path("/tmp/s4-cell/target/debug/s4-cell")
HELPER_STOCK = "/Users/stevenvitali/MetaHarness-harnessharness/target/debug/hh-helper"
MANIFEST = Path("/tmp/parity-manifest-v3.json")
PRIOR = Path("/Users/stevenvitali/MetaHarness-harnessharness/docs/build/reports/2026-11-06-s4-completion.json")
PARSER = PytestParser()
EPOCH = "s4-30slot-repairs-gce"
VENDOR = Path.home() / "tb-work/vendored"
ARCHIVE_MIRROR_TAG = "localhost/s4-mirror/debian-bullseye-slim:latest"
# Per-task repair-class member stamped verbatim on every slot row.
REPAIR = {
    "swe-bench-astropy-2": {
        "repair_class": "machinery_repair",
        "machinery_repair": "declared_workdir",
        "repair_note": (
            "the recorded proximate cause was OUR driver's `cd /app` "
            "hardcode — the task image declares WORKDIR /app/astropy; "
            "the parity H-side run resolves this task. s4-cell now execs "
            "solution/verifier under spec.task_workdir (verbatim from "
            "`podman image inspect` .Config.WorkingDir); empty spec = "
            "the prior /app default echoed verbatim. Same class as the "
            "ANSI fix — driver defect repaired, task untouched.")},
    "qemu-alpine-ssh": {
        "repair_class": "env_repair",
        "env_repair": "archive_mirror",
        "repair_note": (
            "EOL bullseye dropped off deb.debian.org (verbatim rc-100 "
            "deb404); every default bullseye suite resolves on "
            "archive.debian.org (verified). Client image built from the "
            "task Dockerfile with ONLY the FROM line rewritten to the "
            "mirror-patched base localhost/s4-mirror/debian-bullseye-slim "
            "(sed deb.debian.org->archive.debian.org) — the verbatim "
            "rewritten Dockerfile is in the build record; no task file "
            "changes.")},
    "fix-git": {
        "repair_class": "env_repair",
        "env_repair": "vendored_mirror",
        "repair_note": (
            "upstream deleted (rc 128; terminal-bench head still clones "
            "TheMikeMerrill/personal-site — no task-version admission). "
            "Mirror: Software Heritage git-bare snapshot "
            "swh:1:snp:b2cbd5476b5a00d133f224e990498de4e06bad01 (visit 8, "
            "2026-04-04T11:00:17Z, full), vendored at "
            "~/tb-work/vendored/fix-git/personal-site.git; pinned commit "
            "d7d3e4ba9350f634d92d39ace2b471433ee57d50 verified in "
            "refs/heads/master history. Client Dockerfile rewritten ONLY "
            "by added COPY vendor/ + git url.insteadOf lines for the "
            "setup.sh leg (config unset after) — all task files "
            "byte-identical.")},
}
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

ARCHIVE_SED = ("sed -i 's|deb.debian.org/debian|archive.debian.org/debian|g; "
               "s|security.debian.org/debian-security|archive.debian.org/debian-security|g' "
               "/etc/apt/sources.list")


def client_build_plan(task_id, cfg, rec):
    """The verbatim-recorded env-repair build inputs. Returns
    (ctx_dir, dockerfile_path). Default = the task's own ctx +
    Dockerfile byte-untouched. A repair NEVER edits the dataset — the
    rewritten Dockerfile lives under WORK and is recorded verbatim in
    the build record."""
    td_ctx = Path(cfg["ctx"])
    dfile = cfg["dockerfile"]
    orig = (td_ctx / dfile).read_text()
    if task_id == "qemu-alpine-ssh":
        # env_repair: archive_mirror — patch EOL bullseye's apt sources
        # to archive.debian.org on a derived base; rewrite ONLY the FROM.
        mdf = (f"FROM debian:bullseye-slim\n"
               f"RUN {ARCHIVE_SED} && cat /etc/apt/sources.list\n")
        mdir = WORK / "dockerfiles" / "s4-mirror-bullseye"
        mdir.mkdir(parents=True, exist_ok=True)
        (mdir / "Dockerfile").write_text(mdf)
        mb = sh(["podman", "build", "--platform", cfg["platform"] or "linux/amd64",
                 "-f", str(mdir / "Dockerfile"), "-t", ARCHIVE_MIRROR_TAG,
                 str(mdir)], timeout=3600)
        rec["env_repair"] = "archive_mirror"
        rec["mirror_base_build"] = {"tag": ARCHIVE_MIRROR_TAG, "rc": mb["rc"],
                                    "duration_sec": mb["duration_sec"],
                                    "stderr_tail": mb["stderr"][-400:],
                                    "stdout_tail": mb["stdout"][-400:],
                                    "dockerfile": mdf}
        if mb["rc"] != 0:
            return None, None
        rewritten = re.sub(r"(?m)^FROM\s+debian:bullseye-slim\s*$",
                           f"FROM {ARCHIVE_MIRROR_TAG}", orig)
        if rewritten == orig:
            rec["archive_mirror"] = {"rewrite_failed": "no FROM match"}
            return None, None
        rec["client_dockerfile_sha256_orig"] = hashlib.sha256(
            orig.encode()).hexdigest()
        rec["client_dockerfile_rewritten"] = rewritten
        ctx = WORK / "buildctx" / task_id
        ctx.mkdir(parents=True, exist_ok=True)
        (ctx / "Dockerfile").write_text(rewritten)
        return ctx, ctx / "Dockerfile"
    if task_id == "fix-git":
        # env_repair: vendored_mirror — the SWH git-bare snapshot is
        # vendored into the build ctx; a system git url.insteadOf
        # redirects the verbatim setup.sh clone to it for that leg only.
        vgit = VENDOR / "fix-git" / "personal-site.git"
        rec["env_repair"] = "vendored_mirror"
        rec["vendored_mirror"] = {
            "origin": "https://github.com/TheMikeMerrill/personal-site",
            "archive": ("Software Heritage swh:1:snp:"
                        "b2cbd5476b5a00d133f224e990498de4e06bad01 "
                        "(origin visit 8, 2026-04-04T11:00:17Z, "
                        "status full)"),
            "vault": ("git-bare (swh:1:snp cook) -> "
                      "~/tb-work/vendored/fix-git/personal-site.git"),
            "pinned_commit": "d7d3e4ba9350f634d92d39ace2b471433ee57d50",
            "master_tip": "19b5d101aa7083ae47a3a033ccadac79b0693488",
            "upstream_head_state": ("terminal-bench main still clones the "
                                    "deleted repo — no task-version "
                                    "admission exists"),
        }
        if not vgit.is_dir():
            rec["vendored_mirror"]["missing"] = True
            return None, None
        marker = "RUN bash /app/setup.sh"
        add = ("COPY vendor/personal-site.git /app/vendor/personal-site.git\n"
               "RUN git config --system "
               "url.\"file:///app/vendor/personal-site.git\".insteadOf "
               "\"https://github.com/TheMikeMerrill/personal-site.git\"\n")
        unset = ("\nRUN git config --system --unset "
                 "url.\"file:///app/vendor/personal-site.git\".insteadOf || true")
        rewritten = orig.replace(marker, add + marker + unset)
        if rewritten == orig:
            rec["vendored_mirror"]["rewrite_failed"] = "no marker match"
            return None, None
        rec["client_dockerfile_sha256_orig"] = hashlib.sha256(
            orig.encode()).hexdigest()
        rec["client_dockerfile_rewritten"] = rewritten
        ctx = WORK / "buildctx" / task_id
        if ctx.exists():
            shutil.rmtree(ctx)
        ctx.mkdir(parents=True)
        (ctx / "Dockerfile").write_text(rewritten)
        shutil.copy2(td_ctx / "setup.sh", ctx / "setup.sh")
        shutil.copytree(td_ctx / "resources", ctx / "resources")
        (ctx / "vendor").mkdir()
        shutil.copytree(vgit, ctx / "vendor" / "personal-site.git")
        return ctx, ctx / "Dockerfile"
    return td_ctx, td_ctx / dfile


def build_images(task_id, cfg, log):
    slug = ctr_name(task_id)
    client_tag = f"localhost/{slug}__client:latest"
    derived_tag = f"localhost/s4-{slug}:latest"
    plat = cfg["platform"] or "linux/amd64"
    rec = {"client_tag": client_tag, "derived_tag": derived_tag,
           "platform": plat, "bake_pip": cfg["bake_pip"],
           "bake_apt": cfg["bake_apt"], "backend": "gce:podman-s4-backend"}
    ctx, dfile = client_build_plan(task_id, cfg, rec)
    if ctx is None:
        rec["client_build"] = {"rc": None,
                               "note": "env_repair build input failed "
                                       "(verbatim in repair record)"}
        return None, rec, None
    b = sh(["podman", "build", "--platform", plat,
            "-f", str(dfile), "-t", client_tag, str(ctx)], timeout=14400)
    rec["client_build"] = {"rc": b["rc"], "duration_sec": b["duration_sec"],
                           "stderr_tail": b["stderr"][-400:],
                           "stdout_tail": b["stdout"][-400:]}
    if b["rc"] != 0:
        rec["derived_build"] = None
        return None, rec, None
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
        return None, rec, None
    insp = sh(["podman", "image", "inspect", derived_tag,
               "--format", "{{.Id}} {{.Digest}}"])
    rec["derived_image_id_digest"] = insp["stdout"].strip()
    # machinery repair (declared_workdir): the task image's declared
    # WORKDIR, read verbatim from the built cell image — the cwd the
    # oracle/verifier execs under. Empty inspect -> the prior /app
    # default, recorded verbatim.
    wd = sh(["podman", "image", "inspect", derived_tag,
             "--format", "{{.Config.WorkingDir}}"])
    rec["task_workdir_inspect"] = wd["stdout"].strip()
    task_workdir = rec["task_workdir_inspect"] or "/app"
    rec["task_workdir"] = task_workdir
    pr = sh(["podman", "run", "--rm", derived_tag, "sh", "-c",
             "python3 -c 'import pytest,sys; print(\"pytest\", pytest.__version__, sys.version.split()[0])'"],
            timeout=120)
    rec["probe_pytest"] = {"rc": pr["rc"], "out": pr["stdout"].strip() or pr["stderr"].strip()[-200:]}
    pn = sh(["podman", "run", "--rm", derived_tag, "sh", "-c",
             "echo net-check; ip route 2>/dev/null | head -2"], timeout=60)
    rec["probe_env"] = {"rc": pn["rc"], "out": pn["stdout"].strip()[:200]}
    return derived_tag, rec, task_workdir


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
               "repairs": {t: r["repair_class"] + ":" +
                           (r.get("env_repair") or r.get("machinery_repair") or "")
                           for t, r in REPAIR.items()},
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
        image, brec, task_workdir = build_images(task_id, cfg, tranche_log)
        (WORK / "logs" / f"{pos:02d}-{task_id.replace('/', '_')}-build.json").write_text(
            json.dumps({"task_id": task_id, "config": {k: v for k, v in cfg.items()
                                                      if k != "task_yaml"},
                        "task_yaml": cfg["task_yaml"],
                        "build": brec}, indent=2))
        # fix-git's deleted upstream is not a resource bound — the
        # verbatim rc128 is the record; the slots were never executed.
        upstream_deleted = (task_id == "fix-git" and image is None
                            and (brec.get("client_build") or {}).get("rc") == 128)
        repair = REPAIR.get(task_id, {})
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
                # the verbatim build record decides: rc 128 on the
                # clone = still upstream_deleted; mirror-missing/other =
                # the honest unbuildable class verbatim.
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
                rec.update(repair)
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
                # ── machinery repair: the declared WORKDIR verbatim ──
                "task_workdir": task_workdir,
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
            rec.update(repair)
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
