#!/usr/bin/env python3
"""H-side parity runner — direct podman execution reproducing tb's oracle
verdict pipeline on the SAME image tb built (tb__<task>__client).

Per task (single first attempt, no retries):
  1. Reproduce the docker-compose environment with podman:
     - multi-service tasks get a dedicated podman network; each
       non-client service runs with its env/command/hostname/volumes/
       network-alias; healthchecks are polled via `podman exec` of the
       declared test (service_healthy semantics) honoring depends_on
       start order; host port publishes are skipped (intra-network
       reachability is what the verifier uses) and recorded.
     - the client container runs `<image> sh -c 'sleep infinity'` with
       TEST_DIR=/tests plus any extra declared environment (e.g.
       API_URL) — the compose posture verbatim.
  2. solution.sh -> /oracle/solution.sh then `bash /oracle/solution.sh`
     (oracle agent verbatim); solution.yaml -> all committed commands
     piped into ONE persistent `podman exec -i ctr bash` (tmux keystroke
     semantics: sequential, shared shell state, interactive REPLs fed).
  3. tar-copy run-tests.sh -> /tests/run-tests.sh and tests/* -> /tests/
     (put_archive semantics: a directory contributes its contents).
  4. `bash /tests/run-tests.sh` with timeout = task.max_test_timeout_sec.
  5. Verdict via terminal_bench's own PytestParser — identical code path:
     resolved iff every parsed test is PASSED (xfail/skipped map to
     PASSED); missing summary/parse error/timeout -> unresolved.
  6. podman rm -f every container + drop the task network. Every
     artifact recorded verbatim to JSON.
"""
import json
import re
import shlex
import subprocess
import sys
import tarfile
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path.home() / ".local/share/uv/tools/terminal-bench/lib/python3.12/site-packages"))

import yaml  # noqa: E402
from terminal_bench.parsers.pytest_parser import PytestParser  # noqa: E402

DATASET = Path.home() / "tb-work/dataset"

# --- tranche-3 repair: ANSI capture-channel parity -----------------------
# Diagnosed defect (PARITY-TRANCHE-2 discordance, swe-bench-astropy-2):
# the O-side verdict input is a tmux `capture-pane` RENDER — the terminal
# emulator consumes ANSI escapes, so PytestParser sees bare `PASSED`
# tokens. The H-side fed the RAW `podman exec` byte stream
# (`\x1b[32mPASSED\x1b[0m …`) into the same parser, whose first-token
# status regex maps ANSI-wrapped tokens to UNKNOWN -> parsed={} ->
# unresolved. Reproduce capture-pane semantics UNCONDITIONALLY: strip
# CSI (incl. SGR), OSC, and remaining ESC sequences, and collapse
# carriage-return overwrites to the final rendered segment per line.
ANSI_OSC = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
ANSI_CSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")
ANSI_MISC = re.compile(r"\x1b(?:[()][0-2]|[=>#][0-9]?|[@-Z\\-_]|[\^_][^\x1b\x07]*(?:\x07|\x1b\\))")


def strip_ansi(s: str) -> str:
    """Terminal-render approximation: escapes consumed, CR overwrites
    collapse to the final segment of each line (tmux capture-pane
    semantics). Applied UNCONDITIONALLY to the verdict input."""
    s = ANSI_OSC.sub("", s)
    s = ANSI_CSI.sub("", s)
    s = ANSI_MISC.sub("", s)
    s = s.replace("\x1b", "")  # any residual lone ESC
    return "\n".join(line.split("\r")[-1] for line in s.split("\n"))

RUN_TAG = sys.argv[1] if len(sys.argv) > 1 else "run"
OUT_DIR = Path.home() / "tb-work/parity-runs/h-side" / RUN_TAG
OUT_DIR.mkdir(parents=True, exist_ok=True)

TASK_FILE = sys.argv[2] if len(sys.argv) > 2 else "/tmp/parity-taskids.txt"
TASKS = [t.strip() for t in open(TASK_FILE) if t.strip()]
PARSER = PytestParser()


