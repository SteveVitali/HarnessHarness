#!/usr/bin/env python3
"""S0.3b — E2 kernel-slice spike (throwaway, ADR-0050 R1-R6; l1-spike-spec s3).

Implemented from the l1-spike-spec s3.1 scope ALONE (append-only ledger, idp/1-class canonical
form + per-run hash chain `hash = H(leaf_tag || canonical(envelope - hash) || prev_hash)`, 64 KiB
blob offload) in the E2 ecosystem, with its OWN canonical serializer + SHA-256 chain -- no code is
shared with the E1 kernel. The shared corpus (same bytes for every candidate) is read from
`shared-corpus.json`; gate G1 asserts this candidate reproduces the E1 reference head hash
byte-for-byte (canonical form is implementation-independent -- ADR-0029 property 5).

Subcommands:
  g1 <corpus> <reference>            -> assert cross-candidate hash-chain byte-identity; print head
  startup-probe <corpus>             -> minimal: build ONE event's chain link, exit (M-S1-1 probe)
  measure <corpus> <n_large> <cores> -> emit M-S1-* key/value lines (C5/C7 inputs)

CC4: this file names no ecosystem in any committed record; the candidate<->toolchain binding lives
only in the build ADR. Output is data on the sheet; the code is committed-but-throwaway.
"""
import hashlib
import json
import os
import sys
import threading
import time

LEAF_TAG = "hh-event\x00"
OFFLOAD_THRESHOLD = 64 * 1024


# --- canonical form (JCS-class: sorted keys, compact, integers only) -------------------------
def _esc(s):
    out = ['"']
    for ch in s:
        o = ord(ch)
        if ch == '"':
            out.append('\\"')
        elif ch == '\\':
            out.append('\\\\')
        elif ch == '\n':
            out.append('\\n')
        elif ch == '\r':
            out.append('\\r')
        elif ch == '\t':
            out.append('\\t')
        elif o < 0x20:
            out.append('\\u%04x' % o)
        else:
            out.append(ch)
    out.append('"')
    return ''.join(out)


def canonical(v):
    if v is None:
        return "null"
    if v is True:
        return "true"
    if v is False:
        return "false"
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        return _esc(v)
    if isinstance(v, list):
        return "[" + ",".join(canonical(x) for x in v) + "]"
    if isinstance(v, dict):
        items = sorted(v.items(), key=lambda kv: kv[0])
        return "{" + ",".join(_esc(k) + ":" + canonical(val) for k, val in items) + "}"
    raise TypeError("non-canonical value: %r" % (v,))


def sha256_hex(data_bytes):
    return hashlib.sha256(data_bytes).hexdigest()


def envelope_canonical_without_hash(env):
    """Canonical form of the envelope minus `hash` (the hash-chain preimage body)."""
    m = {
        "run_id": env["run_id"],
        "event_id": env["event_id"],
        "seq": env["seq"],
        "parent_event_id": env["parent_event_id"],
        "lease_generation": env["lease_generation"],
        "kind": env["kind"],
        "cost": env["cost"],
    }
    if env.get("blob") is not None:
        m["blob"] = env["blob"]
    else:
        m["payload"] = env["payload"]
    return canonical(m)


def build_chain(corpus, run_id, check_cancel=None):
    """Per-run hash chain over the shared corpus. Returns (head_hash, cost_total, n_done)."""
    prev = "genesis"
    total = 0
    parent = None
    n = 0
    for i, spec in enumerate(corpus):
        if check_cancel is not None and check_cancel():
            break
        payload = spec["payload"]
        canon = canonical(payload)
        if len(canon.encode("utf-8")) >= OFFLOAD_THRESHOLD:
            blob = "sha256:" + sha256_hex(canon.encode("utf-8"))
            env = dict(payload=None, blob=blob)
        else:
            env = dict(payload=payload, blob=None)
        env.update(
            run_id=run_id,
            event_id=i + 1,
            seq=i,
            parent_event_id=parent,
            lease_generation=1,
            kind=spec["kind"],
            cost=spec["cost"],
        )
        body = envelope_canonical_without_hash(env)
        prev = sha256_hex((LEAF_TAG + body + prev).encode("utf-8"))
        total += spec["cost"]
        parent = i + 1
        n += 1
    return prev, total, n


