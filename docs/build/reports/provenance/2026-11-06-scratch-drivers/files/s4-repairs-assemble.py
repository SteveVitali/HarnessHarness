#!/usr/bin/env python3
"""Assemble the S4-30SLOT-REPAIRS supplement packet.

Reads ~/tb-work/s4-repairs/slots/*.json + the frozen manifest + the
s4-completion packet and emits
docs/build/reports/2026-11-06-s4-30slot-repairs.json —
`hh-s4-30slot-repairs/1`, the named `s4-30slot-repairs-gce` epoch over
exactly the 30 slots the completion supplement recorded inconclusive,
under the operator-authorized three repairs (verbatim "let's do
these"). Per-slot verdicts, repair-class members verbatim
(machinery_repair declared_workdir / env_repair archive_mirror /
env_repair vendored_mirror), denominators recomputed over all epochs.
Nothing fabricated: absent slots count as unattempted; every row
carries `prior_disposition` verbatim and the three prior packets stay
byte-untouched.

It also prints the COMBINED cohort fold (confirmation + egress +
completion + repairs) — executed/inconclusive/failed totals,
first-subject denominators — for the run ledger's verbatim record."""
import json
import sys
import time
from pathlib import Path

WORK = Path.home() / "tb-work/s4-repairs"
SLOTS = WORK / "slots"
REPO = Path("/Users/stevenvitali/MetaHarness-harnessharness")
OUT = REPO / "docs/build/reports/2026-11-06-s4-30slot-repairs.json"
MANIFEST = Path("/tmp/parity-manifest-v3.json")
CONFIRM = REPO / "docs/build/reports/2026-11-05-s4-confirmation.json"
EGRESS = REPO / "docs/build/reports/2026-11-05-s4-egress-cohort.json"
COMPLETION = REPO / "docs/build/reports/2026-11-06-s4-completion.json"
EPOCH = "s4-30slot-repairs-gce"


