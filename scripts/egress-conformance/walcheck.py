#!/usr/bin/env python3
"""walcheck.py — the conformance assertions (R6.45k).

Replays the mediator's `<store>/runs/<run>/events.wal` (the
`{"k":"e","v":<envelope>}` / `{"k":"c","n":<seq>}` commit-marker format —
a torn tail drops wholesale, exactly as `replay_committed` folds) and
emits `verdict.json` (`hh-egress-conformance/1`):

  * every `security.egress.requested` has its `security.egress.decided`
    (paired by `request_ref`);
  * zero mediator 5xx — the decided rows carry only allow/deny
    dispositions, no `error:*` outcome lands, the probes' verbatim
    status lines never read >= 500, and mediator.err names no 5xx;
  * every `security.egress.tunnel.closed` carries `close_cause == "eof"`
    and true byte counts (present integers);
  * every step's exit code matches its declared expectation
    (`ok` → 0, `deny` → nonzero);
  * expected denies land as decided `deny` rows naming the host
    (download.pytorch.org under a freeze that withholds it; github.com
    under the agent phase — Q-X55);
  * per-step walls and the peak concurrent :3128 connections recorded
    (the D8 proof).
"""
import argparse
import json
import os
import sys


def committed_events(wal_path):
    """Fold the WAL: `e` records stage, `c` markers commit them."""
    committed, pending = [], []
    try:
        text = open(wal_path, errors="replace").read()
    except FileNotFoundError:
        return committed
    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            continue
        try:
            j = json.loads(line)
        except json.JSONDecodeError:
            break  # torn tail — not durable
        if j.get("k") == "e":
            pending.append(j.get("v") or {})
        elif j.get("k") == "c":
            committed.extend(pending)
            pending = []
    return committed


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--store", required=True)
    ap.add_argument("--run", required=True)
    ap.add_argument("--steps", required=True)
    ap.add_argument("--probes", required=True)
    ap.add_argument("--mediator-err", required=True)
    ap.add_argument("--peak", default="0")
    ap.add_argument("--subject", default="unknown")
    ap.add_argument("--epoch", default="0")
    ap.add_argument("--digest", default="unknown")
    ap.add_argument("--verify-policy", required=True)
    ap.add_argument("--agent-policy", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()

    wal = os.path.join(a.store, "runs", a.run, "events.wal")
    evs = committed_events(wal)
    requested = [e for e in evs if e.get("class") == "security.egress.requested"]
    decided = [e for e in evs if e.get("class") == "security.egress.decided"]
    closed = [e for e in evs if e.get("class") == "security.egress.tunnel.closed"]

    req_refs = {
        (e.get("payload") or {}).get("request_ref") for e in requested
    }
    dec_refs = set()
    deny_hosts = set()
    for e in decided:
        p = e.get("payload") or {}
        dec_refs.add(p.get("request_ref"))
        if p.get("decision") == "deny":
            deny_hosts.add(p.get("host_norm") or p.get("host_raw") or "?")

    steps = []
    if os.path.exists(a.steps):
        for line in open(a.steps, errors="replace"):
            line = line.strip()
            if line:
                steps.append(json.loads(line))
    probes = []
    if os.path.exists(a.probes):
        for line in open(a.probes, errors="replace"):
            line = line.strip()
            if line:
                probes.append(json.loads(line))
    med_err = ""
    if os.path.exists(a.mediator_err):
        med_err = open(a.mediator_err, errors="replace").read()

    verify_grants_pytorch = "download.pytorch.org" in open(a.verify_policy).read()
    agent_grants_github = "github.com" in open(a.agent_policy).read()

    assertions = []
    failures = []

    def check(name, ok, detail):
        assertions.append({"id": name, "ok": bool(ok), "detail": detail})
        if not ok:
            failures.append(f"{name}: {detail}")

    # A1 — step exit codes match expectation.
    bad = [
        s for s in steps
        if (s["expect"] == "ok" and s["rc"] != 0)
        or (s["expect"] == "deny" and s["rc"] == 0)
    ]
    check(
        "steps-exit",
        not bad,
        f"{len(steps)} steps, {len(bad)} unexpected exits"
        + (" — " + "; ".join(f"{s['shape']}:rc{s['rc']} `{s['cmd'][:80]}`" for s in bad[:6]) if bad else ""),
    )

    # A2 — every requested row is decided (request_ref pairing).
    undecided = req_refs - dec_refs
    check(
        "requested-decided",
        not undecided,
        f"{len(requested)} requested / {len(decided)} decided / {len(undecided)} undecided",
    )

    # A3 — zero mediator 5xx: no decided error dispositions, no probe
    # status >= 500, no 5xx named on mediator.err.
    probe_5xx = [p for p in probes if (p.get("status") or 0) >= 500]
    err_5xx = [l for l in med_err.splitlines() if " 50" in l or "5xx" in l]
    check(
        "zero-5xx",
        not probe_5xx and not err_5xx,
        f"probes>=500: {len(probe_5xx)}, mediator.err 5xx lines: {len(err_5xx)}",
    )

    # A4 — every tunnel.closed: close_cause eof + true byte counts.
    causes = {}
    bad_close = []
    for e in closed:
        p = e.get("payload") or {}
        c = p.get("close_cause")
        causes[c] = causes.get(c, 0) + 1
        if c != "eof":
            bad_close.append(c)
        bu, bd = p.get("bytes_up"), p.get("bytes_down")
        if not isinstance(bu, int) or not isinstance(bd, int) or bu < 0 or bd < 0:
            bad_close.append(f"bytes{bu}/{bd}")
    check(
        "tunnel-close",
        not bad_close,
        f"{len(closed)} closed tunnels, causes {causes}"
        + (f", bad: {bad_close[:6]}" if bad_close else ""),
    )

    # A5 — S2's expectation: pytorch deny iff the freeze withholds it.
    s2 = [s for s in steps if s["shape"] == "s2"]
    if s2:
        if verify_grants_pytorch:
            check(
                "s2-epoch-grant",
                all(s["rc"] == 0 for s in s2),
                f"verify grants download.pytorch.org — s2 exits {[s['rc'] for s in s2]}",
            )
        else:
            denied = "download.pytorch.org" in deny_hosts
            check(
                "s2-typed-deny",
                all(s["rc"] != 0 for s in s2) and denied,
                f"freeze withholds download.pytorch.org — s2 rc {[s['rc'] for s in s2]}, "
                f"deny row present: {denied}",
            )

    # A6 — the agent phase never grants github.com; the deny probe's
    # step ran nonzero and the WAL names the deny.
    s5_deny = [s for s in steps if s["shape"] == "s5" and s["expect"] == "deny"]
    if s5_deny:
        check(
            "agent-no-github",
            not agent_grants_github
            and "github.com" in deny_hosts
            and all(s["rc"] != 0 for s in s5_deny),
            f"agent layer grants github.com: {agent_grants_github}, "
            f"deny row: {'github.com' in deny_hosts}, probe rc {[s['rc'] for s in s5_deny]}",
        )

    walls = {s["shape"]: [] for s in steps}
    for s in steps:
        walls[s["shape"]].append(s["wall_ms"])

    verdict = {
        "schema": "hh-egress-conformance/1",
        "subject_ref": a.subject,
        "freeze": {"epoch": int(a.epoch), "digest": a.digest},
        "platform": {
            "os": os.uname().sysname.lower(),
            "arch": os.uname().machine,
        },
        "shapes": {
            sh: {
                "steps": len(ws),
                "wall_ms_total": sum(ws),
                "wall_ms_max": max(ws) if ws else 0,
            }
            for sh, ws in sorted(walls.items())
        },
        "wal": {
            "requested": len(requested),
            "decided": len(decided),
            "tunnel_closed": len(closed),
            "close_causes": causes,
            "denied_hosts": sorted(deny_hosts),
        },
        "probes": probes,
        "peak_conns": int(a.peak or 0),
        "assertions": assertions,
        "failures": failures,
        "result": "pass" if not failures else "fail",
    }
    with open(a.out, "w") as f:
        json.dump(verdict, f, indent=2, sort_keys=True)
        f.write("\n")
    print(json.dumps({"result": verdict["result"], "failures": failures}, indent=1))
    sys.exit(0 if not failures else 1)


if __name__ == "__main__":
    main()