def sh(args, timeout=None, input_bytes=None):
    """Run a host command; return dict with rc/stdout/stderr/duration."""
    t0 = time.time()
    try:
        p = subprocess.run(
            args, capture_output=True, timeout=timeout, input=input_bytes
        )
        return {
            "rc": p.returncode,
            "stdout": p.stdout.decode("utf-8", "replace"),
            "stderr": p.stderr.decode("utf-8", "replace"),
            "duration_sec": round(time.time() - t0, 2),
            "timeout": False,
        }
    except subprocess.TimeoutExpired as e:
        return {
            "rc": None,
            "stdout": (e.stdout or b"").decode("utf-8", "replace"),
            "stderr": (e.stderr or b"").decode("utf-8", "replace"),
            "duration_sec": round(time.time() - t0, 2),
            "timeout": True,
        }


def tar_bytes(file_members, content_dir=None):
    """Tar bytes matching tb's put_archive: files -> basename entries;
    a directory contributes its contents (rglob relative)."""
    buf = tempfile.TemporaryFile()
    with tarfile.open(fileobj=buf, mode="w") as tar:
        for p in file_members:
            tar.add(p, arcname=p.name)
        if content_dir is not None:
            for item in sorted(content_dir.rglob("*")):
                tar.add(item, arcname=item.relative_to(content_dir))
    buf.seek(0)
    return buf.read()


def ctr_name(task_id: str) -> str:
    return f"tb__{task_id.replace('.', '-')}"


def client_image_candidates(task_id: str) -> list[str]:
    base = f"{ctr_name(task_id)}__client:latest"
    return [f"docker.io/library/{base}", f"localhost/{base}"]


def client_image(task_id: str) -> str:
    """Resolve the client image: tb/docker-compose builds land under
    docker.io/library/; podman-built fallbacks under localhost/."""
    cands = client_image_candidates(task_id)
    for c in cands:
        if sh(["podman", "image", "exists", c])["rc"] == 0:
            return c
    return cands[0]  # absent everywhere — caller will build


def env_list(env):
    """compose 'environment' list form -> ['-e','K=V', ...]"""
    out = []
    for item in env or []:
        if isinstance(item, str):
            out += ["-e", item]
        elif isinstance(item, dict):
            for k, v in item.items():
                out += ["-e", f"{k}={v}"]
    if isinstance(env, dict):
        for k, v in env.items():
            out += ["-e", f"{k}={'' if v is None else v}"]
    return out


def wait_healthy(ctr, test_spec, interval, retries, log):
    """service_healthy semantics: poll the declared test via podman exec."""
    if isinstance(test_spec, list) and test_spec:
        kind = test_spec[0]
        if kind == "CMD-SHELL":
            argv = ["sh", "-c", test_spec[1]]
        elif kind == "CMD":
            argv = list(test_spec[1:])
        else:
            argv = ["sh", "-c", " ".join(map(str, test_spec))]
    elif isinstance(test_spec, str):
        argv = ["sh", "-c", test_spec]
    else:
        return True
    tries = max(1, int(retries or 5)) * 3  # generous margin, recorded
    for attempt in range(1, tries + 1):
        r = sh(["podman", "exec", ctr] + argv, timeout=30)
        log.append({"attempt": attempt, "rc": r["rc"],
                    "stderr_tail": r["stderr"][-300:]})
        if r["rc"] == 0:
            return True
        iv = interval or "5s"
        if isinstance(iv, str):
            iv = float(re.sub(r"[^0-9.].*$", "", iv) or 5)
        time.sleep(max(1.0, float(iv)))
    return False