def main():
    manifest = json.loads(MANIFEST.read_text())
    completion = json.loads(COMPLETION.read_text())
    prior_disp = {s["slot_ref"]: s["disposition"] for s in completion["slots"]}
    # the supplement domain: exactly the completion run's inconclusive
    # slots (the 30 the GCE backend could not lift)
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
              "cells_run": 0,
              "machinery_repair": 0, "env_repair": 0}
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
                  "client_build_rc", "task_workdir",
                  "repair_class", "env_repair", "machinery_repair",
                  "repair_note",
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
        if row.get("repair_class") == "machinery_repair":
            counts["machinery_repair"] += 1
        elif row.get("repair_class") == "env_repair":
            counts["env_repair"] += 1
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
        "schema": "hh-s4-30slot-repairs/1",
        "generated": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "harness": "devin-desktop/swe-2-high/subagent",
        "cost_usd": 0.0,
        "protocol_id": manifest["protocol_id"],
        "phase": "confirmation-repairs-supplement",
        "supplement_of": (
            "docs/build/reports/2026-11-05-s4-confirmation.json (hh-s4-confirmation/1 — untouched) + "
            "docs/build/reports/2026-11-05-s4-egress-cohort.json (hh-s4-egress-supplement/1 — untouched) + "
            "docs/build/reports/2026-11-06-s4-completion.json (hh-s4-completion/1 — untouched)"),
        "supplement_epoch": EPOCH,
        "operator_authorization": (
            "verbatim: \"let's do these\" — operator chat 2026-11-06, "
            "authorizing all three repairs to the 30-slot inconclusive "
            "residue (verbatim diagnoses recorded per repair class)"),
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
        "repairs": {
            "swe-bench-astropy-2": {
                "repair_class": "machinery_repair",
                "machinery_repair": "declared_workdir",
                "diagnosis": (
                    "OUR driver's defect, verbatim: the cell exec'd the "
                    "oracle solution + verifier at `cd /app` while the "
                    "task image declares WORKDIR /app/astropy — every "
                    "relative path in run-tests.sh (patch -p1 -i "
                    "/app/test_patch.diff -> astropy/io/..., sed "
                    "pyproject.toml, pip install -e .[test], pytest "
                    "astropy/io/ascii/tests/test_qdp.py) and in "
                    "solution.sh (patch -p1 -i "
                    "/app/astropy/solution_patch.diff) resolves only at "
                    "the repo root. The parity H-side run resolves this "
                    "task — the task itself works."),
                "mechanism": (
                    "s4-cell spec gains `task_workdir` read verbatim "
                    "from `podman image inspect` .Config.WorkingDir of "
                    "the built cell image; all four dispatches exec "
                    "`cd <task_workdir>` (empty spec = the prior /app "
                    "default, echoed verbatim). Same repair class as "
                    "the ANSI fix — a driver defect repaired, task "
                    "files byte-untouched."),
            },
            "qemu-alpine-ssh": {
                "repair_class": "env_repair",
                "env_repair": "archive_mirror",
                "diagnosis": (
                    "upstream drift, verbatim: EOL bullseye dropped off "
                    "deb.debian.org — the client build's apt-get hit the "
                    "recorded rc-100 deb404 (Failed to fetch "
                    "…/qemu-system-gui_5.2+dfsg-11+deb11u5_amd64.deb "
                    "404 Not Found)."),
                "mechanism": (
                    "a mirror-patched base "
                    "localhost/s4-mirror/debian-bullseye-slim = "
                    "debian:bullseye-slim + sed rewriting every default "
                    "suite to archive.debian.org (bullseye, "
                    "bullseye-security, bullseye-updates — all verified "
                    "resolving, apt-get update clean); the client image "
                    "built from the task Dockerfile with ONLY the FROM "
                    "line rewritten to the patched base — the verbatim "
                    "rewritten Dockerfile in the build record; no task "
                    "file changes. A RECORDED environment admission, "
                    "never a silent change."),
            },
            "fix-git": {
                "repair_class": "env_repair",
                "env_repair": "vendored_mirror",
                "diagnosis": (
                    "deleted upstream, verbatim: setup.sh clones "
                    "github.com/TheMikeMerrill/personal-site (rc 128). "
                    "Honest search: `gh api` 404 on the repo; no "
                    "personal-site repo/fork under TheMikeMerrill's 11 "
                    "public repos; pinned commit d7d3e4b absent from "
                    "mldangelo/personal-site and all TheMikeMerrill "
                    "repos; terminal-bench main STILL clones the "
                    "deleted URL — no task-version admission exists."),
                "mechanism": (
                    "the Software Heritage archive holds a full "
                    "snapshot of the deleted origin — swh:1:snp:"
                    "b2cbd5476b5a00d133f224e990498de4e06bad01 (origin "
                    "visit 8, 2026-04-04T11:00:17Z, status full); the "
                    "vault git-bare cook is vendored verbatim at "
                    "~/tb-work/vendored/fix-git/personal-site.git "
                    "(all branches; pinned commit "
                    "d7d3e4ba9350f634d92d39ace2b471433ee57d50 "
                    "'off the job market woo' verified in "
                    "refs/heads/master history via git cat-file). The "
                    "client build runs the task Dockerfile with ONLY "
                    "added `COPY vendor/personal-site.git /app/vendor/` "
                    "+ `git config --system url.insteadOf` lines — the "
                    "verbatim setup.sh clone is redirected to the "
                    "archived mirror for that leg only (config unset "
                    "right after); every task file byte-identical, the "
                    "same upstream objects materialize."),
            },
            "recorded_verbatim": (
                "every slot row carries repair_class + env_repair|"
                "machinery_repair + repair_note verbatim; a slot whose "
                "repair still can't genuinely run records "
                "inconclusive{reason} — never a fabricated verdict."),
        },
        "admission": {
            "egress_profile": "declared_deps",
            "mechanism": ("unchanged first-class R5.56 machinery "
                          "(crates/hh-env/src/egress_profile.rs; "
                          "ADR-0421) + R5.57 verifier-bound admission "
                          "(ADR-0422) retained verbatim on the ten "
                          "astropy specs — verifier_bound_admitted "
                          "echoed per row."),
            "scope": "only slots whose tasks declare runtime egress deps "
                     "(the same mechanical scan verbatim); all 3 tasks "
                     "declared deps",
            "first_attempt_semantics": "named supplement attempts on the "
                                       "same frozen slots (same "
                                       "slot_ref/arm/lane/round; the "
                                       "frozen manifest consumed "
                                       "verbatim, never re-picked). "
                                       "`first` marks the manifest's "
                                       "round-0 subjects; "
                                       "supplement_epoch + "
                                       "prior_disposition mark the "
                                       "attempt lineage verbatim — the "
                                       "original records stand.",
        },
        "declared_inputs": {
            "manifest": "frozen central-protocol/1, seed parity-tranche-2026-11-05:seed:v3 (same manifest, verbatim)",
            "participant": "per-task committed oracle solution — $0, no model calls",
            "subject_machinery": "same HH machinery (EnvDriver/Dispatcher/compact/seal on hh-helper podman cells); the deltas are the task_workdir machinery repair + the two recorded env admissions",
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
        f"s4-30slot-repairs-gce supplement over {counts['planned']} "
        f"prior-inconclusive slots on the GCE backend under three "
        f"verbatim-recorded repairs: {counts['executed']} executed with "
        f"a real receipt ({counts['receipt_resolved']} resolved / "
        f"{counts['receipt_unresolved']} unresolved); "
        f"{counts['inconclusive']} still inconclusive, "
        f"{counts['failed']} failed, {counts['unattempted']} "
        f"unattempted; {first['executed']}/{first['planned']} "
        f"supplement first-subject slots executed "
        f"({first['resolved']} resolved); {counts['cells_run']} cells "
        f"run on the remote; {counts['machinery_repair']} slots under "
        f"machinery_repair=declared_workdir; {counts['env_repair']} "
        f"slots under env_repair (archive_mirror|vendored_mirror); "
        f"{counts['egress_admitted']} slots admitted "
        f"egress_profile=declared_deps. Denominators recomputed over "
        f"the supplement verbatim.")
    OUT.write_text(json.dumps(packet, indent=2))

    # ── combined cohort fold (verbatim, over the committed packets) ──
    confirm = json.loads(CONFIRM.read_text())
    egress = json.loads(EGRESS.read_text())
    conf_by_ref = {s["slot_ref"]: s for s in confirm["slots"]}
    egr_by_ref = {s["slot_ref"]: s for s in egress["slots"]}
    comp_by_ref = {s["slot_ref"]: s for s in completion["slots"]}
    rep_by_ref = {r["slot_ref"]: r for r in per_slot}
    fold = {"slots": len(manifest["schedule"]), "executed": 0, "resolved": 0,
            "unresolved": 0, "inconclusive": 0, "failed": 0, "unattempted": 0,
            "first_executed": 0, "first_resolved": 0,
            "inconclusive_reasons": {}}
    for s in manifest["schedule"]:
        ref = s["slot_ref"]
        # latest epoch wins: repairs > completion > egress > confirmation
        r = rep_by_ref.get(ref) or comp_by_ref.get(ref) or \
            egr_by_ref.get(ref) or conf_by_ref.get(ref)
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
