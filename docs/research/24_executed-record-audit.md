# 24_executed-record-audit — Executed-record audit
Stream: A
Researched: 2026-11-06 (inspection 2026-11-06T22:50Z–23:00Z)
Author: Claude Code subagent (Opus 5.5), experiments-program worker for row A1
Ledger rows covered: A1
Repository baseline: main 75883daa + read-only codex/harnessharness-r5.59 36dfcb3f (that branch merged to `main` as 09fe8523 at 2026-11-06T22:56:05Z during this inspection; `git diff 36dfcb3f 09fe8523` is empty, so every R5.59 citation below holds for both)
Confidence overall: high

## Scope and method

The unit of audit is the committed record. For each record I read the run ledger (`docs/build/runs/*.md`), parsed the JSON packet (`docs/build/reports/*.json`) with `python3 -I`, and traced the code that produced it in the scratch-driver archive `~/tb-work/scratch-archive-2026-11-06/`. All 71 manifest lines in `~/tb-work/scratch-archive-2026-11-06.sha256` verify (`shasum -a 256 -c`, no mismatches). I also read the committed library code those drivers call (`crates/hh-eval`, `crates/hh-context`, `crates/hh-env`). Every number in this note was recomputed from the committed packets unless it is marked "record's own statement". I ran no build, test, container, network or cloud command. Wherever this note says what landed code *would* return, that conclusion comes from reading the source (inference), not from running it.

## 1. Per-record table

"Arms served" counts `served_variant` per `arm_id` over the record's own `slots[]`. A = `arm:o-tb-oracle` → `evict_oldest`. B = `arm:h-podman-oracle` → `clear_tool_results`. The arm-to-strategy binding comes from `declared_inputs.arms`.

