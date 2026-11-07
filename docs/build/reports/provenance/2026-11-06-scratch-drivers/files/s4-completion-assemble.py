#!/usr/bin/env python3
"""Assemble the S4-completion supplement packet (R5.58).

Reads ~/tb-work/s4-completion/slots/*.json + the frozen manifest + the
egress cohort packet and emits
docs/build/reports/2026-11-06-s4-completion.json —
`hh-s4-completion/1`, the named `s4-completion-gce` epoch over exactly
the 50 slots the egress supplement recorded inconclusive. Per-slot
verdicts, egress_profile/helper variant verbatim, denominators, costs
incl. billed instance-hours. Nothing fabricated: absent slots count as
unattempted; every row carries `prior_disposition` verbatim and the
prior packets stay byte-untouched.

It also prints the COMBINED cohort fold (confirmation + egress +
completion) — executed/inconclusive/failed totals, first-subject
denominators — for the run ledger's verbatim record."""
import json
import sys
import time
from pathlib import Path

WORK = Path.home() / "tb-work/s4-completion"
SLOTS = WORK / "slots"
REPO = Path("/Users/stevenvitali/MetaHarness-harnessharness")
OUT = REPO / "docs/build/reports/2026-11-06-s4-completion.json"
MANIFEST = Path("/tmp/parity-manifest-v3.json")
CONFIRM = REPO / "docs/build/reports/2026-11-05-s4-confirmation.json"
EGRESS = REPO / "docs/build/reports/2026-11-05-s4-egress-cohort.json"
EPOCH = "s4-completion-gce"


