#!/usr/bin/env python3
"""Assemble the S4 egress supplement packet.

Reads ~/tb-work/s4-egress/slots/*.json + the frozen manifest + the prior
confirmation packet and emits
docs/build/reports/2026-11-05-s4-egress-cohort.json — the named
egress-profile supplement over exactly the 140 slots the frozen run
recorded inconclusive. Per-slot verdicts, egress_profile verbatim,
denominators, costs. Nothing fabricated: absent slots count as
unattempted; the original inconclusive records stay untouched
(`prior_disposition` carried verbatim on every supplement row)."""
import json
import sys
from pathlib import Path

WORK = Path.home() / "tb-work/s4-egress"
SLOTS = WORK / "slots"
REPO = Path("/Users/stevenvitali/MetaHarness-harnessharness")
OUT = REPO / "docs/build/reports/2026-11-05-s4-egress-cohort.json"
MANIFEST = Path("/tmp/parity-manifest-v3.json")
PRIOR = REPO / "docs/build/reports/2026-11-05-s4-confirmation.json"


def main():
    manifest = json.loads(MANIFEST.read_text())
    prior = json.loads(PRIOR.read_text())
    prior_disp = {s["slot_ref"]: s["disposition"] for s in prior["slots"]}
    # the supplement domain: exactly the prior run's inconclusive slots
    supp_slots = [s for s in manifest["schedule"]
                  if str(prior_disp.get(s["slot_ref"], "")).startswith("inconclusive")]
    records = {}
    for f in sorted(SLOTS.glob("*.json")):
        r = json.loads(f.read_text())
        records[r["slot_ref"]] = r

    per_slot = []
    counts = {"planned": 0, "attempted": 0, "executed": 0,
              "inconclusive": 0, "failed": 0, "unattempted": 0,
              "receipt_resolved": 0, "receipt_unresolved": 0,
              "egress_admitted": 0, "egress_none": 0}
    first = {"planned": 0, "executed": 0, "resolved": 0}
    costs = {"attempts": 0, "dispatches": 0, "wall_ms": 0,
             "output_bytes": 0, "tokens_freed": 0,
             "exposure_micros": 0, "usd": 0.0}
    fidelity = {"opportunities": 0, "starts": 0, "completions": 0,
                "changed_requests": 0, "triggered_first": 0,
                "served_deviations": 0}
    per_task = {}
    inconclusive_reasons = {}

    for s in supp_slots:
        ref = s["slot_ref"]
        counts["planned"] += 1
        if s["first"]:
            first["planned"] += 1
        r = records.get(ref)
        row = {"slot_ref": ref, "task_id": s["task_id"],
               "arm_id": s["arm_id"], "round": s["round"],
               "first": s["first"], "lane_id": s["lane_id"],
               "prior_disposition": prior_disp[ref],
               "supplement_epoch": "egress-profile-rerun"}
        tk = per_task.setdefault(s["task_id"], {"slots": 0, "executed": 0,
                                                "inconclusive": 0, "failed": 0,
                                                "resolved": 0,
                                                "unattempted": 0})
        tk["slots"] += 1
        if r is None:
            row["disposition"] = "unattempted"
            counts["unattempted"] += 1
            tk["unattempted"] += 1
            per_slot.append(row)
            continue
        disp = r.get("disposition", "unattempted")
        row["disposition"] = disp
        counts["attempted"] += 1
        for k in ("cell_attempted", "attempts", "calls", "setup_retries",
                  "executor_backend", "staging_dispatch", "work_dispatch",
                  "work_exit", "solution_log_bytes", "solution_log_sha256",
                  "egress_evidence", "egress_profile", "helper_bin",
                  "cell_network", "policy_net_mode",
                  "pressure_opportunity",
                  "compaction_status", "served_variant", "tokens_freed",
                  "forgotten_count", "request_changed", "next_dispatch_after",
                  "verdict_verbatim_dispatch", "verdict_vendored_dispatch",
                  "verdict_verbatim.rc", "verdict.rc", "seal", "probe",
                  "sentinel_intact", "observer_survived", "intervals",
                  "wall_ms", "input_tokens", "output_bytes",
                  "wall_cap_breach", "failure", "receipt",
                  "disposition_recomputed"):
            if k in r:
                row[k] = r[k]
        if row.get("egress_profile") == "declared_deps":
            counts["egress_admitted"] += 1
        else:
            counts["egress_none"] += 1
        if disp == "executed":
            counts["executed"] += 1
            tk["executed"] += 1
            v = (r.get("receipt") or {}).get("verdict")
            if v == "resolved":
                counts["receipt_resolved"] += 1
                tk["resolved"] += 1
                if s["first"]:
                    first["resolved"] += 1
            elif v == "unresolved":
                counts["receipt_unresolved"] += 1
            if s["first"]:
                first["executed"] += 1
        elif disp.startswith("failed"):
            counts["failed"] += 1
            tk["failed"] += 1
        else:
            counts["inconclusive"] += 1
            tk["inconclusive"] += 1
            inconclusive_reasons[disp] = inconclusive_reasons.get(disp, 0) + 1
        costs["attempts"] += len(r.get("attempts", []))
        costs["dispatches"] += r.get("calls", 0) or 0
        costs["wall_ms"] += r.get("wall_ms", 0) or 0
        costs["output_bytes"] += r.get("output_bytes", 0) or 0
        costs["tokens_freed"] += r.get("tokens_freed", 0) or 0
        if r.get("pressure_opportunity"):
            fidelity["opportunities"] += 1
        if r.get("compaction_started"):
            fidelity["starts"] += 1
        if r.get("compaction_completed"):
            fidelity["completions"] += 1
        if r.get("request_changed"):
            fidelity["changed_requests"] += 1
        if r.get("triggered") and s["first"]:
            fidelity["triggered_first"] += 1
        if r.get("served_variant") and (
                ("evict" in (r.get("served_variant") or ""))
                != (r.get("strategy") == "evict_oldest")):
            fidelity["served_deviations"] += 1
        per_slot.append(row)

    dispositions = {"executed": counts["executed"],
                    "inconclusive": counts["inconclusive"],
                    "failed": counts["failed"],
                    "unattempted": counts["unattempted"],
                    "inconclusive_reasons": inconclusive_reasons}
    packet = {
        "schema": "hh-s4-egress-supplement/1",
        "generated": __import__("time").strftime("%Y-%m-%dT%H:%M:%SZ", __import__("time").gmtime()),
        "harness": "devin-desktop/swe-2-high/subagent",
        "cost_usd": 0.0,
        "protocol_id": manifest["protocol_id"],
        "phase": "confirmation-egress-supplement",
        "supplement_of": "docs/build/reports/2026-11-05-s4-confirmation.json (hh-s4-confirmation/1 — untouched)",
        "supplement_epoch": "egress-profile-rerun",
        "admission": {
            "egress_profile": "declared_deps",
            "mechanism": "scoped helper variant /tmp/s4-helper-egress — crates/hh-helper/src/bin/hh_helper.rs verbatim plus the `network` knob (hharness-helper server/protocol and PodmanBackend unchanged); per-slot cells create with `podman --network bridge` (outbound NAT via the VM's slirp4netns) instead of `--network none`; the stock hh-helper binary and crate source are untouched",
            "policy": "net.mode=public for admitted cells — the N4-honest declaration (kernel authority + a declared residual_channels[host_bridge] entry); fs/proc boundaries unchanged and still probed",
            "scope": "only slots whose tasks declare runtime egress deps (mechanical scan of solution.* + run-tests.sh: apt/pip/uv bootstrap/git clone/curl|wget/HF/remote RPC); none stays for all others — in this supplement every rerun task declared deps",
            "selector": "spec.egress_profile + spec.helper_bin per slot; recorded verbatim in every slot row",
            "first_attempt_semantics": "these are named supplement attempts on the same frozen slots (same slot_ref/arm/lane/round; the frozen manifest is consumed verbatim, never re-picked). `first` still marks the manifest's round-0 subjects; `supplement_epoch` + `prior_disposition` mark the second-attempt fact verbatim — the original inconclusive records stand",
        },
        "declared_inputs": {
            "manifest": "frozen central-protocol/1, seed parity-tranche-2026-11-05:seed:v3 (same manifest, verbatim)",
            "participant": "per-task committed oracle solution — $0, no model calls",
            "subject_machinery": "same HH machinery (EnvDriver/Dispatcher/compact/seal on hh-helper podman cells); the egress delta is the scoped helper variant only",
            "receipt": "task's own verifier — run-tests.sh verbatim channel (now genuinely runnable: its uv/apt/pip bootstrap has egress), vendored_offline fallback unchanged; terminal-bench PytestParser",
        },
        "manifest": {"protocol_id": manifest["protocol_id"]},
        "counts": counts,
        "first_subject_denominators": first,
        "fidelity_denominators": fidelity,
        "costs": costs,
        "per_task": per_task,
        "dispositions": dispositions,
        "slots": per_slot,
    }
    packet["verdict"] = (
        "partial" if counts["executed"] and counts["executed"] < counts["planned"]
        else ("executed" if counts["executed"] == counts["planned"]
              else "inconclusive"))
    packet["verdict_basis"] = (
        f"egress supplement over {counts['planned']} prior-inconclusive slots: "
        f"{counts['executed']} executed with a real receipt "
        f"({counts['receipt_resolved']} resolved / {counts['receipt_unresolved']} unresolved); "
        f"{counts['inconclusive']} still inconclusive, {counts['failed']} failed, "
        f"{counts['unattempted']} unattempted; "
        f"{first['executed']}/{first['planned']} supplement first-subject slots executed "
        f"({first['resolved']} resolved); "
        f"{counts['egress_admitted']} slots admitted egress_profile=declared_deps, "
        f"{counts['egress_none']} kept none. Denominators recomputed over the supplement verbatim.")
    OUT.write_text(json.dumps(packet, indent=2))
    print(json.dumps({"wrote": str(OUT), "counts": counts,
                      "first": first, "fidelity": fidelity,
                      "inconclusive_reasons": inconclusive_reasons,
                      "verdict": packet["verdict"]}, indent=2))


if __name__ == "__main__":
    main()
