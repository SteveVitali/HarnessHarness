#!/usr/bin/env python3
"""S0.3b — driver: run every cross-candidate / MCP-ACP arm N>=3 times and emit the distribution
(median . min-max) as JSON + a human summary. Reproduces the extended measurement sheet's numbers.

Usage: python3 run-sheet.py [reps]   (default reps=5, matching S0.3)

Order: E1 reference (corpus + head) -> G1 byte-identity (E2/E3) -> M-S1-* measure + footprint +
startup (E2/E3) -> M-S1-7 MCP/ACP (E2/E3) -> E5b/E5c boundary splits. No network egress beyond the
one-time SDK install (already vendored under e2-python/.venv and e3-node/node_modules); no model spend.
"""
import json
import os
import statistics
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
CORPUS = os.path.join(HERE, "corpus", "shared-corpus.json")
REF = os.path.join(HERE, "corpus", "reference.json")
PYVENV = os.path.join(HERE, "e2-python", ".venv", "bin", "python")
N_LARGE = 50
CORES = os.cpu_count() or 12


def sh(cmd, cwd=None):
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, check=True).stdout


def kv(text):
    out = {}
    for line in text.splitlines():
        parts = line.split(None, 1)
        if len(parts) == 2:
            out[parts[0]] = parts[1]
    return out


def dist(vals):
    vals = [float(v) for v in vals]
    return {
        "median": statistics.median(vals),
        "min": min(vals),
        "max": max(vals),
        "n": len(vals),
    }


def collect(reps, runner, keys):
    acc = {k: [] for k in keys}
    for _ in range(reps):
        out = kv(runner())
        for k in keys:
            if k in out:
                acc[k].append(out[k])
    return {k: dist(v) for k, v in acc.items() if v}


def main():
    reps = int(sys.argv[1]) if len(sys.argv) > 1 else 5
    result = {"reps": reps, "cores": CORES, "n_large": N_LARGE}

    # 0. E1 reference
    e1 = os.path.join(HERE, "e1-ref", "target", "release", "s03b-e1-ref")
    if not os.path.exists(e1):
        subprocess.run(["cargo", "build", "--release"], cwd=os.path.join(HERE, "e1-ref"), check=True)
    sh([e1, CORPUS, REF])
    reference = json.load(open(REF))
    result["reference"] = reference

    # 1. G1 byte-identity (deterministic; run once each)
    g1 = {}
    for cand, cmd in (("E2", [PYVENV, os.path.join(HERE, "e2-python", "e2_kernel.py"), "g1", CORPUS, REF]),
                      ("E3", ["node", os.path.join(HERE, "e3-node", "e3_kernel.mjs"), "g1", CORPUS, REF])):
        g1[cand] = kv(sh(cmd))
    result["g1"] = g1

    m_keys = ["m_s1_5_cpu_fanout_core_normalized", "m_s1_5_cpu_fanout_ratio_wall",
              "m_s1_5_achieved_parallelism", "m_s1_3_append_throughput_eps",
              "m_s1_8_cancel_ms", "m_s1_9_tail_lag_median_us", "m_s1_9_tail_lag_p95_us"]

    def measure(kernel_cmd):
        return lambda: sh(kernel_cmd + ["measure", CORPUS, str(N_LARGE), str(CORES)])

    def footprint(kernel_cmd):
        return lambda: sh(kernel_cmd + ["footprint", CORPUS, str(N_LARGE)])

    def startup(kernel_cmd):
        def run():
            t0 = time.perf_counter()
            subprocess.run(kernel_cmd + ["startup-probe", CORPUS], check=True,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            return "m_s1_1_startup_ms %.3f" % ((time.perf_counter() - t0) * 1000.0)
        return run

    kernels = {
        "E2": [PYVENV, os.path.join(HERE, "e2-python", "e2_kernel.py")],
        "E3": ["node", os.path.join(HERE, "e3-node", "e3_kernel.mjs")],
    }
    result["s1"] = {}
    for cand, kcmd in kernels.items():
        d = collect(reps, measure(kcmd), m_keys)
        d.update(collect(reps, footprint(kcmd), ["m_s1_2_rss_per_session_mib", "m_s1_2_rss_delta_mib"]))
        d.update(collect(reps, startup(kcmd), ["m_s1_1_startup_ms"]))
        result["s1"][cand] = d

    # 2. M-S1-7 MCP + ACP official SDKs
    mcp = {
        "E2": lambda: sh([PYVENV, os.path.join(HERE, "e2-python", "mcp_roundtrip.py")]),
        "E3": lambda: sh(["node", os.path.join(HERE, "e3-node", "e3_mcp_roundtrip.mjs")]),
    }
    acp = {
        "E2": lambda: sh([PYVENV, os.path.join(HERE, "e2-python", "acp_roundtrip.py")]),
        "E3": lambda: sh(["node", os.path.join(HERE, "e3-node", "e3_acp_roundtrip.mjs")]),
    }
    result["m_s1_7"] = {}
    for cand in ("E2", "E3"):
        d = collect(reps, mcp[cand], ["m_s1_7_mcp_setup_ms", "m_s1_7_mcp_call_median_ms", "m_s1_7_mcp_call_p95_ms"])
        d.update(collect(reps, acp[cand], ["m_s1_7_acp_setup_ms", "m_s1_7_acp_prompt_ms"]))
        # verification flags (last run)
        d["mcp_echo_verified"] = kv(mcp[cand]()).get("m_s1_7_mcp_echo_verified")
        d["acp_session_verified"] = kv(acp[cand]()).get("m_s1_7_acp_session_verified")
        result["m_s1_7"][cand] = d

    # 3. E5b / E5c boundary splits
    e5b = lambda: sh(["python3", os.path.join(HERE, "e5b-e5c", "e5b_lab_python.py"), CORPUS])
    e5c = lambda: sh(["node", os.path.join(HERE, "e5b-e5c", "e5c_surface_node.mjs"), CORPUS])
    result["e5"] = {}
    for name, runner in (("E5b", e5b), ("E5c", e5c)):
        d = collect(reps, runner, ["m_s2_1_per_run_overhead_us", "m_s2_1_per_run_overhead_pct", "m_s2_7_hash_equal_pct"])
        d["m_s2_7_events_verified"] = kv(runner()).get("m_s2_7_events_verified")
        result["e5"][name] = d

    out_path = os.path.join(HERE, "distributions.json")
    json.dump(result, open(out_path, "w"), indent=2)
    print(json.dumps(result, indent=2))
    print("\nwrote %s" % out_path, file=sys.stderr)


if __name__ == "__main__":
    main()
