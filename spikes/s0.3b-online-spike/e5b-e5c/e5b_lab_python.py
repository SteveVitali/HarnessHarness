#!/usr/bin/env python3
"""S0.3b — E5b LAB side (E2 ecosystem) driving the E3 kernel over the kernel<->lab boundary
(subprocess + newline-delimited JSON-RPC 2.0 over stdio). Matched E3<->E3 for the transport cost
(CC9): per-run crossing overhead = (split wall driving the E3 kernel) - (the SAME E3 kernel run
natively), /N. Also verifies far-side (E2) hash-equality of every event (M-S2-7): the lab
re-canonicalises the chain with its OWN serializer and every event hash must match the kernel's.

Emits key/value lines for the E5b C12 row. No network egress, no model spend."""
import json
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
E2 = os.path.join(HERE, "..", "e2-python")
sys.path.insert(0, E2)
import e2_kernel  # noqa: E402  (its own canonical serializer + build_chain — the E2 lab side)

N = 50
KERNEL = os.path.join(HERE, "e5b_kernel_node.mjs")


def load_corpus(path):
    with open(path, "rb") as f:
        return json.loads(f.read().decode("utf-8"))


def native_wall_us(corpus_path):
    out = subprocess.run(
        ["node", KERNEL, "native", corpus_path, str(N)],
        capture_output=True, text=True, check=True,
    ).stdout
    for line in out.splitlines():
        if line.startswith("native_build_wall_us"):
            return int(line.split()[1])
    raise RuntimeError("no native wall")


def verify_far_side(corpus, run_id, kernel_events):
    """Far-side (E2) hash-equality: recompute the chain locally and compare every event hash."""
    prev = "genesis"
    parent = None
    matched = 0
    for i, spec in enumerate(corpus):
        canon = e2_kernel.canonical(spec["payload"])
        if len(canon.encode("utf-8")) >= e2_kernel.OFFLOAD_THRESHOLD:
            env = dict(payload=None, blob="sha256:" + e2_kernel.sha256_hex(canon.encode("utf-8")))
        else:
            env = dict(payload=spec["payload"], blob=None)
        env.update(run_id=run_id, event_id=i + 1, seq=i, parent_event_id=parent,
                   lease_generation=1, kind=spec["kind"], cost=spec["cost"])
        h = e2_kernel.sha256_hex((e2_kernel.LEAF_TAG + e2_kernel.envelope_canonical_without_hash(env) + prev).encode("utf-8"))
        if kernel_events[i]["seq"] == i and kernel_events[i]["hash"] == h:
            matched += 1
        prev = h
        parent = i + 1
    return matched


def main():
    corpus_path = sys.argv[1]
    corpus = load_corpus(corpus_path)

    # native baseline (E3 kernel, no boundary)
    native_us = native_wall_us(corpus_path)

    # split: E2 lab drives the E3 kernel over stdio JSON-RPC
    proc = subprocess.Popen(
        ["node", KERNEL, "serve", corpus_path],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1,
    )

    def call(req):
        proc.stdin.write(json.dumps(req) + "\n")
        proc.stdin.flush()
        return json.loads(proc.stdout.readline())

    hello = call({"jsonrpc": "2.0", "id": 0, "method": "hello", "params": {}})
    assert hello["result"]["kernel_candidate"] == "E3", "handshake"

    # warm
    call({"jsonrpc": "2.0", "id": -1, "method": "run_session", "params": {"run_id": "warm"}})

    # TIMED: transport only (send request, receive + parse per-run result) — NO verification compute
    # in the timed section, so M-S2-1 isolates the crossing cost (matched E3<->E3).
    results = []
    t0 = time.perf_counter()
    for i in range(N):
        run_id = "run-%d" % i
        resp = call({"jsonrpc": "2.0", "id": i + 1, "method": "run_session", "params": {"run_id": run_id}})
        results.append((run_id, resp["result"]))
    split_us = (time.perf_counter() - t0) * 1e6

    # UNTIMED: far-side (E2) hash-equality verification of every event (M-S2-7).
    total_events = 0
    matched_events = 0
    for run_id, res in results:
        matched_events += verify_far_side(corpus, run_id, res["events"])
        total_events += len(res["events"])

    proc.stdin.close()
    proc.wait(timeout=5)

    per_run_overhead_us = (split_us - native_us) / N
    # representative single-run wall for the % figure (native build / N).
    native_per_run_us = native_us / N
    pct = 100.0 * (per_run_overhead_us / native_per_run_us) if native_per_run_us else 0.0
    hash_equal_pct = 100.0 * matched_events / total_events if total_events else 0.0

    print("split E5b")
    print("m_s2_1_split_wall_us %d" % int(split_us))
    print("m_s2_1_native_wall_us %d" % native_us)
    print("m_s2_1_per_run_overhead_us %.1f" % per_run_overhead_us)
    print("m_s2_1_per_run_overhead_pct %.3f" % pct)
    print("m_s2_7_hash_equal_pct %.1f" % hash_equal_pct)
    print("m_s2_7_events_verified %d" % total_events)
    print("e5b_far_side_candidate E2")
    print("e5b_kernel_candidate E3")


if __name__ == "__main__":
    main()