| Record | Surface / code path that ran | R per task | Participant | Arms intended → served | Estimator actually invoked | Supports | Does **not** support |
|---|---|---|---|---|---|---|---|
| PARITY-TRANCHE (t1), [run](../build/runs/PARITY-TRANCHE.md), [packet](../build/reports/2026-11-05-parity-tranche.json) | O: `tb run -a oracle` (terminal-bench harness, docker compose → podman applehv VM). H: `/tmp/parity-h-side.py` (Python, podman CLI, tb's `PytestParser`). Manifest: scratch `/tmp/parity-driver` → `hharness_eval::central_protocol::materialize_protocol` | 1 | Committed `solution.sh`/`solution.yaml` on both sides | Arms are "frozen scaffolding" (`declared_inputs.arms_note`); no compaction ran on either side | `materialize_protocol` only (selection/schedule). No parity estimator | A recorded 20/30 agreement, `infeasible{margin}`; 13/13 agreement on genuine O verdicts | Any HH-cell claim; any statistical parity admission |
| PARITY-TRANCHE-2 (t2), [run](../build/runs/PARITY-TRANCHE-2.md), [packet](../build/reports/2026-11-05-parity-tranche-2.json) | Same as t1. The H driver is platform-pinned and batches are pruned between runs | 1 | Same | Same (scaffolding) | `materialize_protocol` only | 28/30 agreement, `infeasible{margin}`; diagnosis of the ANSI capture-channel divergence | Same |
| PARITY-TRANCHE-3 (t3), [run](../build/runs/PARITY-TRANCHE-3.md), [packet](../build/reports/2026-11-05-parity-tranche-3.json) | Same as t2, plus the ANSI strip added to the *scratch* H driver and `--n-concurrent 1` on O | 1 | Same | Same (scaffolding) | `materialize_protocol` only. The quoted "≈0.116" is not computed by any driver (F3) | 30/30 directional agreement between `tb` and the scratch podman-exec runner, on arm64 applehv, over 26 executed pairs and 4 build-stage pairs | Parity of **HH cells** with `tb`; REQ-D-301…310 admission; any KL/TOST equivalence verdict |
| S4-CONFIRMATION, [run](../build/runs/S4-CONFIRMATION.md), [packet](../build/reports/2026-11-05-s4-confirmation.json) | HH cells. Scratch `/tmp/s4-cell` (Rust; `EnvDriver`, `Dispatcher`, `HelperExecutor`, `hh-helper --backend podman`, `--network none`) is driven by scratch `/tmp/s4-orch.py`. Verdicts are parsed in Python with tb's `PytestParser` | 5 per task per arm | Same oracle | A: 80/80 `hh/evict-oldest@1`. B: 50 `hh/clear-tool-results@1`, 30 `hh/evict-oldest@1`; 20 per arm had no compaction (image unavailable) | None (packet assembly only) | 160/300 executed through HH cells (130 R / 30 U); 140 inconclusive with reasons | A compaction contrast; parity of these cells with `tb` |
| S4-EGRESS, [run](../build/runs/S4-EGRESS.md), [packet](../build/reports/2026-11-05-s4-egress-cohort.json) | HH cells. The **scratch** helper variant `/tmp/s4-helper-egress` (`--network bridge`) is patched into s4-cell, with `/tmp/s4-egress-orch.py` | 5 | Same | A: 45 evict. B: 20 clear, 25 evict (no-op); 20 per arm no compaction | None | 90/140 prior-inconclusive slots executed under bridge egress | That the bridge path was committed machinery (it was not until R5.56) |
| S4-COMPLETION (the R5.58 row; its ledger is S4-COMPLETION.md, not R5.58.md, per the record's own note), [run](../build/runs/S4-COMPLETION.md), [packet](../build/reports/2026-11-06-s4-completion.json) | HH cells on the GCE backend (amd64, rootful podman). The committed R5.56 `admit_declared_deps`/`podman_helper_args` and R5.57 `admit_verifier_bound` are called from the patched scratch s4-cell, with a remote `/work` mirror over `gcloud compute ssh` | 5 | Same | A: 10 evict. B: 10 clear; 15 per arm no compaction | None | 20/50 executed (oom 10 R, eval-mteb.hard 10 U) | First-attempt outcomes (these are second or third attempts) |
| S4-30SLOT-REPAIRS, [run](../build/runs/S4-30SLOT-REPAIRS.md), [packet](../build/reports/2026-11-06-s4-30slot-repairs.json) | Same GCE path, plus the scratch s4-cell `task_workdir` fix, a rewritten-`FROM` mirror base, and a vendored Software Heritage git mirror | 5 | Same | A: 15 evict. B: 10 clear, 5 evict | None | 30/30 executed (20 R / 10 U) | First-attempt outcomes (fourth attempts); an unchanged task build environment |
| R5.59 efficacy (`hh-s4-efficacy/1`), `36dfcb3f:docs/build/reports/2026-11-06-r5.59-efficacy.json` | Committed code: `hharness_eval::finite_manifest::finite_manifest_contrast` runs from `crates/hh-env/tests/r5_49.rs` over byte-copy fixtures of the four S4 packets ("last epoch record wins") | 5 | Same | Contrast is by **assigned** `arm_id`; `served_variant` is not read | **`finite_manifest_contrast`** (Beta(1,1), 64,000 draws) | A deterministic recompute of the registered contrast over the composite fold: observed 0 ppm in every stratum | Any statement about compaction-policy efficacy (F6, F10) |
| Study reports `hh-study-report/1`: [2026-11-04](../build/reports/2026-11-04-r5.49-study-report.json), [2026-11-06](../build/reports/2026-11-06-r5.49-study-report.json), R5.59 `36dfcb3f:docs/build/reports/2026-11-06-r5.59-study-report.json` | Committed `crates/hh-env/src/study.rs` folds the packets' **recorded** members (counts, verdict strings). The parity leg checks `tranche_verdict == "verified"`, `discordant == 0` and `n_pairs == 30` (`evaluate_parity_stage`) | n/a | n/a | n/a | None at the parity leg. At R5.59 the efficacy leg re-adds the estimator's cells | Record folding and self-consistency: 11/04 `unmet{efficacy_unavailable;parity_withheld}`; 11/06 `unmet{efficacy_unavailable;s4_partial}` (`partial{250_300}`); R5.59 `unmet{s1_failed}`, `executed{300_300}` | Any parity inference; it echoes the tranche's own `verified` label |

## 2. Brief §2 reconciliation

| §2 claim | Status | Evidence |
|---|---|---|
| "Mechanism: … Replaying [the gold script] inside HH cells and grading with the task's own verifier proves the execution surface is faithful" | **CORRECTED** | The oracle and $0 parts are true. But only S4 replayed inside HH cells, and S4 has **no** original-runner comparison. S3 compared `tb` with a scratch Python podman-exec driver (F1). In S4, grading used run-tests.sh in only 200/300 final receipts; 100 used `vendored_offline`, which bypasses the task's bootstrap. Verdicts were parsed by Python outside HH (F5). |
| "Execution surface = the HH cell machinery … `--network none` default; admitted `declared_deps` bridge variant, ADR-0421 … sentinel/observer … sealed artifacts … verifier parsing (ANSI-stripped, workdir-respecting — two real machinery defects found and repaired by the experiment itself)" | **CORRECTED** | HH-cell items are VERIFIED for S4. All 300 final slots have `sentinel_intact`/`observer_survived` true and `seal: sealed`; the final fold is 160 `none` and 140 `bridge`. But `probe` is `ok:undeterminable` on all 300. The bridge ran first as a scratch helper; it is first-class only from S4-COMPLETION on. Both "machinery defects" were repaired in **scratch drivers** (`strip_ansi` in `parity-h-side.py`/`s4-orch.py`; `task_workdir` in `s4-cell`). `rg strip_ansi\|task_workdir\|PytestParser crates scripts` returns nothing. The workdir defect sat in the HH-cell driver that S3 never exercised (F1, F9). |
| "S3 parity: 30/30 task-verdict agreement vs the original Terminal-Bench runner, zero discordance, `kl_directional_bound` ≈0.116 at N=30 — `verified`" | **CORRECTED** | The 30/30 count and the zero-discordance record are VERIFIED. But H was not HH (F1); R=1 (F2); 4/30 agreements are build-stage, and 26/30 H runs reused the O-built image (F4). ≈0.116 is 1−.025^(1/30) quoted from the spec, not computed (F3). `verified` is the tranche's own exact-agreement rule. The landed `parity_admission` would refuse this design (F2). It is the third epoch, after two `infeasible{margin}` epochs. |
| "S4 confirmation: 300/300 slots executed with real receipts (30 tasks × 2 arms × 5 rounds; 220 resolved / 80 unresolved; 60/60 first subjects)" | **CORRECTED** | The arithmetic is VERIFIED over a four-epoch "last record wins" fold. However, 140 slots were re-attempted (90 twice, 20 three times, 30 four times). 28 of the 60 round-0 receipts are not first attempts. 50 slots ran on a different backend and architecture (F8). |
| "Three epochs of honest records" | **CONTRADICTED** | S4 has **four** epochs: confirmation, egress, completion, repairs. S3 adds three tranches. |
| "every repair (ANSI, OOM, disk, workdir, archive-mirror, vendored-repo) recorded verbatim as machinery or admission — never hidden" | **PARTIALLY VERIFIED** | Each repair is recorded in prose. The list omits the two biggest admissions (the egress bridge and the 360→900 verifier bound) and three H-driver defects. OOM and disk are environment changes, not machinery or admission. A structured `repair_class` exists only in the repairs packet (F9). |
| "Cost: $0.00 inference; ~$1.41 GCE instance-hours; zero model calls" | **PARTIALLY VERIFIED** | $0 inference is VERIFIED structurally: `cost_usd` 0.0 and `costs.usd` 0.0, with `exposure_micros` 0 in every packet. $1.41 appears only in prose. It is a rate-card estimate (≈4.7 h × ≈$0.30/h), not a billing figure, and the local applehv time is uncosted (F11). |
| "Evidence: …parity-tranche-3.json, …s4-*.json; ledgers PARITY-TRANCHE-3.md, S4-*.md" | **VERIFIED (incomplete)** | All files exist. The list omits tranches 1–2, the R5.56/R5.57 admissions and the R5.59 efficacy/study packets. |
| Phase-3 bullet: "the same parity machinery that just verified 30/30 … (same participant, both surfaces, exact directional agreement, declared margins, recorded discordances)" | **CORRECTED** | The "parity machinery" that ran is scratch (`parity-h-side.py` plus a bash pipeline), not committed. The only margin declared was "EXACT", which is not a §4.4 ε. The committed parity machinery (`parity_admission`, `directional_kl::kl_equivalence`) has never run on an executed tranche (F2, F3). |

**Pre-charter observations (brief A-2, task statement): verify or correct.** "S3 H side was `/tmp/parity-h-side.py` (direct podman, not HH cells)": VERIFIED. "S3 ran R=1": VERIFIED. "Landed KL/TOST estimators were not run; ≈0.116 is 1−.025^(1/30)": VERIFIED (1−.025^(1/30) = 0.11570; spec §4.5 lists .115703). "S4 ran through HH cells": VERIFIED, with the caveat that HH ends at log capture. "60 B-arm slots served `hh/evict-oldest@1`": VERIFIED *for the confirmation packet*; the final composite fold also has 60, but 100 served deviations exist across all epoch records (F7). "With an oracle participant the A/B compaction contrast cannot affect outcomes": VERIFIED from source, and per-task outcomes are invariant (F6). "Scratch drivers lived only in /tmp": VERIFIED. They still exist there and in the archive, and they compiled against the *shared live worktree* by path-dep (F15). "Instance-hours recorded only in prose": VERIFIED (F11).

## 3. Public-surface drift

| Surface / locator (main 75883daa = 09fe8523 for these files) | States | Latest record | Drift |
|---|---|---|---|
| `docs/evidence.md` study card (lines ~194–224) | `partial{250_300}`; cites the 2026-11-06 r5.49 study report; "no efficacy, parity … result exists", while the same card says "parity is now `verified{30_30}`" | 270/300 (S4-COMPLETION), then 300/300 (repairs); R5.59: `executed{300_300}`, efficacy `computed{…}` | Stale twice over, and contradicts itself |
| `docs/evidence.md` operations card (lines ~226–245) | "confirmation tranche … stayed `withheld{tranche_absent}` … `s4_unmet` remains `true`" | Executed (four epochs); R5.59 `s4_unmet:false` | Stale since S4-CONFIRMATION |
| `docs/evidence.md` scenario table rows S3/S4 (lines ~403–404) | S3 "efficacy still unavailable (partial cohort, no contrast)"; S4 "250/300 receipts, 50 inconclusive" | 300/300; contrast computed | Stale. Also mixes scenario IDs (F14) |
| `docs/evidence.md` unmet block (lines ~409–420) | "S4's confirmation cohort `partial`" | 300/300 | Stale. This is a pinned `<!-- unmet -->` region: edits must respect `check-position.sh`'s mutation battery (OL-12) |
| `docs/status.md` §Round-5 evidence program (lines 94–111) | "*Update 2026-11-05*", `unmet{efficacy_unavailable;s4_partial}`, "S1/S4 unmet", "Three deferrals … remain open" | R5.59 `unmet{s1_failed}`, `s4_unmet:false`; `bash scripts/deferrals-open.sh` returns empty on main | Stale |
| `README.md` | No link to any parity, confirmation or study record | — | OL-01 needs a link; none exists |
| R5.59 change set | `git diff --stat 75883daa 09fe8523 -- docs/evidence.md docs/status.md README.md` is empty | — | Closure updated none of the three surfaces |
| GATE-G19 readout (`36dfcb3f:docs/build/readouts/GATE-G19.md`) | Labels every S3/S4 packet "layer: live-executed" | No `hh-live-evidence/1` record for these runs under `docs/build/reports/live/` | Root AGENTS.md gotcha 5 (ADR-0363) reserves `live-executed` for cited live-evidence records. This is a historical record: correct it with a dated note, not an edit |
| R5.59 efficacy text ("a sound null-equivalent contrast") | — | Spec §4.3: "Credible intervals are not frequentist TOST or automatic practical-margin certificates" | The wording overstates (F10) |

## 4. Spec reconciliation

**§4.1 (TB 2.0).** §4.1 retains "TB **2.0**" and imports "the 89-task TB2 catalogue identity". Every S3/S4 manifest declares `suite_ref: terminal-bench-core==0.1.1` with `catalogue_tasks: 80`. A grep for `terminal-bench-core|tb-core|0\.1\.1` over `docs/adr`, `docs/build/planning`, `docs/build/readouts`, `docs/tickets`, `docs/build/LEDGER.md` and `docs/build/pr` finds nothing. The only hits are the run records and packets themselves, plus this program's brief. REQ-E-011 (`docs/build/planning/2026-10-30_evidence-coverage.json:2131`) says admission "rejects silent substitutions". The substitution is unrecorded (F13). §4.1 also requires separate identities for parity and confirmation and says "material changes create a new cohort". The confirmation reused the parity tranche's `protocol_id` 9edc5327, and the egress (network mode), GCE (platform) and repairs (build inputs) epochs stayed inside it (F8).

**§4.4.** §4.4 requires the "**same participant/configuration** under HH (H) and original runner (O)", first attempts, "adapter floor ≥3/task and S3 floor ≥5/task/arm", and says "oracle transport equality alone fail[s]". The landed code agrees: `crates/hh-eval/src/parity_admission.rs` sets `ADAPTER_FLOOR = 3` (l.48) and `S3_FLOOR = 5` (l.50), lists `OracleTransportEquality` as a typed `InauthenticEvidence` shape (l.190–194, refused at l.856–862), and checks floors after the design-equality checks (l.900–910). §4.5 additionally requires applicability "to both actual policies". Neither tranche runner ran any compaction policy.

## 5. Scratch-driver inventory

All Rust drivers are standalone crates (`[workspace]` empty) whose path-deps point at `/Users/stevenvitali/MetaHarness-harnessharness/crates/*`. That is the shared, moving build worktree, not a pinned commit. All Python drivers import `terminal_bench.parsers.pytest_parser` from a hard-coded `~/.local/share/uv/tools/terminal-bench/lib/python3.12/site-packages`.

| File (archive) | Lang | Purpose | Committed crates/APIs called | Produced | Hard-coded inputs / env assumptions | To commit reproducibly |
|---|---|---|---|---|---|---|
| `parity-driver/` (`Cargo.toml`, `src/main.rs`, 176 l) | Rust | Materialize the seeded manifest from `task|split|stratum` lines plus a seed | `hharness_eval::central_protocol::{materialize_protocol, ArmDecl, ProtocolDecl, Quota, TaskEntry, TaskSplit, Warmth, ARM_STRATEGIES, PHASES, PRIMARIES, RETRYABLE_CLASSES}`, `hharness_lab::experiment::{SchedulingPolicy, ReattemptPolicy, …}` | `parity-manifest{,-v2,-v3}.json`, each structurally equal to the committed packet's `manifest` (checked by `json.load` equality; v3 also equals the S4-confirmation manifest) | Quotas 5/16/9, arms, lanes and phases in code; `exclusion_publication_ref` fixed to `PARITY-TRANCHE.md` even for v2/v3; catalogue identity is FNV-1a-64 printed with a `sha256:` prefix | Workspace member or test-binary with the declaration as a committed fixture; correct the identity label; one source serving all three seeds |
| `parity-tasks.txt`, `parity-taskids*.txt`, `parity-batch-*.txt`, `parity-t3-b*.txt` | data | Catalogue (78 held-out + 2 dev) and per-batch task lists | — | Driver inputs | Generated from `~/tb-work/dataset` | Commit as fixtures with the dataset digest |
| `parity-h-side.py` (476 l) | Python | H runner: reproduce the compose posture with podman, replay the oracle, run run-tests.sh, parse with `PytestParser` (ANSI-stripped) | **No HH crate** | H side of t1–t3; records in `~/tb-work/parity-runs/h-side/{h1,h2,h3,t2,t3}` (not committed) | `~/tb-work/dataset`, the tb venv path, the output dir; default podman networking; only the final (t3) version is archived, so the t1/t2 driver versions are lost | Commit as a labelled *reference runner* (not HH) with its version history, or retire it in favour of B2's committed runner |
| `parity-t3-pipeline.sh`, `parity-t3-diskwatch.sh` | bash | t3 O→H→prune batch loop; buildx-cache watchdog | — | t3 orchestration and logs | `DOCKER_HOST=unix:///tmp/podman-api/podman.sock`, machine name `podman`, `/tmp` log paths | Fold into the runner as a recorded hygiene policy |
| `s4-cell/` (1,405 l) | Rust | One frozen slot through HH: provision/attach, stage → oracle → verifier verbatim → vendored, `compact()`, seal, collect, teardown; remote `/work` sync | `hharness_env::{driver::EnvDriver, dispatch::Dispatcher, helper::HelperExecutor, egress_profile, verifier_bound, ops::{Sentinel,OpsObserver}, record::*}`, `hharness_context::compact::{compact, EvictOldest, ClearToolResults}`, `hharness_ledger::store::{Store, Lease}`, `hharness_monitor`, `hharness_containment`, `hharness_compiler`, `hharness_hir`, `hharness_provenance`, `hharness_secrets`, `hharness_wire`, `hharness_identity` | Per-slot records for all four S4 epochs | `window_cap: 400` and `occupancy > 400` (l.1159–1164); `cd /app` default; `gcloud compute ssh` sync; **only the final patched version is archived**, so the confirmation- and egress-era sources are not recoverable | Commit under `crates/hh-env` (test-binary or `xtask`) with git history; record the driver commit in each packet |
| `s4-helper-egress/` (160 l) | Rust | `hh_helper.rs` copy plus `--network none|bridge` | `hharness_helper::{podman::PodmanBackend, server::{serve, HelperServer}}` | S4-EGRESS cells only | Spawned by `S4_HELPER_NETWORK`/argv | Superseded by R5.56. Keep it archived as the egress epoch's provenance |
| `s4-orch.py`, `s4-egress-orch.py`, `s4-completion-orch.py`, `s4-repairs-orch.py` (472/501/542/740 l) | Python | Per-task build → derived image (pre-staged packages) → 10 slots via s4-cell → `PytestParser` verdict | Spawns s4-cell with `HH_HELPER_BIN=/Users/stevenvitali/MetaHarness-harnessharness/target/debug/hh-helper` | Per-slot records | `/tmp/parity-manifest-v3.json`; the prior packet is read from the *shared worktree's* `docs/`; `~/tb-work/{dataset,s4-*,vendored}`; GCE socket `/tmp/podman-gce/podman.sock`; SSH recipe and port | Replace with B2's committed runner. Freeze inputs as fixtures; no absolute paths |
| `s4-assemble.py`, `s4-egress-assemble.py`, `s4-completion-assemble.py`, `s4-repairs-assemble.py` | Python | Fold `~/tb-work/s4-*/slots/*.json` into the packet | — | The four committed S4 packets | Write **directly into** `/Users/stevenvitali/MetaHarness-harnessharness/docs/build/reports/…`; the completion/repairs docstrings promise "billed instance-hours", but the code emits only `usd: 0.0` | Committed assembler with a recompute check against committed raw-record digests |
| `*.log`, `*.err`, `parity-smoke.txt`, `parity-pairs-v{2,3}.json` | text/JSON | Batch logs, stderr, pair folds | — | Source of the packets' pair tables | — | Retain as hashed evidence |

Not archived (they exist only under `~/tb-work`, read-only `ls`): O-side `tb` run dirs (`parity-runs/o-side{,-2,-3}`), H-side records, the 520 S4 per-slot records (`s4-confirm` 300, `s4-egress` 140, `s4-completion` 50, `s4-repairs` 30), build records, the derived/mirror images and the vendored mirror. The committed packets therefore cannot be re-derived from committed or archived inputs.

## 6. Findings

### F1 — The S3 "H side" was a scratch Python podman driver, not HH cells, and it differs from the S4 HH-cell surface in ways that changed verdicts
Claim: All three S3 tranches compared `tb run -a oracle` with `/tmp/parity-h-side.py`. That script uses podman CLI calls plus tb's own `PytestParser` and no HH crate. Its posture (default podman network, image WORKDIR, Python ANSI strip) differs from the S4 HH cells.
Status: VERIFIED
Evidence kind: source inspection + recorded execution (inspected)
Evidence: `PARITY-TRANCHE*.md` §Runners ("H-side … direct podman execution"); `parity-h-side.py` imports only stdlib, `yaml` and `terminal_bench` (l.29–42); the client runs with `podman run -d` and no `--network none` (l.328–341); verifier via `podman exec` (l.392). Divergence: `swe-bench-astropy-2` resolved on H in t3, but was `inconclusive{verifier_unavailable}` in HH cells until the scratch `task_workdir` repair (`S4-30SLOT-REPAIRS.md` §repairs). 14/30 tasks that H executed were inconclusive in HH cells at confirmation (`s4-confirmation.json` `per_task`).
Retrieved: 2026-11-06T22:52Z
Implication for the spec: S3 supports `tb` ↔ reference-runner agreement only. HH-cell ↔ `tb` agreement is unmeasured and must be established (A2/B2) before any HH-surface parity claim.
Outline delta: CORRECTS OL-01 — the record's subject is "tb vs a reference podman runner" plus "HH-cell gold-script execution", not "HH cells verified against tb".
Limitations: It does not show that HH cells would *disagree* with `tb` on the 26 executed t3 tasks; that is untested.

### F2 — S3 ran R=1 per task; the landed `parity_admission` would refuse the design before any KL step
Claim: Each tranche has one first attempt per task per runner (N=T=30). `parity_admission` requires repeats ≥3 (adapter) and ≥5 (S3), declared policies, pins, a dimension map and replay refs.
Status: VERIFIED (design); the refusal is UNVERIFIED by execution (inference from source)
Evidence kind: source inspection
Evidence: The tranche endpoint says "one first attempt per task per runner". `parity_admission.rs` l.48/50 (floors) and l.884–910 (`pairs.len() == T·R`, then `AdapterFloor`/`S3Floor`). Earlier checks at l.846–853 require a non-empty `participant_ref`, pins and `policies`. Neither tranche runner ran a policy (`arms_note`: "record scaffolding").
Retrieved: 2026-11-06T22:55Z
Implication for the spec: The record must label `verified` as the tranche-local exact-agreement rule. A2 should run `parity_admission` records-in and commit its typed refusal (expected `AdapterFloor{repeats:1}` or an earlier identity refusal) as the honest admission outcome.
Outline delta: EXTENDS OL-15 — parity declarations need floors stated before data.
Limitations: The exact first refusal depends on how A2 maps tranche fields onto `ParityAdmissionDecl`.

### F3 — No landed estimator ran on any tranche; "≈0.116" is the closed-form zero-discordance endpoint
Claim: No driver calls `kl_directional_bound` or `kl_equivalence`. The figure equals 1−.025^(1/30) = 0.11570. No ε was registered, so §4.4's strict `L_Δ>−ε ∧ U_Δ<ε` rule has nothing to test.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `parity-driver/src/main.rs` calls only `materialize_protocol` and `check_id`. `stats.rs:500–510` gives U=1−η^(1/n) at x=0. Spec §4.5 lists ".115703" at N=30. `rg kl_directional_bound` hits the run docs only as prose. `study.rs` `evaluate_parity_stage` (36dfcb3f) compares strings and counts.
Retrieved: 2026-11-06T22:55Z
Implication for the spec: Describe 0.116 as each direction's 97.5% one-sided concentration endpoint (Δ interval [−0.116, 0.116]). It is neither an admitted margin nor an equivalence verdict. Any future verdict must name ε before data.
Outline delta: CORRECTS OL-01.
Limitations: None material.

### F4 — The t3 "30/30" is 26 executed pairs plus 4 build-stage agreements, with shared images
Claim: 22 pairs resolved on both sides and 4 failed tests on both. In 4 pairs (eval-mteb.hard, fix-git, oom, qemu-alpine-ssh) both builds failed. 26/30 H runs reused the O-built image, and all ran on one arm64 applehv VM.
Status: VERIFIED
Evidence kind: recorded execution (inspected)
Evidence: `parity-tranche-3.json` `pairs[]` tally (22 / 4 / 4), `counts.h_image_reuse` 26, `environment_notes`. Two of the build-stage tasks later resolved in HH cells on GCE (`oom` 10 R; `fix-git` 10 R after the mirror repair).
Retrieved: 2026-11-06T22:58Z
Implication for the spec: Report executed-pair agreement (26/26) separately. Image reuse means build fidelity was not compared. S3 says nothing about amd64/GCE.
Outline delta: EXTENDS OL-06 — HIR parity must declare which stages (build/exec/verify) are compared.
Limitations: None.

### F5 — S4 ran through HH cells, but grading and part of the environment sat outside HH
Claim: In S4, provisioning, dispatch, sealing, sentinel and observer are HH. Verdict parsing is tb's `PytestParser` in scratch Python. 100/300 final receipts used `vendored_offline`, which bypasses run-tests.sh's bootstrap. Images were derived with pre-staged packages. The multi-service `security-vulhub-minio` ran single-container and still recorded `resolved`.
Status: VERIFIED
Evidence kind: source inspection + recorded execution (inspected)
Evidence: `s4-cell/src/main.rs` imports (l.23–76); `s4-orch.py` l.31 and l.312–329 (`PARSER.parse`); final-fold channels 200 `verbatim` / 100 `vendored_offline` (10 tasks); `S4-CONFIRMATION.md` §Declared honest bounds (derived images). `s4-cell` has no `multi_service` handling (grep); egress slot `security-vulhub-minio` shows `egress_evidence: "Could not resolve"` with `resolved`. Probe is `ok:undeterminable` on all 300.
Retrieved: 2026-11-06T22:58Z
Implication for the spec: The record must name where HH ends (captured logs). Report the channel per slot, flag environment departures (derived images, sidecars absent), and keep the committed runner's verdict channel inside the repo.
Outline delta: EXTENDS OL-01.
Limitations: I did not re-inspect raw logs to judge whether the minio verifier is meaningful without sidecars.

### F6 — With the oracle participant, the A/B compaction contrast cannot affect outcomes
Claim: s4-cell calls `compact()` *after* the work dispatch, on a synthetic three-candidate plan (bytes/4 token estimates, `window_cap: 400`). The resulting view is never delivered to anything: the verifier command is a fixed string. All 30 tasks have identical verdicts across all 10 slots.
Status: VERIFIED
Evidence kind: source inspection + recorded execution (inspected)
Evidence: `s4-cell/src/main.rs` l.1143–1265 (plan, cap 400, strategy select, result used only for record fields) and l.1270–1293 (verifier `vcmd` independent of `oc.view`). In the final fold, no task has more than one distinct verdict (computed). The manifest declares `artificial_pressure: false` while the driver hard-codes a 400-token cap.
Retrieved: 2026-11-06T22:57Z
Implication for the spec: Describe S4 as execution-surface replay under an assignment schedule, not as a compaction experiment. Phase-4 sweeps need a participant that reads the context (OL-07; D3).
Outline delta: CORRECTS OL-01; EXTENDS OL-07.
Limitations: None.

### F7 — Intended and served policies diverge: the final fold is B 90 clear / 60 evict; 100 deviations across all epoch records
Claim: When `assess` finds nothing to reclaim, `compact()` returns a no-op record labelled `hh/evict-oldest@1` with status `Applied`. That label landed on B-arm slots.
Status: VERIFIED
Evidence kind: source inspection + recorded execution (inspected)
Evidence: `crates/hh-context/src/compact.rs` l.2311–2335 (early return with `EVICT_OLDEST_REF`). Per-packet B-arm evict counts: confirmation 30 executed + 30 inconclusive = 60, egress 30, completion 5, repairs 5 (= the packets' `served_deviations`). In the final fold, A has 150 evict and B has 90 clear + 60 evict. R5.59 contrasts by `arm_id` (`r5_49.rs` `efficacy_slots`) and never reads `served_variant`.
Retrieved: 2026-11-06T22:57Z
Implication for the spec: Records must report intended vs served per arm and name the analysis basis (assignment/ITT). A no-op should carry a distinct served label (a kernel change; route to E2/ADR).
Outline delta: EXTENDS OL-07/OL-15.
Limitations: Why 10 no-pressure B slots still served clear-tool-results was not traced (the assessment threshold sits below the 400 pressure flag).

### F8 — "300/300" is a four-epoch composite outside the frozen reattempt bound, mixing attempts, backends and network modes
Claim: 140 slots received more than one attempt (90×2, 20×3, 30×4), while the frozen bound is ≤2 per plan and ≤10 % of plans. 28 of the 60 round-0 receipts are later attempts. The final fold mixes 250 arm64-applehv slots with 50 amd64-GCE slots, and 160 `--network none` slots with 140 bridge slots.
Status: VERIFIED
Evidence kind: recorded execution (inspected)
Evidence: Slot-ref multiplicity across the four packets (computed); the manifest `reattempt` (`PARITY-TRANCHE-3.md` declaration); `backend.platform` in the completion/repairs packets. Contrast: t1's record states a mass re-measure "inside this protocol_id is not permitted — a clean O-side re-measure requires a new epoch". S4 kept `protocol_id` 9edc5327 instead. The epochs were operator-authorized (recorded verbatim in each run doc).
Retrieved: 2026-11-06T22:59Z
Implication for the spec: The record must stratify by epoch and attempt, disclose the departure (or record it as a dated amendment), and must not present the fold as a frozen first-attempt cohort.
Outline delta: CORRECTS OL-01; EXTENDS OL-15.
Limitations: Whether the operator authorizations amount to a protocol amendment is an operator question (Q-A1-2).

### F9 — Repair register: what each repair was and where it lives
Claim: The repairs fall into four classes. Both "machinery" repairs are in scratch drivers.
Status: VERIFIED
Evidence kind: source inspection + recorded execution (inspected)
Evidence: (1) **H-driver machinery (scratch)**: t1 `localhost/` tag lookup, t1 healthcheck `float('5s')`, t2 amd64-base platform pin, t3 ANSI strip. (2) **S4-driver machinery (scratch)**: `task_workdir` (repairs). (3) **Environment/runner configuration**: O-side `--n-concurrent 1`, prune and buildx prune, the diskwatch, a larger GCE backend (OOM/ENOSPC lifted). (4) **Admissions**: `declared_deps` bridge (scratch variant in egress; R5.56/ADR-0421 committed from completion on), verifier bound 360→900 (R5.57/ADR-0422), `archive_mirror` (rewritten `FROM`), `vendored_mirror` (added `COPY` plus `url.insteadOf`). Sources: the run docs' repair sections; the repairs packet `repairs`/`repair_class`; empty grep for the scratch repairs in `crates/`.
Retrieved: 2026-11-06T22:58Z
Implication for the spec: One structured repair register per record, with class, code location (committed or scratch, with commit) and affected slots. Phase 1 must land the two scratch machinery repairs in committed code with regression tests (A2/B2).
Outline delta: EXTENDS OL-15.
Limitations: None.

### F10 — R5.59's `finite_manifest_contrast` is the only landed estimator that ran, and its zero result is degenerate by construction
Claim: The committed estimator ran over byte-copy fixtures and produced observed 0 ppm with credible95 ≈±0.15/±0.12/±0.09. Those widths come from Beta(1,1) with n=5 per cell over deterministic outcomes. They are not measurement uncertainty about compaction.
Status: VERIFIED
Evidence kind: source inspection + recorded execution (inspected)
Evidence: `36dfcb3f:docs/build/runs/R5.59.md` §Efficacy; `…r5.59-efficacy.json` `strata.*` (cells c∈{0,5} of n=5; the assumption "independent Bernoulli runs within cells"); F6 invariance.
Retrieved: 2026-11-06T22:56Z
Implication for the spec: Reword through a dated note: "contrast 0 by construction (oracle participant)". This is not "null-equivalent", which §4.3 forbids. A2 must run `parity_admission`/`kl_equivalence` with the same honesty.
Outline delta: CORRECTS OL-01.
Limitations: The deterministic recompute itself is sound. I did not run the test.

### F11 — Inference cost is structured; instance-hours exist only as prose estimates
Claim: Every packet carries `cost_usd: 0.0`, `costs.usd: 0.0` and `exposure_micros: 0`. No packet has an instance-hour field. "$0.72" and "$1.41" are ≈h × $0.30/h rate-card figures, and the local VM time is uncosted.
Status: VERIFIED
Evidence kind: source inspection
Evidence: Packet `costs` members (all four S4 packets); `s4-completion-assemble.py` l.10 vs l.53/164; `S4-COMPLETION.md`/`S4-30SLOT-REPAIRS.md` §Costs; R5.59 §DEPROVISION; `~/tb-work/gce-backend.md` §Cost estimate ("~$0.268/hr" + "~$0.027/hr").
Retrieved: 2026-11-06T22:56Z
Implication for the spec: Add a structured compute-cost member (start/stop UTC, machine, rate source, estimate vs billing export) to every packet (B3).
Outline delta: EXTENDS OL-15/OL-17.
Limitations: The actual billing was not consulted (no network).

### F12 — The public surfaces lag the records; R5.59 changed none of them
Claim: `docs/evidence.md`, `docs/status.md` and README trail the 270/300, 300/300 and R5.59 states. The study card contradicts itself, and README has no link to the record.
Status: VERIFIED
Evidence kind: source inspection
Evidence: §3 table; `git diff --stat 75883daa 09fe8523 -- docs/evidence.md docs/status.md README.md` is empty.
Retrieved: 2026-11-06T22:59Z
Implication for the spec: Phase 1 must transition these cards (through `check-position.sh`) to the A2 record's corrected language. The new README link is OL-01's deliverable.
Outline delta: EXTENDS OL-12.
Limitations: I did not run `check-position.sh`.

### F13 — The TB 2.0 → `terminal-bench-core==0.1.1` substitution is unrecorded, and the catalogue identity is mislabelled
Claim: Spec §4.1 names TB 2.0 (89 tasks). Execution used core 0.1.1 (80 tasks) with no ADR, readout or ticket record. `catalogue_identity` is a 64-bit FNV-1a over the sorted task-ID list, printed as `sha256:…`, so it pins names, not task content.
Status: VERIFIED
Evidence kind: source inspection
Evidence: §4 grep; `parity-driver/src/main.rs` l.43–61. REQ-E-011's literal row is in `docs/build/planning/2026-10-30_evidence-coverage.json:2131`.
Retrieved: 2026-11-06T22:59Z
Implication for the spec: Record the substitution by ADR/amendment (H1). Phase-2 identities must be content digests with a correct label.
Outline delta: EXTENDS OL-15.
Limitations: The "TB 1.0" identification of core 0.1.1 is taken from the brief (A-5) and was not verified externally.

### F14 — Label hygiene: S3/S4 collide with spec scenario IDs, and "live-executed" is misapplied
Claim: Records use "S3" for the parity predicate and "S4" for the confirmation cohort (`s3_unmet`/`s4_unmet`). Spec §1.1 defines S3 = compaction study and S4 = operations. GATE-G19 labels these packets `live-executed` without `hh-live-evidence/1` records.
Status: VERIFIED
Evidence kind: source inspection
Evidence: spec §1.1; `study.rs` field docs ("`s3_unmet` — the tranche verified parity"); GATE-G19 Evidence list; `ls docs/build/reports/live/` has no S3/S4 entries.
Retrieved: 2026-11-06T22:59Z
Implication for the spec: The methodology record defines its terms ("parity tranche", "confirmation cohort") and uses the repo's evidence classes (recorded execution, internal).
Outline delta: EXTENDS OL-12.
Limitations: The study-packet field names are frozen history; only new prose can disambiguate them.

### F15 — The drivers are not reproducible as archived
Claim: Path-deps target the moving shared worktree. Only the final patched sources survive. Inputs, sockets and output paths are absolute. Raw records live only in `~/tb-work`.
Status: VERIFIED
Evidence kind: source inspection
Evidence: §5 table; the three `Cargo.toml`s; assembler `REPO`/`OUT` constants.
Retrieved: 2026-11-06T22:57Z
Implication for the spec: The A2/B2 driver promotion must commit sources with history, freeze inputs as fixtures, hash the raw records, and stamp each packet with the producing commit.
Outline delta: EXTENDS OL-01/OL-15.
Limitations: `Cargo.lock` files pin external crates, but HH crates resolve to whatever was checked out at build time.

## Answer and completion assessment

**What executed.** Three S3 tranches, all `tb` oracle vs a scratch Python podman runner, R=1, no committed estimator: `infeasible`, `infeasible`, then `verified` 30/30 by exact agreement on 26 executed and 4 build-stage pairs. Then a 300-slot S4 cohort through real HH cells, driven by scratch drivers, over four epochs, two backends and two network modes, with 140 slots re-attempted. Its oracle participant makes the A/B compaction arms inert: per-task outcomes are invariant, and 60 final B slots served a no-op evict label. Finally, R5.59 ran the committed `finite_manifest_contrast` (0 by construction) and a records-in study fold. Inference cost was $0, recorded structurally. Compute was ≈$1.41, estimated in prose only.

**What the records support.** (a) HH cells can provision, mediate, seal and observe 300 gold-script executions with retained denominators. (b) `tb` and a reference podman runner agree on 26 executed TB-core-0.1.1 tasks on arm64. (c) Honest disposition records across epochs.

**What they do not support.** HH-cell parity with `tb`. Any §4.4 admission or KL equivalence. Any compaction-efficacy statement. A single frozen first-attempt cohort.

**Completion.** The row is complete. No technical blocker. The open uncertainties are operator dispositions (below).

## Open questions

- **Q-A1-1** (operator decision; owner A2/H1; affects A2, B2, D2): Can phase 1 establish HH-cell ↔ `tb` agreement records-in, by comparing S4 HH-cell receipts against t3 O verdicts on the same 30 tasks? That comparison is confounded by epochs, backends and egress. Or does it need a fresh $0 tranche meeting R≥5 on HH cells?
- **Q-A1-2** (operator decision; owner H1): Are the egress, completion and repairs epochs, run under one `protocol_id` beyond the frozen reattempt bound, to be recorded as a dated protocol amendment? Or does the record present them only as epoch-stratified supplements?
- **Q-A1-3** (operator decision; owner H1): Should an ADR record the TB 2.0 → core-0.1.1 substitution before any publication of the phase-1 record?
- **Q-A1-4** (no decision; owner A2): Dated correction notes for the R5.59 efficacy wording ("null-equivalent") and for GATE-G19's `live-executed` layer labels.
- **Q-A1-5** (no decision; owner A2/F2; outside A1's subject): S1 drift. LEDGER records DF-CAP5.1-1 closed by an executed S1 packet (2026-11-05). The R5.59 study fold still reads `useful_s1: failed`, and status/README still describe S1 as failed with three deferrals open, while `deferrals-open.sh` returns empty.

## Spec requirements emitted

| ID | Requirement | Rationale / source | OL | Acceptance |
|---|---|---|---|---|
| REQ-XA-001 | The methodology record states, per tranche, that S3 compared `tb run -a oracle` with a non-HH reference podman runner, and that HH cells were not a parity surface. | F1 | OL-01 | Review plus `check-position.sh` phrase pin |
| REQ-XA-002 | The record states R=1 for S3, that `verified` is the tranche-local exact-agreement rule, and the outcome of running `parity_admission` records-in (its typed refusal committed). | F2 | OL-01, OL-15 | Committed admission packet from the landed function; test fails if relabelled |
| REQ-XA-003 | Any KL figure is either computed by `kl_directional_bound`/`kl_equivalence` with a pre-registered ε, or labelled a closed-form endpoint. | F3 | OL-01, OL-06 | Packet field `estimator` plus a recompute test |
| REQ-XA-004 | Parity records report executed-pair agreement separately from build-stage agreement, and disclose image reuse and platform. | F4 | OL-01, OL-06 | Packet members checked by the study fold |
| REQ-XA-005 | The S4 record states that the oracle participant never consumes the compacted view, that outcomes are invariant per task, and that the efficacy contrast is 0 by construction. It is never called "null-equivalent" or efficacy evidence. | F6, F10 | OL-01, OL-07 | `check-position.sh` forbidden-phrase mutation |
| REQ-XA-006 | Every arm-bearing packet carries intended vs served policy counts per arm and names the analysis basis (assignment vs served). | F7 | OL-07, OL-15 | Fold check: counts equal recomputation from `slots[]` |
| REQ-XA-007 | Composite cohorts disclose per-slot attempt index, epoch, backend/platform and network mode. A "first subject" means attempt 1 only; departures from frozen reattempt bounds are recorded as dated amendments. | F8 | OL-01, OL-15 | Fold test recomputing the attempt histogram (90/20/30) |
| REQ-XA-008 | One structured repair register per record: class ∈ {machinery-committed, machinery-scratch, environment, admission}, code location with commit, affected slots. | F9 | OL-15 | Schema check on packets |
| REQ-XA-009 | Scratch machinery repairs (ANSI-strip verdict channel, declared-workdir exec) land in committed code with regression tests that fail without them. | F9, F1 | OL-01 | `cargo test` regression battery |
| REQ-XA-010 | Each packet records its producing driver commit. Drivers are committed with history, use no absolute paths, and take inputs from committed fixtures. | F15 | OL-01, OL-15 | Clean-clone recompute script |
| REQ-XA-011 | Raw per-slot/per-pair records are committed or content-addressed by a committed hash manifest, so packets are recomputable. | F15 | OL-01 | Recompute from the hashed raw set reproduces the packet |
| REQ-XA-012 | Every packet carries a structured compute-cost member (instance id, machine, start/stop UTC, rate source, estimate vs billing export), separate from inference cost. | F11 | OL-15, OL-17 | Schema check; teardown receipt cross-reference |
| REQ-XA-013 | The benchmark-edition substitution (TB 2.0/89 → core 0.1.1/80) is recorded by ADR/amendment. Catalogue identities are correctly labelled content digests. | F13 | OL-15 | ADR present; identity recompute test |
| REQ-XA-014 | `docs/evidence.md` study/operations cards, scenario rows, the unmet block, `docs/status.md` §Round-5 and README are reconciled with the latest record and a README link is added, without weakening pinned `unmet` clauses. | F12 | OL-01, OL-12 | `check-position.sh` (+`--head`) and its mutation battery |
| REQ-XA-015 | The record uses repo evidence classes (not `live-executed` without `hh-live-evidence/1`), defines "parity tranche"/"confirmation cohort", and distinguishes them from spec scenarios S3/S4. | F14 | OL-12 | Review; `check-live-evidence.sh` |
| REQ-XA-016 | Receipts disclose the verifier channel per slot (verbatim vs vendored), where parsing ran, and environment departures (derived images, absent sidecars, undeterminable probe). | F5 | OL-01 | Packet members plus a fold count check (200/100) |

## Handoff

- **Dependencies resolved:** none needed (wave-1 row).
- **Proposed follow-up:** A2 should consume F1–F11 and REQ-XA-001…016 directly. B2 should treat `s4-cell` and `s4-orch.py` as the prototype to replace (§5), keeping verdict parsing inside the committed runner. B3 owns REQ-XA-012. H1 owns Q-A1-2/Q-A1-3 and the substitution ADR. E2 should take the no-op served-label issue (F7).
- **Conflicts:** None with existing notes. This note contradicts brief §2's account as tabulated in §2, which A-2 anticipated.
- **Capability/portfolio dispositions:** The executed record supports "HH-cell execution-surface replay (internal, recorded execution)". It does not support "parity verified for HH" or "compaction efficacy".