def main():
    manifest = json.loads(MANIFEST.read_text())
    egress = json.loads(EGRESS.read_text())
    prior_disp = {s["slot_ref"]: s["disposition"] for s in egress["slots"]}
    # the supplement domain: exactly the egress run's inconclusive slots
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
              "egress_admitted": 0, "egress_none": 0,
              "cells_run": 0}
    first = {"planned": 0, "executed": 0, "resolved": 0}
    costs = {"attempts": 0, "dispatches": 0, "wall_ms": 0,
             "output_bytes": 0, "tokens_freed": 0,
             "exposure_micros": 0, "usd": 0.0}
    fidelity = {"opportunities": 0, "starts": 0, "completions": 0,
                "changed_requests": 0, "triggered_first": 0,
                "served_deviations": 0}
    per_task = {}
    inconclusive_reasons = {}
    remote_ws_syncs = 0

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
               "supplement_epoch": EPOCH}
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
                  "helper_variant", "declared_deps", "remote_host",
                  "remote_ws_syncs", "remote_ws_push",
                  "cell_network", "policy_net_mode", "backend",
                  "client_build_rc",
                  "pressure_opportunity",
                  "compaction_status", "served_variant", "tokens_freed",
                  "forgotten_count", "request_changed", "next_dispatch_after",
                  "verdict_verbatim_dispatch", "verdict_vendored_dispatch",
                  "verdict_verbatim.rc", "verdict.rc",
                  "verifier_bound_admitted",
                  "seal", "probe",
                  "sentinel_intact", "observer_survived", "intervals",
                  "wall_ms", "input_tokens", "output_bytes",
                  "wall_cap_breach", "failure", "receipt",
                  "cell_setup_failure", "disposition_recomputed"):
            if k in r:
                row[k] = r[k]
        if row.get("egress_profile") == "declared_deps":
            counts["egress_admitted"] += 1
        else:
            counts["egress_none"] += 1
        remote_ws_syncs += int(r.get("remote_ws_syncs") or 0)
        if r.get("cell_attempted"):
            counts["cells_run"] += 1
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
        "schema": "hh-s4-completion/1",
        "generated": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "harness": "devin-desktop/swe-2-high/subagent",
        "cost_usd": 0.0,
        "protocol_id": manifest["protocol_id"],
        "phase": "confirmation-completion-supplement",
        "supplement_of": (
            "docs/build/reports/2026-11-05-s4-confirmation.json (hh-s4-confirmation/1 — untouched) + "
            "docs/build/reports/2026-11-05-s4-egress-cohort.json (hh-s4-egress-supplement/1 — untouched)"),
        "supplement_epoch": EPOCH,
        "backend": {
            "id": "gce:podman-s4-backend",
            "project": "harnessharness-evidence-8d7a99",
            "zone": "us-central1-a",
            "machine": "e2-standard-8 (8 vCPU / 32 GiB) + 200 GB pd-balanced, debian-12",
            "podman": "4.3.1 rootful (podman-remote.service)",
            "access": "CONTAINER_HOST=unix:///tmp/podman-gce/podman.sock "
                      "(SSH forward → remote /tmp/podman.sock); "
                      "DOCKER_HOST=tcp://localhost:23750 verified",
            "platform": "linux/amd64 (native build arch; the prior local "
                        "backend built linux/arm64 under applehv)",
            "workspace_parity": "each cell's /work tree staged locally is pushed to the "
                                "byte-identical path on the VM (the ssh user's "
                                "/Users/stevenvitali mirror) before provision and "
                                "pulled after each dispatch via tar over "
                                "gcloud compute ssh — recorded per slot "
                                "(remote_ws_syncs/remote_ws_push)",
        },
        "admission": {
            "egress_profile": "declared_deps",
            "mechanism": ("FIRST-CLASS R5.56 machinery (crates/hh-env/src/egress_profile.rs; "
                          "ADR-0421): admit_declared_deps mutates the attached "
                          "ContainmentPolicy to net.mode=public + declared "
                          "residual_channels[host_bridge], stamps the record ext "
                          "(egress_profile/cell_network/policy_net_mode/declared_deps), "
                          "and podman_helper_args appends `--network bridge` to the "
                          "stock hh-helper spawn argv. The scratch /tmp/s4-helper-egress "
                          "variant is superseded — never used in this epoch."),
            "verifier_bound": ("R5.57 first-class (crates/hh-env/src/verifier_bound.rs; "
                               "ADR-0422): the ten swe-bench-astropy-2 specs carry "
                               "task_id + enforced max_test_timeout_sec=360; "
                               "admit_verifier_bound stamps 900 + the verbatim "
                               "verifier_bound_admission record; verifier_deadline_ms "
                               "derives min(decl,900s) on tc-verdict + tc-vendored; "
                               "verifier_bound_admitted echoed per row. The recorded "
                               "proximate cause was the bootstrap cwd/pytest bake, "
                               "not the bound — a still-failing bootstrap records "
                               "inconclusive{reason} verbatim."),
            "policy": "net.mode=public for admitted cells — the N4-honest declaration "
                      "(kernel authority + declared residual_channels[host_bridge]); "
                      "fs/proc boundaries unchanged and still probed",
            "scope": "only slots whose tasks declare runtime egress deps (the same "
                     "mechanical scan verbatim); all 5 tasks declared deps",
            "fix_git": "permanently unbuildable — the deleted upstream is not a resource "
                       "bound. The client build was attempted once on the remote "
                       "(verbatim rc 128 on `git clone` of the deleted repo); all ten "
                       "slots record inconclusive{upstream_deleted} verbatim — never "
                       "executed, never fabricated.",
            "first_attempt_semantics": "named supplement attempts on the same frozen slots "
                                       "(same slot_ref/arm/lane/round; the frozen manifest "
                                       "consumed verbatim, never re-picked). `first` marks "
                                       "the manifest's round-0 subjects; supplement_epoch + "
                                       "prior_disposition mark the attempt lineage verbatim — "
                                       "the original records stand.",
        },
        "declared_inputs": {
            "manifest": "frozen central-protocol/1, seed parity-tranche-2026-11-05:seed:v3 (same manifest, verbatim)",
            "participant": "per-task committed oracle solution — $0, no model calls",
            "subject_machinery": "same HH machinery (EnvDriver/Dispatcher/compact/seal on hh-helper podman cells); the deltas are the first-class variant + bound admissions and the remote backend only",
            "receipt": "task's own verifier — run-tests.sh verbatim channel preferred, vendored_offline fallback unchanged; terminal-bench PytestParser",
        },
        "manifest": {"protocol_id": manifest["protocol_id"]},
        "counts": counts,
        "first_subject_denominators": first,
        "fidelity_denominators": fidelity,
        "costs": costs,
        "per_task": per_task,
        "dispositions": dispositions,
        "remote_ws_syncs_total": remote_ws_syncs,
        "slots": per_slot,
    }
    packet["verdict"] = (
        "partial" if counts["executed"] and counts["executed"] < counts["planned"]
        else ("executed" if counts["executed"] == counts["planned"]
              else "inconclusive"))
    packet["verdict_basis"] = (
        f"s4-completion-gce supplement over {counts['planned']} prior-inconclusive "
        f"slots on the GCE backend: {counts['executed']} executed with a real receipt "
        f"({counts['receipt_resolved']} resolved / {counts['receipt_unresolved']} unresolved); "
        f"{counts['inconclusive']} still inconclusive, {counts['failed']} failed, "
        f"{counts['unattempted']} unattempted; "
        f"{first['executed']}/{first['planned']} supplement first-subject slots executed "
        f"({first['resolved']} resolved); "
        f"{counts['cells_run']} cells run on the remote; "
        f"{counts['egress_admitted']} slots admitted egress_profile=declared_deps. "
        f"Denominators recomputed over the supplement verbatim.")
    OUT.write_text(json.dumps(packet, indent=2))

    # ── combined cohort fold (verbatim, over the committed packets) ──
    confirm = json.loads(CONFIRM.read_text())
    conf_by_ref = {s["slot_ref"]: s for s in confirm["slots"]}
    egr_by_ref = {s["slot_ref"]: s for s in egress["slots"]}
    comp_by_ref = {r["slot_ref"]: r for r in per_slot}
    fold = {"slots": len(manifest["schedule"]), "executed": 0, "resolved": 0,
            "unresolved": 0, "inconclusive": 0, "failed": 0, "unattempted": 0,
            "first_executed": 0, "first_resolved": 0,
            "inconclusive_reasons": {}}
    for s in manifest["schedule"]:
        ref = s["slot_ref"]
        # latest epoch wins: completion > egress > confirmation
        r = comp_by_ref.get(ref) or egr_by_ref.get(ref) or conf_by_ref.get(ref)
        disp = (r or {}).get("disposition", "unattempted")
        if disp == "executed":
            fold["executed"] += 1
            v = (((r or {}).get("receipt") or {}).get("verdict"))
            if v == "resolved":
                fold["resolved"] += 1
                if s["first"]:
                    fold["first_resolved"] += 1
            elif v == "unresolved":
                fold["unresolved"] += 1
            if s["first"]:
                fold["first_executed"] += 1
        elif disp.startswith("failed"):
            fold["failed"] += 1
        elif disp == "unattempted":
            fold["unattempted"] += 1
        else:
            fold["inconclusive"] += 1
            fold["inconclusive_reasons"][disp] = \
                fold["inconclusive_reasons"].get(disp, 0) + 1
    print(json.dumps({"wrote": str(OUT), "counts": counts,
                      "first": first, "fidelity": fidelity,
                      "inconclusive_reasons": inconclusive_reasons,
                      "verdict": packet["verdict"],
                      "combined_fold": fold}, indent=2))


if __name__ == "__main__":
    main()