def build_ledger(corpus, run_id):
    """Build a retained ledger: the list of committed envelopes for one idle session (M-S1-2)."""
    prev = "genesis"
    parent = None
    out = []
    for i, spec in enumerate(corpus):
        payload = spec["payload"]
        canon = canonical(payload)
        if len(canon.encode("utf-8")) >= OFFLOAD_THRESHOLD:
            env = dict(payload=None, blob="sha256:" + sha256_hex(canon.encode("utf-8")))
        else:
            env = dict(payload=payload, blob=None)
        env.update(run_id=run_id, event_id=i + 1, seq=i, parent_event_id=parent,
                   lease_generation=1, kind=spec["kind"], cost=spec["cost"])
        env["hash"] = sha256_hex((LEAF_TAG + envelope_canonical_without_hash(env) + prev).encode("utf-8"))
        prev = env["hash"]
        parent = i + 1
        out.append(env)
    return out


def load_corpus(path):
    with open(path, "rb") as f:
        raw = f.read()
    return json.loads(raw.decode("utf-8")), sha256_hex(raw)


# --- gate G1: cross-candidate byte-identity --------------------------------------------------
def cmd_g1(corpus_path, ref_path):
    corpus, corpus_sha = load_corpus(corpus_path)
    with open(ref_path) as f:
        ref = json.load(f)
    head, total, n = build_chain(corpus, ref["run_id"])
    ok_corpus = corpus_sha == ref["corpus_sha256"]
    ok_head = head == ref["head_hash"]
    ok_cost = total == ref["cost_total"]
    print("candidate E2")
    print("g1_corpus_sha256 %s" % corpus_sha)
    print("g1_corpus_sha_matches_reference %s" % ("true" if ok_corpus else "false"))
    print("g1_head_hash %s" % head)
    print("g1_head_matches_reference %s" % ("true" if ok_head else "false"))
    print("g1_cost_total %d" % total)
    print("g1_cost_matches_reference %s" % ("true" if ok_cost else "false"))
    print("g1_event_count %d" % n)
    ok = ok_corpus and ok_head and ok_cost and n == ref["event_count"]
    print("g1_byte_identical_across_candidates %s" % ("true" if ok else "false"))
    return 0 if ok else 1


# --- M-S1-1 startup probe --------------------------------------------------------------------
def cmd_startup_probe(corpus_path):
    corpus, _ = load_corpus(corpus_path)
    # minimal: build the first ledger event's chain link and make it "visible"
    _ = build_chain(corpus[:1], "startup")
    return 0


# --- S1 measurements (C5/C7 inputs) ----------------------------------------------------------
def _fanout_threads(corpus, run_id_prefix, n):
    """Run n concurrent sessions as OS threads (matched to E1 std::thread), each building the
    in-memory chain. Returns wall micros. GIL bounds CPU parallelism -- that IS the C5 property
    ADR-0050 scored for this candidate; measured, not assumed."""
    results = [None] * n
    threads = []
    t0 = time.perf_counter()
    for i in range(n):
        def work(idx=i):
            results[idx] = build_chain(corpus, "%s-%d" % (run_id_prefix, idx))
        th = threading.Thread(target=work)
        threads.append(th)
    for th in threads:
        th.start()
    for th in threads:
        th.join()
    wall = (time.perf_counter() - t0) * 1e6
    return wall