def start_service(task_id, svc_name, spec, net, rec):
    """Run one compose service under podman; return container name."""
    slug = f"{ctr_name(task_id)}-{svc_name}-{RUN_TAG}"
    args = ["podman", "run", "-d", "--name", slug]
    if net:
        args += ["--network", net, "--network-alias", svc_name]
    if spec.get("hostname"):
        args += ["--hostname", spec["hostname"]]
    if spec.get("platform"):
        args += ["--platform", spec["platform"]]
    args += env_list(spec.get("environment"))
    for v in spec.get("volumes") or []:
        if isinstance(v, str) and ":" in v and "$" in v.split(":", 1)[0]:
            rec.setdefault("skipped_volumes", []).append(f"{svc_name}:{v}")
            continue  # host-side var path (logs) — skip, recorded
        args += ["-v", v]
    for p in spec.get("ports") or []:
        rec.setdefault("skipped_host_ports", []).append(
            f"{svc_name}:{p}")
    img = spec.get("image")
    b = spec.get("build")
    plat = spec.get("platform")  # tranche-2: honor compose platform
    if not img and b:
        ctx = b["context"] if isinstance(b, dict) else b
        df = b.get("dockerfile", "Dockerfile") if isinstance(b, dict) else "Dockerfile"
        ctx_dir = str(DATASET / task_id / ctx)
        img = f"localhost/hh-parity-{ctr_name(task_id)}-{svc_name}:latest"
        br = sh(["podman", "build",
                 "--platform", plat or "linux/arm64",
                 "-f", str(Path(ctx_dir) / df),
                 "-t", img, ctx_dir], timeout=14400)
        rec.setdefault("sidecar_builds", {})[svc_name] = br
        if br["rc"] != 0:
            return None, br
    elif img and sh(["podman", "image", "exists", img])["rc"] != 0:
        pr = sh(["podman", "pull"] + (["--platform", plat] if plat else [])
                + [img], timeout=1800)
        rec.setdefault("sidecar_pulls", {})[svc_name] = pr
        if pr["rc"] != 0:
            return None, pr
    args.append(img)
    cmd = spec.get("command")
    if isinstance(cmd, list):
        args += [str(c) for c in cmd]
    elif isinstance(cmd, str):
        args += shlex.split(cmd)
    up = sh(args, timeout=300)
    rec.setdefault("sidecars", {})[svc_name] = {"up": up, "name": slug}
    if up["rc"] != 0:
        return None, up
    hc = spec.get("healthcheck") or {}
    if hc.get("test"):
        rec["sidecars"][svc_name]["healthcheck_log"] = hl = []
        ok = wait_healthy(
            slug, hc["test"], hc.get("interval", "5s"),
            hc.get("retries", 5), hl)
        rec["sidecars"][svc_name]["healthy"] = ok
        if not ok:
            return None, {"rc": 1, "stderr": "healthcheck never passed"}
    return slug, None


def topo_order(services):
    """depends_on-aware start order (deps first)."""
    order, done = [], set()
    pending = dict(services)
    while pending:
        progressed = False
        for name, spec in list(pending.items()):
            deps = spec.get("depends_on") or {}
            dep_names = deps.keys() if isinstance(deps, dict) else deps
            if all(d in done or d not in services for d in dep_names):
                order.append(name)
                done.add(name)
                del pending[name]
                progressed = True
        if not progressed:
            order += list(pending)  # cycle — record verbatim order
            break
    return order


