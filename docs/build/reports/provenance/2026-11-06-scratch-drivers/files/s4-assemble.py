#!/usr/bin/env python3
"""Assemble the S4 confirmation packet from the per-slot records.

Reads ~/tb-work/s4-confirm/slots/*.json + the frozen manifest and emits
docs/build/reports/2026-11-05-s4-confirmation.json — per-slot verdicts,
denominators (all slots / all attempts / first-subject), costs,
dispositions. Nothing fabricated: absent slots count as unattempted."""
import json
import sys
from pathlib import Path

WORK = Path.home() / "tb-work/s4-confirm"
SLOTS = WORK / "slots"
REPO = Path("/Users/stevenvitali/MetaHarness-harnessharness")
OUT = REPO / "docs/build/reports/2026-11-05-s4-confirmation.json"
MANIFEST = Path("/tmp/parity-manifest-v3.json")


def main():
    manifest = json.loads(MANIFEST.read_text())
    slots = manifest["schedule"]
    records = {}
    for f in sorted(SLOTS.glob("*.json")):
        r = json.loads(f.read_text())
        records[r["slot_ref"]] = r

    per_slot = []
    counts = {"planned": 0, "attempted": 0, "executed": 0,
              "inconclusive": 0, "failed": 0, "unattempted": 0,
              "receipt_resolved": 0, "receipt_unresolved": 0,
              "egress_bound": 0}
    first = {"planned": 0, "executed": 0, "resolved": 0}
    costs = {"attempts": 0, "dispatches": 0, "wall_ms": 0,
             "output_bytes": 0, "tokens_freed": 0,
             "exposure_micros": 0, "usd": 0.0}
    fidelity = {"opportunities": 0, "starts": 0, "completions": 0,
                "changed_requests": 0, "triggered_first": 0,
                "served_deviations": 0}
    per_task = {}

    for s in slots:
        ref = s["slot_ref"]
        counts["planned"] += 1
        if s["first"]:
            first["planned"] += 1
        r = records.get(ref)
        row = {"slot_ref": ref, "task_id": s["task_id"],
               "arm_id": s["arm_id"], "round": s["round"],
               "first": s["first"], "lane_id": s["lane_id"]}
        tk = per_task.setdefault(s["task_id"], {"slots": 0, "executed": 0,
                                                "inconclusive": 0, "failed": 0,
                                                "resolved": 0})
        tk["slots"] += 1
        if r is None:
            row["disposition"] = "unattempted"
            counts["unattempted"] += 1
            per_slot.append(row)
            continue
        disp = r.get("disposition", "unattempted")
        row["disposition"] = disp
        counts["attempted"] += 1
        for k in ("cell_attempted", "attempts", "calls", "setup_retries",
                  "executor_backend", "staging_dispatch", "work_dispatch",
                  "work_exit", "solution_log_bytes", "solution_log_sha256",
                  "egress_evidence", "pressure_opportunity",
                  "compaction_status", "served_variant", "tokens_freed",
                  "forgotten_count", "request_changed", "next_dispatch_after",
                  "verdict_verbatim_dispatch", "verdict_vendored_dispatch",
                  "verdict_verbatim.rc", "verdict.rc", "seal", "probe",
                  "sentinel_intact", "observer_survived", "intervals",
                  "wall_ms", "input_tokens", "output_bytes",
                  "wall_cap_breach", "failure", "receipt"):
            if k in r:
                row[k] = r[k]
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
        if r.get("egress_evidence"):
            counts["egress_bound"] += 1
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
                    "unattempted": counts["unattempted"]}
    packet = {
        "schema": "hh-s4-confirmation/1",
        "generated": __import__("time").strftime("%Y-%m-%dT%H:%M:%SZ", __import__("time").gmtime()),
        "harness": "devin-desktop/swe-2-high/subagent",
        "cost_usd": 0.0,
        "protocol_id": manifest["protocol_id"],
        "phase": "confirmation",
        "unblocking_fact": "parity tranche 3 verified 30/30 (S3 met) — docs/build/reports/2026-11-05-parity-tranche-3.json",
        "declared_inputs": {
            "manifest": "frozen central-protocol/1, seed parity-tranche-2026-11-05:seed:v3 (re-materialized, never re-picked)",
            "arms": {"arm:o-tb-oracle": "evict_oldest",
                     "arm:h-podman-oracle": "clear_tool_results"},
            "participant": "per-task committed oracle solution (solution.sh | solution.yaml stream) — $0, no model calls",
            "subject_machinery": "hh-env EnvDriver/Dispatcher/compact/seal on podman cells (--network none, hh-helper)",
            "receipt": "task's own verifier — run-tests.sh verbatim channel where it could run, else the derived image's pytest on the task's own test file (vendored_offline, bootstrap bypass recorded); terminal-bench PytestParser",
            "machinery_bounds": {
                "cell_network": "none — solutions/verifiers needing live egress cannot genuinely run inside the cell",
                "derived_base_images": "per-task FROM <client> + the packages the task's own scripts install (recorded verbatim in the slot/build records)",
                "runtime_downloads_not_prestaged": True,
            },
        },
        "manifest": manifest,
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
        f"{counts['executed']}/{counts['planned']} slots executed with a real receipt "
        f"({counts['receipt_resolved']} resolved / {counts['receipt_unresolved']} unresolved); "
        f"{counts['inconclusive']} inconclusive, {counts['failed']} failed, "
        f"{counts['unattempted']} unattempted; "
        f"{first['executed']}/{first['planned']} first-subject slots executed "
        f"({first['resolved']} resolved). Denominators retained verbatim.")
    OUT.write_text(json.dumps(packet, indent=2))
    print(json.dumps({"wrote": str(OUT), "counts": counts,
                      "first": first, "verdict": packet["verdict"]}, indent=2))


if __name__ == "__main__":
    main()