def cmd_measure(corpus_path, n_large, cores):
    corpus, _ = load_corpus(corpus_path)
    events_per_session = len(corpus)

    # M-S1-5 CPU/concurrency fan-out (in-memory chain build, no I/O), core-normalized with the
    # SAME formula as E1 (ratio / (N / min(cores, N))). Warm once, then measure.
    _ = _fanout_threads(corpus, "warm", 1)
    cpu_n1 = _fanout_threads(corpus, "cpu1", 1)
    cpu_n50 = _fanout_threads(corpus, "cpu50", n_large)
    ratio = cpu_n50 / max(cpu_n1, 1.0)
    s_over_p = n_large / min(cores, n_large)
    core_normalized = ratio / s_over_p
    achieved = (cpu_n1 * n_large) / max(cpu_n50, 1.0)  # effective concurrent sessions
    print("m_s1_5_cpu_fanout_ratio_wall %.3f" % ratio)
    print("m_s1_5_cpu_fanout_core_normalized %.3f" % core_normalized)
    print("m_s1_5_achieved_parallelism %.2f" % achieved)
    print("m_s1_5_wall_n1_us %d" % int(cpu_n1))
    print("m_s1_5_wall_n50_us %d" % int(cpu_n50))

    # M-S1-3 throughput at N=50 (events/s aggregate) from the fan-out wall.
    eps = (n_large * events_per_session) / (cpu_n50 / 1e6)
    print("m_s1_3_append_throughput_eps %.0f" % eps)

    # M-S1-4 project() rebuild + incremental (cost-totals fold over the chain).
    t0 = time.perf_counter()
    total = sum(s["cost"] for s in corpus)
    rebuild_us = (time.perf_counter() - t0) * 1e6
    t0 = time.perf_counter()
    acc = 0
    for s in corpus:
        acc += s["cost"]
    incr_us = ((time.perf_counter() - t0) * 1e6) / len(corpus)
    print("m_s1_4_project_rebuild_ms %.4f" % (rebuild_us / 1000.0))
    print("m_s1_4_project_incremental_us %.3f" % incr_us)
    _ = total

    # M-S1-8 cancellation: cancel flag -> all N=50 sessions stop between events.
    cancel = threading.Event()
    stopped = [0]
    lock = threading.Lock()

    def cancellable(idx):
        build_chain(corpus, "cx-%d" % idx, check_cancel=cancel.is_set)
        with lock:
            stopped[0] += 1

    threads = [threading.Thread(target=cancellable, args=(i,)) for i in range(n_large)]
    for th in threads:
        th.start()
    time.sleep(0.001)
    t0 = time.perf_counter()
    cancel.set()
    for th in threads:
        th.join()
    cancel_us = (time.perf_counter() - t0) * 1e6
    print("m_s1_8_cancel_ms %.3f" % (cancel_us / 1000.0))

    # M-S1-9 streaming tail lag: subscribe-delivery latency at N=50 (append -> observed).
    lags = []
    q = []
    qlock = threading.Lock()

    def producer(idx):
        for k in range(events_per_session):
            with qlock:
                q.append(time.perf_counter())
    prods = [threading.Thread(target=producer, args=(i,)) for i in range(n_large)]
    for th in prods:
        th.start()
    seen = 0
    target = n_large * events_per_session
    while seen < target:
        with qlock:
            while q:
                emitted = q.pop(0)
                lags.append((time.perf_counter() - emitted) * 1e6)
                seen += 1
    for th in prods:
        th.join()
    lags.sort()
    med = lags[len(lags) // 2]
    p95 = lags[int(len(lags) * 0.95)]
    print("m_s1_9_tail_lag_median_us %d" % int(med))
    print("m_s1_9_tail_lag_p95_us %d" % int(p95))

    return 0


def cmd_footprint(corpus_path, n_large):
    """M-S1-2 in a CLEAN process (no fan-out contamination). Resident memory per idle session at
    N=50, matched to E1 (one process holding N sessions, RSS/N): build N independent session
    ledgers (each retains its 200-event envelope list), hold them resident, read RSS delta."""
    corpus, _ = load_corpus(corpus_path)
    rss0 = _rss_kib()
    ledgers = [build_ledger(corpus, "idle-%d" % i) for i in range(n_large)]
    rss1 = _rss_kib()
    held = sum(len(x) for x in ledgers)
    per_session_mib = (rss1 - rss0) / 1024.0 / n_large
    print("m_s1_2_rss_total_mib %.1f" % (rss1 / 1024.0))
    print("m_s1_2_rss_delta_mib %.1f" % ((rss1 - rss0) / 1024.0))
    print("m_s1_2_rss_per_session_mib %.3f" % per_session_mib)
    if held != n_large * len(corpus):
        raise RuntimeError("ledger retention failed")
    return 0


def _rss_kib():
    try:
        import resource
        r = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        # macOS reports bytes; Linux reports kilobytes.
        return r // 1024 if sys.platform == "darwin" else r
    except Exception:
        return 0


def main():
    if len(sys.argv) < 2:
        print("usage: e2_kernel.py <g1|startup-probe|measure> ...", file=sys.stderr)
        return 2
    cmd = sys.argv[1]
    if cmd == "g1":
        return cmd_g1(sys.argv[2], sys.argv[3])
    if cmd == "startup-probe":
        return cmd_startup_probe(sys.argv[2])
    if cmd == "measure":
        return cmd_measure(sys.argv[2], int(sys.argv[3]), int(sys.argv[4]))
    if cmd == "footprint":
        return cmd_footprint(sys.argv[2], int(sys.argv[3]))
    print("unknown command %r" % cmd, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