def run_task(task_id: str) -> dict:
    rec = {
        "task_id": task_id,
        "runner": "h-podman-exec",
        "started_at": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
    }
    task_dir = DATASET / task_id
    cfg = yaml.safe_load((task_dir / "task.yaml").read_text()) or {}
    test_timeout = float(cfg.get("max_test_timeout_sec", 60.0))
    rec["max_test_timeout_sec"] = test_timeout
    img = client_image(task_id)
    rec["image"] = img
    client_ctr = f"{ctr_name(task_id)}-client-{RUN_TAG}"
    rec["container"] = client_ctr
    compose = yaml.safe_load((task_dir / "docker-compose.yaml").read_text()) or {}
    services = compose.get("services") or {}
    client_spec = services.get("client") or {}
    net = f"hh-parity-net-{ctr_name(task_id)}" if len(services) > 1 else None
    rec["compose_services"] = list(services)
    rec["network"] = net
    spawned = [client_ctr]
    if net:
        sh(["podman", "network", "create", net])
    try:
        # 1. client image: reuse the O-side build; fallback one build.
        exists = sh(["podman", "image", "exists", img])
        rec["image_preexisting"] = exists["rc"] == 0
        if not rec["image_preexisting"]:
            # podman tags unqualified builds under localhost/ — build
            # there and run the resolved localhost/ name verbatim.
            img = f"localhost/{ctr_name(task_id)}__client:latest"
            rec["image"] = img
            bld = client_spec.get("build") or {}
            ctx = bld.get("context", ".") if isinstance(bld, dict) else bld
            df = bld.get("dockerfile", "Dockerfile") if isinstance(bld, dict) else "Dockerfile"
            ctx_dir = str(task_dir / (ctx or "."))
            # tranche-2: pin platform — compose-declared if present,
            # else arm64; kills the amd64-base tag-leak tranche 1 hit.
            cplat = client_spec.get("platform") or "linux/arm64"
            rec["build_platform"] = cplat
            b = sh(["podman", "build", "--platform", cplat,
                    "-f", str(Path(ctx_dir) / df),
                    "-t", f"{ctr_name(task_id)}__client:latest",
                    ctx_dir], timeout=14400)
            rec["build"] = b
            if b["rc"] != 0:
                rec["verdict"] = "unresolved"
                rec["verdict_reason"] = "build_failure"
                return rec
        insp = sh(["podman", "image", "inspect", img,
                   "--format", "{{.Id}} {{.Digest}}"])
        rec["image_id_digest"] = insp["stdout"].strip()

        # 2. sidecars first, in depends_on order (client last).
        for svc in topo_order(services):
            if svc == "client":
                continue
            name, err = start_service(task_id, svc, services[svc], net, rec)
            if name:
                spawned.append(name)
            if err is not None:
                rec["verdict"] = "unresolved"
                rec["verdict_reason"] = f"sidecar_failure:{svc}"
                return rec

        # 3. client container (compose posture: sleep infinity + env).
        args = ["podman", "run", "-d", "--name", client_ctr]
        if net:
            args += ["--network", net, "--network-alias", "client"]
        client_env = dict()  # TEST_DIR plus declared extras
        for item in client_spec.get("environment") or []:
            if isinstance(item, str) and "=" in item:
                k, v = item.split("=", 1)
                client_env[k] = v
        client_env["TEST_DIR"] = "/tests"
        for k, v in client_env.items():
            if "${" in v:
                v = re.sub(r"\$\{T_BENCH_TEST_DIR\}", "/tests", v)
            args += ["-e", f"{k}={v}"]
        args += [img, "sh", "-c", "sleep infinity"]
        up = sh(args, timeout=120)
        rec["container_up"] = up
        if up["rc"] != 0:
            rec["verdict"] = "unresolved"
            rec["verdict_reason"] = "container_start_failure"
            return rec
        sh(["podman", "exec", client_ctr, "mkdir", "-p", "/oracle", "/tests"])

        # 4. agent: run the committed solution.
        sol_sh = task_dir / "solution.sh"
        sol_yaml = task_dir / "solution.yaml"
        if sol_sh.exists():
            rec["solution_kind"] = "sh"
            blob = tar_bytes([sol_sh])
            cp = sh(["podman", "exec", "-i", client_ctr, "tar", "-C",
                     "/oracle", "-xf", "-"], input_bytes=blob)
            rec["solution_copy_rc"] = cp["rc"]
            sol = sh(["podman", "exec", client_ctr, "bash",
                      "/oracle/solution.sh"], timeout=3600)
            rec["solution_exec"] = sol
        elif sol_yaml.exists():
            rec["solution_kind"] = "yaml"
            cmds = yaml.safe_load(sol_yaml.read_text())
            rec["solution_command_count"] = len(cmds)
            stream = ""
            for c in cmds:
                stream += c.get("command", "")
                if c.get("append_enter", True):
                    stream += "\n"
            sol = sh(["podman", "exec", "-i", client_ctr, "bash"],
                     timeout=3600, input_bytes=stream.encode())
            rec["solution_exec"] = sol
        else:
            rec["verdict"] = "unresolved"
            rec["verdict_reason"] = "no_solution_file"
            return rec

        # 5. stage tests: run-tests.sh file + tests/ dir contents -> /tests
        blob = tar_bytes([task_dir / "run-tests.sh"])
        cp1 = sh(["podman", "exec", "-i", client_ctr, "tar", "-C", "/tests",
                  "-xf", "-"], input_bytes=blob)
        rec["run_tests_copy_rc"] = cp1["rc"]
        tdir = task_dir / "tests"
        if tdir.exists():
            blob = tar_bytes([], tdir)
            cp2 = sh(["podman", "exec", "-i", client_ctr, "tar", "-C",
                      "/tests", "-xf", "-"], input_bytes=blob)
            rec["tests_copy_rc"] = cp2["rc"]

        # 6. verifier.
        tv = sh(["podman", "exec", client_ctr, "bash",
                 "/tests/run-tests.sh"], timeout=test_timeout)
        rec["test_exec"] = tv

        # 7. verdict — tb's own parser, identical code path.
        # tranche-3: the O-side parser input is a tmux capture-pane
        # render (ANSI consumed); reproduce that channel by stripping
        # ANSI unconditionally before parsing. Raw bytes kept verbatim.
        raw_pane = tv["stdout"] + tv["stderr"]
        pane = strip_ansi(raw_pane)
        rec["verdict_input_raw_len"] = len(raw_pane)
        rec["verdict_input_stripped_len"] = len(pane)
        rec["ansi_stripped"] = raw_pane != pane
        if tv["timeout"]:
            rec["verdict"] = "unresolved"
            rec["verdict_reason"] = "test_timeout"
            rec["parsed"] = None
            return rec
        try:
            results = PARSER.parse(pane)
            rec["parsed"] = {k: v.value for k, v in results.items()}
            rec["verdict"] = (
                "resolved"
                if results and all(v.value == "passed"
                                   for v in results.values())
                else "unresolved"
            )
            rec["verdict_reason"] = (
                "all_tests_passed" if rec["verdict"] == "resolved"
                else "some_tests_failed"
            )
        except Exception as e:  # noqa: BLE001 — mirror tb parse failure
            rec["parsed"] = None
            rec["verdict"] = "unresolved"
            rec["verdict_reason"] = f"parse_error:{e}"
        return rec
    finally:
        for c in spawned:
            sh(["podman", "rm", "-f", c])
        if net:
            sh(["podman", "network", "rm", "-f", net])
        # disk hygiene: release images this side built/pulled itself
        # (preexisting tb images are kept — matched-image provenance).
        if rec.get("build") and rec["build"].get("rc") == 0:
            sh(["podman", "rmi", "-f",
                f"{ctr_name(task_id)}__client:latest"])
        for svc, br in (rec.get("sidecar_builds") or {}).items():
            if br.get("rc") == 0:
                sh(["podman", "rmi", "-f",
                    f"localhost/hh-parity-{ctr_name(task_id)}-{svc}:latest"])
        for svc, pr in (rec.get("sidecar_pulls") or {}).items():
            if pr.get("rc") == 0:
                spec = (compose.get("services") or {}).get(svc) or {}
                if spec.get("image"):
                    sh(["podman", "rmi", "-f", spec["image"]])
        rec["ended_at"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")


def main():
    results = []
    for i, t in enumerate(TASKS):
        out = OUT_DIR / f"{t}.h-side.json"
        if out.exists():
            print(f"[{i+1:2d}/{len(TASKS)}] {t} — recorded, skipped",
                  flush=True)
            results.append(json.loads(out.read_text()))
            continue
        print(f"[{i+1:2d}/{len(TASKS)}] {t}", flush=True)
        try:
            rec = run_task(t)
        except Exception as e:  # noqa: BLE001 — record verbatim, continue
            rec = {"task_id": t, "runner": "h-podman-exec",
                   "verdict": "unresolved",
                   "verdict_reason": f"runner_exception:{e}"}
        (OUT_DIR / f"{t}.h-side.json").write_text(json.dumps(rec, indent=2))
        print(f"      -> {rec.get('verdict')} ({rec.get('verdict_reason')})",
              flush=True)
        results.append(rec)
    (OUT_DIR / "h-side-all.json").write_text(json.dumps(results, indent=2))
    print(json.dumps(
        {t["task_id"]: t.get("verdict") for t in results}, indent=2))


if __name__ == "__main__":
    main()
