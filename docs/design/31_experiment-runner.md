# 31_experiment-runner — Committed experiment runner and TB 2.1 execution surface
Stream: B
Researched: 2026-11-06 (inspection 2026-11-06T23:16Z–23:27Z)
Author: design worker for ledger row B2 (Claude Code subagent, claude-opus-5-5), dispatched by the experiments-program coordinator
Ledger rows covered: B2
Repository baseline: main 09fe8523. Inspected at worktree HEAD 688c7f0a on `claude/experiments-spec`; `git diff --stat 09fe8523 HEAD -- crates spec scripts docs/adr` is empty, so every code locator below holds at 09fe8523. Untracked program notes 25/27/28 and `docs/design/33_model-mediation.md` (C2, written concurrently) were read as inputs.
Confidence overall: medium. Repository facts are high-confidence (source read, locators cited). Everything this note proposes is a design proposal and is UNVERIFIED by construction: nothing was built, compiled or run, and no podman, `harbor`, `tb` or network command was executed. The three riskiest assumptions are named where they occur: the per-slot process/store topology (F4), the CONNECT tunnel leg on the egress mediator (F11), and the bootstrap-failure classifier (F7).

## Scope, inputs and method

Row B2 asks for one committed runner that every phase uses; the egress profile a TB 2.1 cell runs under; the verdict channels; the timeout admission; the $0 oracle re-proof; and the task-selection hook. The binding inputs, all read in full, were:

- `docs/research/24_executed-record-audit.md` (A1);
- `docs/research/25_tb21-harbor.md` (B1), including the Appendix A census;
- `docs/design/33_model-mediation.md` (C2), read after the coordinator's note of 2026-11-06T23:27Z: §1.1 topology, §1.5, F1, F2, F5, §3, §4, open questions and handoff;
- `docs/research/27_harness-survey.md` (C3) F8–F14 and `docs/research/28_reference-harness-anatomy.md` (E1), findings index and REQ-XE-001…014, skimmed.

Program files read: the brief (§3 addenda); ledger row B2, §Q (Q-X5, Q-X8, Q-X17, Q-X30, Q-X31, Q-X32), the Execution contract and standing limits; `docs/research/CONVENTIONS.md`; and the OL-02/09/16 rows of `docs/experiments-OUTLINE_TRACE.md`. Root `AGENTS.md` and `crates/AGENTS.md` were read. No crate has a nested `AGENTS.md` (`find crates -name AGENTS.md` returns only `crates/AGENTS.md`).

**Repository sources read (09fe8523):**

| Area | Locators |
|---|---|
| hh-env | `egress_profile.rs` 1–186 (whole); `verifier_bound.rs` 1–311 (whole); `driver.rs` 1–80, 812–890 (`start_helper`), 2073–2200 (`set_phase`), 3557–3582 (`env_upload`), symbol index; `egress.rs` 1–120, 390–640 (`gate`/`GateAllow`), symbol index; `record.rs` 123–193 (`ImageRef`, `ResolvedImage`); `helper.rs` 127–160 (`spawn`); `study.rs` 1–50 plus symbol index; `Cargo.toml` |
| hh-helper | `bin/hh_helper.rs` (whole); `podman.rs` (whole); `protocol.rs` 160–230 (exec/read/detach verbs); `Cargo.toml`; `tests/r5_56.rs` 1–40 |
| hh-containment | `policy.rs` 187–206, 380–485, 671–690, 791–816; `egress.rs` 1–80 plus symbol index; `backend.rs` 1–60, 200–240, 630–700 |
| hh-eval | `central_protocol.rs` 1–330, 600–700 |
| hh-lab | `experiment.rs` 1–60, 269–300, 444–565, 846–886, 2369–2500; `expand.rs` 1–40 plus symbol index |
| hh-experiment | `engine.rs` 1–60, 1909–2000; crate file list and dependencies |
| hh-embed | `experiment_ops.rs` 1–80, 690–916; `envops.rs` 1–40; `service.rs` method table (grep) and 685–710 |
| hh-bench | `oracle.rs` 1–200, 380–420; `grade.rs` 1–90; `infra.rs` 1–40, 133–150 |
| hh-ontology / hh-ledger | `control.rs` 249–280, 632–650; `eval.rs` 2243–2249; `classes.rs` class-name grep; `store.rs` 255–290 |
| Spec / ADRs | `spec/CANONICAL_SPEC.md` lines 1230, 1833, 3733, 4889, 5281, 5286 (grep hits, read in context); `docs/evidence-and-adoption-spec.md` §4.4 (l.282–320) and §9.2–9.3 (l.597–645); ADR-0421 and ADR-0422 (whole); ADR-0363 (l.1–30) |
| Records | `docs/build/reports/2026-11-06-s4-30slot-repairs.json`: top-level keys and `slots[0]`, parsed with `python3 -I` |

**Scratch drivers read** (archive `~/tb-work/scratch-archive-2026-11-06/`, read-only):
- `s4-cell/src/main.rs` 1–200, 217–364, 684–726, 816–1000, plus a function and lifecycle grep (1066–1401);
- `s4-orch.py` 1–60, 274–470;
- `s4-repairs-orch.py` 1–40 plus a symbol grep;
- `parity-t3-pipeline.sh` (whole).

Nothing cited below goes beyond these reads.

## 1. Design

### 1.1 Where the runner lives (summary of F2–F4)

```text
                    ┌──────────────────────────────── hh-embed/1 (unchanged) ──┐
 hh (CLI) ── K-2 ──►│ lab.experiment.{register,expand,open,next,claim,launch,  │  read-only use only
                    │ settle,pause,resume,close} · env.* · kernel.*            │  (status, describe)
                    └──────────────────────────────────────────────────────────┘
 hh-runner  (NEW workspace crate `hharness-runner`, lib + bin `hh-runner`; design proposal)
   coordinator process ── owns the experiment store; drives hh-experiment::ExperimentEngine
   │   (next/claim/launch/settle; ReattemptPolicy; OrderPlan; pools) — no second scheduler
   ├── slot worker process ×k ── one per in-flight slot attempt; owns a slot store; EnvDriver +
   │     hh-helper (podman); hh-bench OracleRunner (verdict); writes hh-experiment-slot/1
   ├── hh-egress-mediator process (NEW bin, hh-env) ── phase egress run; per-slot unix listeners;
   │     EgressMediator.gate/forward (+ tunnel leg); registry allowlist          [B2 owns]
   ├── hh-model-proxy process (hh-hosting bin; C2 §1.1) ── phase proxy run; per-slot sockets [C2 owns]
   └── harbor_o ── `harbor run …` subprocess (O surface) + read-only job ingest
 depends on: hh-experiment, hh-lab, hh-eval, hh-bench, hh-env, hh-helper, hh-ledger, hh-budget
 (all downward; no crate depends on hh-runner; hh-embed may wrap it later — not proposed here)
```

### 1.2 Slot lifecycle

```text
 coordinator: engine.next ─► claim(run_plan_id) ─► launch → {subject run_id, attempt_no, slice}
   │  spawn slot worker(spec = frozen plan row + pins + admitted deadlines + egress decl)
   ▼
 S0 ADMIT    task bytes == pinned digest · image digest resolvable locally · deadline admission
             · egress profile composed + validate() · C2 slot slice reserved
             ── any refusal ─► withheld{admission:<typed reason>}  (no cell; nothing fabricated)
 S1 PROVISION host: podman image exists @sha256 (pull by digest only) → EnvDriver.provision
             + attach(local_container; --network none; image WORKDIR; cpus/memory limits;
             relay = main process; sockets bind-mounted) ── fail ─► inconclusive{infra:provision:<c>}
 S2 SETUP    set_phase(setup) · C4 install hook (prebuilt layer ⇒ verify-only check)
             · oracle participant only: upload solution/ → /solution
 S3 AGENT    set_phase(agent) → mediator loads sealed agent allowlist; C2 slot listener open
             · launch participant (deadline = [agent].timeout_sec × m_agent)
             · end-cause ∈ {exited(rc), agent_timeout, budget_refused, crashed}
             · cell died ─► inconclusive{infra:environment_died}
             · C2 slot listener closed at agent end (model route unreachable from here on)
 S4 SEAL     post-agent seal: podman diff manifest + changed-file digests (budgeted) + /work pin
             + process list → action.environment.snapshot{purpose: submission_seal}
 S5 VERIFY   set_phase(verify) → mediator loads verify allowlist · rm /logs/verifier/reward.*
             · upload tests/ → /tests (kernel-mediated, never via /work)
             · bash /tests/test.sh, cwd = WORKDIR (deadline = [verifier].timeout_sec × m_ver)
             · download /logs/verifier/{reward.json,reward.txt,ctrf.json,test-stdout.txt}
             · VerdictChannel.classify (F6/F7) via OracleRunner (shared isolation)
             └─ oracle_failure{verifier_unavailable:*} ─► regrade in the SAME live cell
                (≤ InstrumentPolicy.max_attempts) ─► still unavailable ⇒ stays inconclusive
 S6 RECORD   hh-experiment-slot/1 blob in the slot store · slot run finished{stop}
 S7 TEARDOWN EnvDriver.teardown · mediator + proxy slot listeners closed · own-label sweep
             · teardown receipt (container gone, sockets unlinked)
   ▼
 coordinator: bind slot-store head → subject run finished → engine.settle → RunOutcome
   scored ⇒ final · infrastructure_failure ⇒ replan as a NEW attempt (≤ policy bounds;
   failed attempt preserved) · oracle_failure ⇒ never re-runs the participant
```

### 1.3 Egress-profile composition (owned by B2; reconciles C2)

There is one cell egress profile for paid TB 2.1 slots: **`allowlist_mediated`**, owned by B2 under one ADR (ADR-B2-EGRESS, §5). C2's `model_proxy` channel (C2 F5, ADR-C2-C) is one component of it, not a separate profile. The cell stays on podman `--network none` for its whole life, with `net.mode = mediated`. It reaches the host only through bind-mounted unix sockets, each relayed from an in-cell loopback port by one relay binary. Per-phase differences are host-side policy, switched by the existing `set_phase` sealed schedule (F10). The container is never re-networked mid-slot.

| Phase | Cell network | `net.mode` (policy) | Reachable destinations | Entry source (frozen before paid runs) | Enforcement | Logging | Residuals declared |
|---|---|---|---|---|---|---|---|
| provision (host) | n/a (no cell yet) | n/a | registry `docker.io` for `alexgshaw/<task>@sha256:…` | digest resolved at selection freeze (F14) | host-side pull by digest only; tag pulls refused | pull receipt (ref, digest, platform, bytes) | none (host act, outside the cell) |
| setup | `none` | `mediated` | nothing for a prebuilt harness layer; otherwise C4's declared install hosts | C4 admission | mediator `setup` schedule | `security.egress.*` rows | as agent, if any host is granted |
| agent | `none` | `mediated` | (a) **model**: `/run/hh/model.sock` ⇄ relay `127.0.0.1:P` → C2 proxy; (b) **registry**: `/run/hh/egress.sock` ⇄ relay `127.0.0.1:3128` (HTTP forward / CONNECT) → B2 mediator, allow = base registries ∪ instruction-derived hosts ∪ granted oracle-observed hosts | base list per image family (protocol); static scan of `instruction.md`; oracle-observe pass (§1.6) | `--network none` (no route); mediator `decide_egress` per CONNECT/request; C2 token gate | every request → `security.egress.requested/decided` on the phase egress run (slot-tagged); C2 `model.call.*` on the phase proxy run | `unix_socket` ×2 (owner, closer); `tls_opaque` (CONNECT not terminated); `approved_host_body` (bodies on allowed shared hosts such as github.com); `dns_names` (mediator resolves); `side_channel` (per-socket attribution: every in-cell process is attributed to the slot's agent-phase effect) |
| verify | `none` | `mediated` | registry channel only, allow = verifier hosts (apt mirror for the image's base, `astral.sh`, uv/Python-build download hosts, `pypi.org`, `files.pythonhosted.org`, and per-task GitHub for build-cython-ext and filter-js-from-html); model socket closed | static scan of `tests/*` ∪ oracle-observed verify-phase hosts | as agent | as agent | as agent, minus the model socket |
| teardown | removed | — | — | — | helper stops and removes the container; listeners unlinked | teardown receipt | closed |

The cell env carries routing *hints* only: `http_proxy`/`https_proxy` (lower and upper case) = `http://127.0.0.1:3128`, `no_proxy=127.0.0.1,localhost`, plus C2's base URLs and slot token. The containment backend records these proxy variables as the stratified `proc.env: proxy_env_is_not_mediation` lowering loss (`backend.rs` 682–689). That is correct: the enforcement is `--network none` plus the socket grants, never the variables. Every slot under this profile carries the loss uniformly, so it is disclosed rather than hidden.

The `declared_deps` bridge profile (ADR-0421) stays admitted. It serves as the canonical-posture (`public`) comparison in the oracle re-proof (§1.6) and as the posture for any arm D1 declares canonical. It is never used silently in place of `allowlist_mediated`.

### 1.4 Slot-record envelope `hh-experiment-slot/1` (sketch)

This envelope extends the S4 `hh-s4-slot/1` member set (repairs packet `slots[0]`). It keeps that set's spellings where they carry over: `egress_profile`, `cell_network`, `policy_net_mode`, `prior_disposition`, `supplement_epoch`, `repair_class`, `seal`, `sentinel_intact`, `observer_survived`, `probe`. C2 owns `costs.model`; B3 owns `costs.compute` (C2 §1.5). Both are folds referenced here, never re-specified.

```text
hh-experiment-slot/1 {
  identity:  { experiment_id, protocol_id, selection_id, run_plan_id, cell_id, arm_id, task_id,
               replicate_index, first, pool_keys[], attempt_no, epoch, supersedes_attempt?,
               subject_run_id, slot_store{root_digest, run_id, head_hash},
               runner{commit, binary_sha256}, helper{binary_sha256}, relay{binary_sha256} },
  task:      { dataset{git_commit, hub_ref, task_digest}, task_tree_sha256,
               image{ref: "docker.io/alexgshaw/<t>@sha256:…", digest, platform, workdir},
               declared{cpus, memory_mb, storage_mb, agent_timeout_sec, verifier_timeout_sec} },
  participant:{ participant_ref, kind: oracle|hosted, harness{name, version, layer_digest}?,  // C4
               config_ref? },
  cell:      { backend{podman_version, rootful|rootless, host, arch}, limits_applied{cpus, memory},
               egress{ profile, cell_network, policy_net_mode, policy_ids_by_phase{},
                       residuals[], mediator{run_id, allowed_by_host{}, denied_by_host{},
                       bytes_up, bytes_down}, model_socket: bool } },
  phases:    [ { phase: setup|agent|verify, started_seq, ended_seq, wall_ms,
                 deadline{source: task_declared|legacy_cap, declared_sec, multiplier_ppm,
                          enforced_ms, admission_ref}, end_cause, exit_code?, dispatch_outcome } ],
  verdict:   { channel: harbor_reward_file/1 | tb10_pytest_log/1,
               files{name → {sha256, bytes, blob_ref}}, reward_raw, reward_ppm?, success?,
               ctrf{tests, passed, failed, skipped, errors}?, classification, flags[],
               grade_attempts[{attempt_no, invoked_ref, verdict_ref}] },
  disposition: "executed{resolved|unresolved}" | "inconclusive{<cause>}" | "withheld{<reason>}"
               | "failed{<reason>}", prior_disposition?, repairs[{class, location, commit}],
  integrity: { seal, seal_ref, sentinel_intact, observer_survived, probe },
  trajectory:{ atif_ref?, atif_schema?, proxy_transcript_ref? },                   // C4 / C2
  costs:     { ref: hh-slot-costs/1 — model (C2), compute (B3) }
}
```

O-surface trials are never forced into this envelope. They get a sibling ingest record, `hh-harbor-trial/1` (§1.7), which carries their own identity and the same `task`, `verdict` and `disposition` vocabulary.

### 1.5 Verdict channels

**`harbor_reward_file/1` (TB 2.1).** The channel is an `OracleExecutor` implementation (`hh-bench/src/oracle.rs` 181) whose `execute` runs S5 inside the live cell. It reports an `OracleObservation`:
- `stdout` is the canonical `{"reward": <ppm>}` document derived from the reward file;
- the raw reward-file bytes and their digest are kept verbatim in the attempt record;
- the derivation is labelled with the channel name.

`grade`/`parse_typed_reward` are reused unchanged (`grade.rs` 55–70). The rules, in order:

1. `reward.json` is preferred over `reward.txt`, matching Harbor (B1 F6). A `reward.json` must be an object with exactly one numeric `reward` member. Several keys are refused as `oracle_failure{verifier_unavailable:reward_json_multi_key}`, because the D1 estimand is a scalar.
2. `reward.txt` is a trimmed decimal in [0, 1] with at most six fractional digits. It maps exactly to ppm. Anything else, including an empty or absent file, maps to `oracle_failure{verifier_unavailable:reward_unparseable|reward_missing}`. Harbor's `RewardFileNotFoundError`/`RewardFileEmptyError` are the same classes (B1 F13).
3. Success is declared per protocol. The TB 2.1 default is `reward_ppm > 0`, which equals Harbor `is_success` (B1 F16). All 89 tasks write 0 or 1, so the binary and ">0" rules coincide on this dataset. Non-binary rewards are retained as ppm for D1.
4. A verifier deadline lapse maps to `oracle_failure{verifier_unavailable:verifier_timeout}`, which matches Harbor's `VerifierTimeoutError` being an invalid reward.
5. The bootstrap rule is F7.
6. Isolation is spelled `shared` with `tamper: n/a{no_detector}`, as `oracle.rs` 34–38 requires. Harbor TB 2.1 verification is shared (B1 F6), so the record can never claim a separate verifier.

**`tb10_pytest_log/1` (TB 1.0; phase-1 reproducibility only).** This is the committed form of the scratch channel: the `strip_ansi` regex set from `s4-orch.py` 32–43 plus a pytest short-summary parse equivalent to tb's `PytestParser`, run under the image WORKDIR (`task_workdir` repair). It implements the same `VerdictChannel` trait and is selectable only when the dataset pin is `terminal-bench-core==0.1.1`. A2 owns the historical drivers and records. B2 supplies the channel so that A2's records-in recompute and any Q-X24 fresh TB-1.0 tranche run committed code (REQ-XA-009). The channel is refused for any TB 2.1 slot.

### 1.6 The $0 oracle re-proof on TB 2.1 (phase-2 precondition)

| Element | Design |
|---|---|
| Question | Does the HH cell surface execute TB 2.1 tasks faithfully: same image, same task bytes, same verification semantics as Harbor, and under the allowlist profile that paid runs will use? |
| Participant | The task's own `solution/solve.sh` as Harbor's `oracle` agent runs it: upload `solution/` to `/solution`, run `bash /solution/solve.sh` with `DEBIAN_FRONTEND=noninteractive` plus `solution.env` (empty on TB 2.1) under the agent timeout, stdout to `/logs/agent/oracle.txt` (B1 Harbor table). |
| Surfaces | **O**: `harbor run -p <checkout@12b75656>/tasks -i <task…> -a oracle -e podman -k R -n <packed> -r 0 --no-force-build --jobs-dir <scratch> --job-name <protocol/epoch-derived>`, never `--upload`. **H-pub**: HH cells under `declared_deps` (canonical `public` posture, ADR-0421). **H-allow**: HH cells under `allowlist_mediated` with the frozen allowlist. |
| Passes | (1) **observe**: H-pub with the mediator in record-only mode. The cell has a bridge, so the record is the host's view of connections; DNS and host names are logged by a host-side capture declared in B3's recipe. This yields observed hosts per phase. If no capture is admitted, H-allow is run with a permissive `ask→log` rule instead. (2) **freeze**: allowlist = base ∪ static scan ∪ granted observed hosts, committed before any paid slot. (3) **prove**: O, H-pub and H-allow at R each. |
| R and N | R = 3 per surface per task (protocol parameter, minimum 2). The oracle is deterministic in intent but not in effect: network fetches, mirrors and timing make flake possible, and R = 1 cannot tell flake from defect (A1 F2's lesson). R = 3 does **not** meet a §4.4 parity floor and is not presented as one. N = selected tasks × 3 surfaces × R. The stage-1 population is the Q-X31 cheapest stratum, sized by D1 and quoted by B3. |
| Pairing | By `(task, repeat)`. Order is interleaved by task blocks (O block, then H blocks, seeded), so time drift is symmetric. No independence or CRN claim is made. |
| Agreement | Per task: `admit` iff O = 3/3 reward 1 **and** H-pub = 3/3 **and** H-allow = 3/3. |
| Discordance classes | `o_fail`: O < 3/3. The task is excluded on the outcome-blind basis `runner_infeasible:oracle_fails_on_O{harbor_cause}`; flaky O (1–2/3) is the same basis with sub-reason `flaky` (D1 may instead keep it as a covariate, OQ-3). `machinery`: O 3/3, H-pub < 3/3. A defect to diagnose and repair; never an exclusion (B1 F19). `allowlist_gap`: H-pub 3/3, H-allow < 3/3. Read the mediator's denied hosts; a host is added only by a recorded amendment before paid slots, then that task is re-proven. |
| Discordance register | One row per discordant task: surfaces, repeats, reward/ctrf/stdout digests, F7 classification, denied hosts, Harbor exception, image-digest equality, walls, hypothesis, repair `{class ∈ machinery-committed, machinery-scratch, environment, admission; location; commit}` (REQ-XA-008), re-run epoch. Rows are append-only. |
| Label | "Oracle execution-surface re-proof (internal, recorded execution, `parity: none`)". `parity_admission` refuses oracle-transport equality as `InauthenticEvidence::OracleTransportEquality` (A1 §4, `parity_admission.rs` 190–194, 856–862). The record runs it and commits that typed refusal next to the result. `live-executed` is claimed only if the ticket registers a `scripts/live-verify.sh` leg producing `hh-live-evidence/1` (ADR-0363); otherwise the label is "recorded execution". |

### 1.7 Harbor O-surface invocation and ingest

The runner writes the Harbor job config itself (committed into the record) and invokes `harbor` as an external pinned tool. The pin is a stable tag (`v0.25.0` = `fdba28fc`, B1 Q-B1-5), recorded as `runner_ref` with `harbor --version` output.

Image identity is held equal without editing `task.toml`:
- pull each image by digest and tag it locally with the `task.toml` tag;
- run with `--no-force-build`;
- after the job, assert that `podman image inspect <tag>` resolves to the pinned digest.

A mismatch marks every trial of that task `withheld{image_drift}`. A `-p` local checkout carries the per-task bytes. Their tree digests are checked against `dataset.toml` before the job (REQ-XB-002).

Ingest is read-only. The runner walks `jobs/<job>/`, hashes every file, copies the bytes into the blob pool, and emits `hh-harbor-ingest/1`, which contains:
- the job's `config.json`/`lock.json`/`result.json` digests;
- one `hh-harbor-trial/1` per trial directory, carrying the trial name, task, attempt index, reward file(s), `ctrf.json`, `test-stdout.txt`, `exception.txt`, `trajectory.json` with its ATIF version, and the Harbor cause mapped by a table pinned at the Harbor commit (B1 REQ-XB-012).

The job directory is never mutated. `tb run` follows the same pattern (`hh-tb-ingest/1` over tb's run dir) and is invoked only by A2's TB-1.0 path.

## 2. Findings

### F1 — No committed forward runner exists; the scratch drivers own lifecycle, verdict, retry and resume, and several of their choices must not be carried forward
Claim: Every S4 slot was driven by scratch `s4-cell` plus Python orchestrators. Their lifecycle, verdict parsing, retry and resume logic exists nowhere in `crates/`. Six of their choices are wrong for a committed runner:
- (a) each attempt **deletes** its store root, then reopens it under `ManualClock::at(1_000)` and `SeqIds`;
- (b) resume is "the output JSON file exists → skip";
- (c) tests are staged into the `/work` mount before the oracle runs, so they are visible to the participant;
- (d) the cwd is hard-coded to `/app` until the repairs epoch;
- (e) a 1,800,000 ms wall cap is hard-coded;
- (f) `sweep_hh_env_containers` removes every `hh-env-*` container, which is unsafe under concurrency.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `s4-cell/src/main.rs` 217–231 (`remove_dir_all` then `Store::open_with(… ManualClock::at(1_000), SeqIds …)`), 879–886 (`w > 1_800_000` → `wall_cap_breach`), 940–962 (solution, `run-tests.sh` and `tests/` copied into the attempt workspace before provision), 1276 (`cd {task_cwd} … bash /tests/run-tests.sh`), 547 (`sweep_hh_env_containers`). Spec fields 76–133 (`task_workdir` "empty = the prior `/app` default"). `s4-orch.py` 385–388 (`if out_file.exists(): … skip`), 317–349 (Python `PytestParser` verdict outside HH). A1 F5, F9, F15.
Retrieved: 2026-11-06T23:19Z
Implication for the spec: The runner replaces these drivers wholesale; it does not port them. Durable state comes from the ledger. Tests stay absent until verification. The cwd is the image WORKDIR. Deadlines come from the admission (F13). Containers are swept by the experiment's own label only.
Outline delta: EXTENDS OL-02 — the execution surface must be committed machinery; CONFIRMS A1's REQ-XA-009/010.
Limitations: Only the final patched versions of the drivers survive (A1 F15), so earlier-epoch behaviour is not inspectable.

### F2 — The Lab already owns durable scheduling; `launch` opens only the subject run and its lease, so the missing piece is the cell driver
Claim: `ExperimentEngine` keeps all scheduling truth in the experiment run's ledger stream and re-projects its view after any crash. It hands out plans in recorded order with lease claims and reconcile-before-dispatch expiry, opens the subject run at `launch`, settles exactly once, and replans infrastructure failures under the `ReattemptPolicy`. `lab.experiment.*` exposes all of it on hh-embed/1. `run_plan_id` keys `(experiment, arm, configuration_version, task, replicate)`. Nothing drives a cell between `launch` and `settle`.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-experiment/src/engine.rs` 1–43 (engine contract: S-1 re-projection, KP-E1…E5, `claim`/`claim_expired`, `launch` opens the subject `agent` run, `settle` exactly-once, replans per `ReattemptPolicy`/`CancelPolicy`), 1909–1975 (`settle` requires `lifecycle.run.finished` and derives `OutcomeClass` via `derive_outcome_class`). `crates/hh-embed/src/experiment_ops.rs` 742–789 ("the opened subject run + its writer lease (the driver writes with it)"). `crates/hh-lab/src/experiment.rs` 444–565 (`Pool{key: model_snapshot:<ref>|environment_class:<name>|…, limit}`, `SchedulingPolicy`, `ReattemptPolicy{max_per_plan, max_fraction_of_plans_ppm, error_classes_included}`), 2369–2387 (`run_plan_id`), 2450–2500 (`RunPlan`, `OrderPlan`, `CellPlan`). `crates/hh-ledger/src/classes.rs`: all `measurement.experiment.*` classes are registered.
Retrieved: 2026-11-06T23:21Z
Implication for the spec: The runner is a **driver over the engine**. It adds no scheduler, store or retry counter of its own (CC1). Plans, claims, attempts and settlement stay engine records. The runner contributes the cell lifecycle, the verdict and the slot receipt. Arms (harnesses, oracle) are `ArmSpec`s, and replicates are `replicates_per_cell`.
Outline delta: EXTENDS OL-16 — typed dispositions and retry bounds are enforced by the engine's existing records, not by runner convention.
Limitations: Whether `register` accepts the phase-2 arm shape (hosted participants, `limits_enforced: partial`) without refusal was not exercised.

### F3 — Placement: a new `hharness-runner` crate and binary, above hh-env/hh-experiment/hh-bench and below hh-embed; not the CLI, not hh-env, not hh-experiment
Claim: The runner should be a new workspace member, `crates/hh-runner/` (package `hharness-runner`, library plus binary `hh-runner`). It depends downward on hh-experiment, hh-lab, hh-eval, hh-bench, hh-env, hh-helper, hh-ledger and hh-budget. It spawns C2's `hh-model-proxy` and B2's `hh-egress-mediator` as processes; it does not link `hharness-hosting` (C2 F2: no crate may depend on hosting).
Status: UNVERIFIED (design proposal). Proposed validation: `cargo tree -p hharness-runner` shows no edge into hh-embed, hh-cli or hh-hosting; `bash scripts/check-removability.sh` stays green.
Evidence kind: design proposal over source inspection
Evidence: Dependencies, from each `Cargo.toml` `[dependencies]` list:

| Crate | Depends on (`hharness-*`) | Excludes |
|---|---|---|
| hh-experiment | wire, identity, provenance, ontology, ledger, budget, lab, eval | env |
| hh-bench | wire, identity, ontology, ledger, lab, hir, provenance, budget, eval | env |
| hh-env | (normal deps) | experiment, bench (it uses them as dev-deps only) |

So hh-env cannot host the runner without upward edges. hh-experiment is a pure engine and would gain podman reach. K-2 rules out the `hh` CLI. Root `AGENTS.md`: "`hh` is a client and renderer over `hh-embed/1`; keep kernel semantics out of the CLI".
Retrieved: 2026-11-06T23:22Z
Implication for the spec:
- **(a) A test binary or xtask under hh-env** (A1's suggestion). Rejected: it is not production machinery shared by four phases, and dev-dep edges cannot ship.
- **(b) An `lab.experiment.drive` op in hh-embed.** Rejected for now: a multi-hour blocking driver inside the serial RPC handler is wrong. It can be a later thin wrapper.
- **(c) A new crate.** **Recommended.** An ADR records it (ADR-B2-RUNNER). The crate map in `crates/AGENTS.md` and the root members list change. No external dependency is added.
Outline delta: EXTENDS OL-16 — the runner respects K-2 and CC1 by construction.
Limitations: A new member's effect on CI time and removability tiers was not measured.

### F4 — The store is single-writer in-process and dispatch borrows it across an exec, so slot concurrency must be process-level
Claim: `Store` is "one per root; `&mut self` … the in-process single-writer guard", and the S4 driver's `Dispatcher::new(store, …)` holds that borrow across a whole helper exec. A multi-hour agent phase therefore blocks every other append in the same process. The workable topology is:
- one coordinator process that owns the experiment store;
- one worker process per in-flight attempt that owns a **slot store**;
- the egress mediator and model proxy as separate sole-writer processes, each writing its own phase run (C2 §1.1 already does this for the proxy).

The coordinator then binds the worker's sealed slot-run head into the subject run and appends `lifecycle.run.finished` so that `settle` can project the outcome.
Status: PARTIALLY VERIFIED (the single-writer guard and the borrow pattern); the topology is UNVERIFIED.
Evidence kind: source inspection + design proposal
Evidence: `crates/hh-ledger/src/store.rs` 268–269. `s4-cell/src/main.rs` 684–701 (`Dispatcher::new(store, monitor, run, …)` then `disp.dispatch(driver, exec, …)`). `engine.rs` 1950–1961 (`settle` reads `self.store.events(&attempt.run_id)` for `lifecycle.run.finished`, so the subject run must live in the experiment store). C2 §1.1 ("the sole writer of one phase proxy run in the store").
Retrieved: 2026-11-06T23:22Z
Implication for the spec:
- **Proposed validation spike** (first runner ticket): a two-process offline test, coordinator plus worker, with a fixture executor that sleeps. It must show that `settle` projects the bound outcome, that kill-points between worker finish and coordinator bind leave a recoverable state, and that no store root is opened by two writing processes.
- **Conflict recorded with C2.** C2 F2/§1.1 writes the phase proxy run "in the store" from a separate process. If that is the same store root the coordinator holds, two processes write one root, which the single-writer guard does not cover. Recommendation: the proxy and the mediator each get their **own store root**, and slot folds read them read-only. H1 should rule between this note and 33 F2.
Outline delta: EXTENDS OL-17 (B3 sizes per-slot processes); EXTENDS OL-16 (durable-before-visible across processes).
Limitations: Cross-process read semantics of a store root another process is writing were not inspected.

### F5 — `central-protocol/1` is fixed to the round-5 compaction study and cannot express a harness comparison; selection needs a sibling materializer
Claim: The materializer is frozen to the round-5 study:
- `TASKS = 30`, `ROUNDS = 5`, `MAX_CONCURRENT_SUBJECTS = 2`;
- exactly two arms whose strategies must be `evict_oldest` and `clear_tool_results`;
- `PRIMARIES` fixed to three compaction metrics.

Its identity is a versioned payload, so widening it would silently change a frozen record's meaning. Its reusable parts are the domain-separated `idp_digest` ranks, `OUTCOME_BLIND_EXCLUSION_BASES`, typed refusals, and the support/drift/relabel gates.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-eval/src/central_protocol.rs` 40–82 (constants and bases), 89–95 (`rank`), 166–240 (`ProtocolDecl`), 660–700 (arm-set refusal: "exactly 2 required", strategies from `ARM_STRATEGIES`).
Retrieved: 2026-11-06T23:20Z
Implication for the spec: The selection hook is a new `hh-eval` module, sketched as `task_selection` / `task-selection/1`. Interface only; D1 owns the method:

```text
SelectionDecl { dataset{git_commit, hub_ref, catalogue_digest (sha256 over sorted (task_id, task_digest))},
  tasks[{task_id, task_digest, strata{difficulty, category, cost_stratum, timeout_stratum},
         declared{agent_timeout_sec, verifier_timeout_sec, cpus, memory_mb}}],
  exclusions[{task_id, basis ∈ OUTCOME_BLIND_EXCLUSION_BASES, ref}], exclusion_publication_ref,
  stages[{stage_id, filter (over declared fields only), quotas{stratum → n}, nested_in?}], seed }
→ MaterializedSelection { selection_id = idp_digest(...), stages[{stage_id, ordered tasks[]}] }
```

`cost_stratum`/`timeout_stratum` are derived **only** from declared `task.toml` values. Q-X31's "cheapest/shortest first" is then a stage filter, for example `agent_timeout_sec ≤ 900 ∧ memory_mb ≤ 4096`, chosen by D1. By the B1 census, 50 tasks have an agent timeout ≤ 900 s. Outcome-derived lists (HF saturation) are refused as filters. The stage's ordered tasks become `suite_tasks` for `lab.experiment.expand`. A2's correction of the FNV-labelled identity (A1 F13) is honoured by a real sha256 `catalogue_digest`.
Outline delta: EXTENDS OL-09 — admission and exclusion stay typed and outcome-blind; CONFIRMS B1 REQ-XB-017.
Limitations: The engine's `OrderPlan` decides interleaving; whether `random_permuted` blocks by replicate is as D1 needs was not traced in `expand.rs`.

### F6 — The verdict machinery to reuse exists: `OracleRunner`, typed ppm rewards, a closed infra taxonomy and honest shared isolation
Claim: hh-bench already provides the pieces a TB 2.1 verdict needs:
- the attempt owner, which persists `verification.oracle.invoked` before execution and the verdict after, with a frozen regrade count and no participant feedback;
- an injected `OracleExecutor` port;
- strict `{reward: ppm}` parsing, where failures are `oracle_failure` and never a 0;
- a closed `InfraClass` taxonomy, where "ambiguous ⇒ scored, flagged `infra_suspected`" (ADR-0144);
- `VerifierIsolation::Shared` spelled with `tamper: n/a{no_detector}`.

The Harbor reward file fits as an executor adapter. No new grading path is needed.
Status: VERIFIED (machinery); UNVERIFIED (the adapter)
Evidence kind: source inspection + design proposal
Evidence: `crates/hh-bench/src/oracle.rs` 1–44 (contract), 121–140 (`AttemptSpec{…, isolation}`), 147–155 (`SealedSubmission`: "a mutable workspace is never graded"), 181–189 (`OracleExecutor`), 385–420 (`OracleRunner`, durable `next_attempt_no`). `grade.rs` 1–13, 55–70. `infra.rs` 1–40, 133–150. `crates/hh-ontology/src/lab.rs` 321–329.
Retrieved: 2026-11-06T23:23Z
Implication for the spec: §1.5 as written. One tension is recorded honestly. `SealedSubmission` assumes the graded object is the sealed artifact. TB 2.1 verification grades the **live** post-agent container, including running processes (B1 F6). The S4 seal therefore becomes evidence of the end state that was graded, not the graded object itself. The record says `graded_object: live_cell; seal: post_agent_manifest`. This is a declared divergence from §5h.4's separate-verifier default, which the family record for TB 2.1 must declare as `shared` (ADR-0143 per `grade.rs` 6–7).
Outline delta: EXTENDS OL-02, OL-06 — the verdict channel is committed and typed.
Limitations: Whether `OracleRunner::attempt` accepts a seal row that pins a container diff manifest rather than a workspace pin was not traced.

### F7 — The "verifier bootstrap failure silently scores 0" hazard needs a deterministic, recorded classification rule
Claim: Every TB 2.1 `test.sh` writes `0` when the apt/astral/uv bootstrap fails (B1 F7). HH can separate a verifier-tier failure from a participant failure using only recorded bytes:

| Recorded condition | Classification |
|---|---|
| `reward_ppm == 0` and pytest never produced a report (no `ctrf.json`, or an unparseable one), with failure evidence: ≥1 mediator `decided{deny}` or upstream error in the verify phase, or a pre-pytest bootstrap signature in `test-stdout.txt` (apt `E:` lines, `Temporary failure resolving`, curl non-zero, uv fetch error) | `oracle_failure{verifier_unavailable:bootstrap:<egress_denied\|upstream_error\|signature>}` |
| The same, with no such evidence | **scored 0**, flagged `infra_suspected:verifier_no_report`, per ADR-0144's ambiguity rule. In shared verification the participant can legitimately break apt or Python, so this must not be relabelled inconclusive. |
| `ctrf.json` present with any collected tests | scored on the reward, whatever the counts |
| `reward = 1` and `ctrf.json` shows failures | scored on the reward, flagged `reward_ctrf_inconsistent` |

Status: UNVERIFIED (design proposal)
Evidence kind: design proposal over B1 F7/F13 and `infra.rs`
Evidence: B1 F7 (`fix-git/tests/test.sh` sequence; 0/89 `set -e`). `infra.rs` 1–12 (closed taxonomy and the ambiguity rule). §1.3 (per-phase egress rows make "egress denied during verify" an observable fact).
Retrieved: 2026-11-06T23:23Z
Implication for the spec: The classifier is a pure function of the slot's recorded files plus mediator rows, so it is recomputable records-in. `verifier_unavailable` triggers an in-cell regrade (rm stale reward, re-upload `/tests`, re-run) up to `max_attempts` (proposed 2). A regrade never re-runs the participant. The H/O divergence (Harbor scores 0) is declared in every comparison, and the leaderboard-convention metric (errors = 0) remains a secondary metric (B1 REQ-XB-012).
Proposed validation:
- fixture tests over a fake verifier, offline: no-report plus denied-egress gives `oracle_failure`; no-report with no evidence gives scored 0 with `infra_suspected`; ctrf present gives scored;
- a live fault-injection leg in the oracle re-proof that removes `astral.sh` from the verify allowlist for one task.
Outline delta: EXTENDS OL-02; refines B1 REQ-XB-007 (adds the evidence requirement and the ambiguity branch).
Limitations: `pytest-json-ctrf` behaviour on collection-time crashes was not read. The signature list is heuristic until the re-proof records real bootstrap output.

### F8 — The podman cell lacks four things TB 2.1 needs: the image WORKDIR, CPU/memory limits, a digest image ref in practice, and a container path for `/tests`
Claim: The cell as built today falls short in four places:
- `create_args` forces `--workdir /work`. That is the `task_workdir` defect class: TB 2.1 images declare their own WORKDIR (e.g. `/app`).
- The binary always passes `extra_args: vec![]`, so `ResourceLimits{memory_bytes, cpu_ms}` never reach podman. The task's `cpus`/`memory_mb` are not applied, while Harbor applies them as limits (B1 F9).
- The network flag is the closed `none|bridge` sum.
- `env_upload` is the fs effect surface over the handle's roots. Whether it can target `/tests` outside the `/work` mount is not established.
Status: VERIFIED (current argv and fields); UNVERIFIED (the upload target)
Evidence kind: source inspection
Evidence: `crates/hh-helper/src/podman.rs` 99–123 (`--workdir /work`, `-v ws:/work:rw`, `--pids-limit 512`, `extra_args` appended, `sleep infinity`). `bin/hh_helper.rs` 113–119 (`extra_args: vec![]`), 85–89 (`none|bridge` else usage). `hh-containment/src/policy.rs` 671–688 (`ResourceLimits`). `hh-env/src/driver.rs` 3557–3582 (`env_upload` refuses provider and hosted handles; byte source only). ADR-0421 Alternatives ("generic `--network <name>` passthrough: rejected").
Retrieved: 2026-11-06T23:24Z
Implication for the spec: A helper and cell-spec change, admitted by ADR in the ADR-0421 pattern:
- closed flags `--workdir-from-image` (read `.Config.WorkingDir` and record it), `--cpus N`, `--memory <MiB>`, and the socket mounts (§1.3);
- the C2 relay as the main process;
- a kernel-mediated copy-in verb for container paths outside `/work` (or a proven `env_upload` target), used for `/solution` and `/tests` only at their phases.

Every change keeps the default argv byte-identical, as ADR-0421 did.
Outline delta: EXTENDS OL-02.
Limitations: Podman `--cpus`/`--memory` behaviour under rootless cgroup v2 on the B3 image is not verified.

### F9 — The canonical containment contract decides the allowlist mechanism: `mediated` means no namespace route and bridged channels only; proxy variables never count; the attribution token is never model-visible
Claim: Canonical §5g.4 §2.2 defines `mediated` as "the namespace is removed and the only route out is a bridged channel to a mediator on the host side" and says "setting proxy environment variables alone never counts as `mediated`". The canonical spec also says the attribution token is delivered "through the bridged-channel preface, never the model-visible environment". This rules out three alternatives:
- (a) a podman `--internal` network plus a proxy sidecar, which keeps a namespace route and is not canonical `mediated` without a §5g.4 amendment;
- (b) bridge plus proxy variables, which is `public` dressed up;
- (c) any design that puts the kernel attribution token in the cell env.

The design that fits the contract unchanged is `--network none` plus bind-mounted unix-socket grants plus an in-cell loopback relay plus a host-side mediator. This is the same mechanism C2 chose for the model route.
Status: VERIFIED (contract text); UNVERIFIED (the composed profile)
Evidence kind: source inspection + design proposal
Evidence: `spec/CANONICAL_SPEC.md` 5286 (§2.2 modes), 5281 (attribution delivery), 3733 (`AttributionToken`: not a credential, single kernel minter), 1833 (harness-inside placements only as hosted `container-installed` participants). `hh-containment/src/policy.rs` 187–195, 411–425 (`UnixSocketMode::AllowListed`: the bridged channel "plus `allow[]` grants"), 791–804 (`ResidualKind::UnixSocket`, `TlsOpaque`, `ApprovedHostBody`, `DnsNames`, `SideChannel`). `backend.rs` 1–22 and 630–689 (proxy env is a stratified `lowering_loss`). C2 F5.
Retrieved: 2026-11-06T23:24Z
Implication for the spec:
- The registry channel's attribution is **kernel-side by socket identity**: the per-slot socket path binds slot and phase. The mediator presents the slot's agent-phase or verify-phase effect token internally, and no token enters the cell.
- C2's per-slot proxy token (`*_API_KEY`) is a proxy credential, distinct from the kernel attribution token, so the two designs do not conflict here.
- Residual `side_channel`: every in-cell process is attributed to the phase effect. Per-process attribution is not claimed.
Outline delta: EXTENDS OL-02, OL-15; supplies Q-X30's "enforced and logged per slot".
Limitations: Whether the podman backend's declared caps satisfy `enforce_unix` for `allow_listed` (`backend.rs` 448) is unverified, and so is SELinux labelling of the mounted sockets. C2 F5 shares both risks.

### F10 — Per-phase network policy already exists (`set_phase` over a sealed `phase_schedule`) and maps one-to-one onto Harbor's phase semantics
Claim: `EnvDriver::set_phase(setup|agent|verify)` applies a sealed `phase_schedule` entry, either a `net_mode` shorthand or a full `layer` policy, through the layered meet. It refuses typed when the schedule is undeclared, emits a durable `action.environment.phase.changed`, and is exposed as `env.set_phase`. Harbor scopes `[agent]` network policy to `agent.run()` and verifier policy to verification (B1 Harbor table). The runner can therefore change the egress allowlist per phase by policy alone, with no container change.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-env/src/driver.rs` 2073–2200 (CF-318, ADR-0142, S4.14b; `basis` must be `policy_rule`; the meet refuses widening). `crates/hh-embed/src/envops.rs` 1–15.
Retrieved: 2026-11-06T23:25Z
Implication for the spec: The agent and verify allowlists are sealed `layer` documents, with `net.rules` allow entries per host, inside the cell's policy before provision. The mediator decides against the handle's current effective policy, so the policy ids per phase land in the slot record. A phase that loosens (verify adds verifier hosts) names `principal` authority, as `set_phase` requires.
Outline delta: EXTENDS OL-02.
Limitations: Whether a `layer` carrying `rules` passes the meet without `ContainmentWidening` for a sibling-phase loosening was not exercised.

### F11 — The egress mediator is request/response; registry traffic needs a CONNECT tunnel leg that reuses `gate`/`forward` ordering
Claim: `EgressMediator` splits a request into `gate`, which makes `requested` durable, attributes, and runs `decide_egress`, and `forward`, which rechecks, makes `decided{allow}` durable, sends on the wire and charges. Its `EgressTransport::forward` returns one `WireResponse`. apt, pip and uv need streamed HTTP and `CONNECT` tunnels with `tls.terminate = false`.
Status: VERIFIED (current shape); UNVERIFIED (tunnel leg)
Evidence kind: source inspection + design proposal
Evidence: `crates/hh-env/src/egress.rs` 1–28 (ordering contract 1–5), 132–140 (`EgressTransport::forward`), 393–429 (`GateOutcome`/`GateAllow`), 559–634 (`gate`), 1191–1194 (`HttpsConnect` leg handling). `hh-containment/src/policy.rs` 315–328 (`EgressProtocol::HttpsConnect`).
Retrieved: 2026-11-06T23:25Z
Implication for the spec: The design adds a tunnel transport leg in hh-env, `forward_tunnel`, which runs after the same recheck and `decided{allow}`. It pipes bytes until close, then appends a closing summary row (bytes up/down, duration) and charges `network.calls`. The `hh-egress-mediator` binary hosts one listener per slot socket and is the sole writer of its phase egress run (F4). The per-slot and per-host counts in the slot record are folds over those rows (CC3). Whether the closing row is a new class or a payload member of an existing one is an E2/H1 call. Any new class goes in `classes.rs` (CC10).
Outline delta: EXTENDS OL-02.
Limitations: apt over a CONNECT proxy needs `Acquire::https::Proxy` or `https_proxy`, and some tools ignore proxy variables. Those fail closed, which is honest, and the oracle re-proof will surface them.

### F12 — The allowlist can be derived outcome-blind before paid runs, but host granularity leaves answer-key exposure on shared hosts
Claim: TB 2.1 `task.toml` declares only `allow_internet = true` (B1 F8), so the allowlist cannot come from the task. A frozen, outcome-blind allowlist can be built from four sources:
- (i) a declared base list per image family, from the B1 `FROM` census: Debian/Ubuntu mirrors, `astral.sh`, PyPI hosts and the uv Python download host;
- (ii) a committed URL scan of `instruction.md` (agent phase) and `tests/*` (verify phase);
- (iii) oracle-observed hosts from the re-proof's observe pass;
- (iv) explicit host denies for answer-key sites (`hub.harborframework.com`, `tbench.ai`).

CONNECT reveals only host names, so a granted shared host (`github.com`, `raw.githubusercontent.com`, `huggingface.co`) cannot be path-filtered. That is an `approved_host_body` residual. The trajectory/egress scan (Q-X30 "+ scan", D1) covers it.
Status: UNVERIFIED (design proposal resting on VERIFIED census facts)
Evidence kind: design proposal
Evidence: B1 F4, F8, F18 and Appendix A (`Ni`/`Nv` flags: 5 instruction-download tasks; 2 verifier-GitHub tasks). C3 F9 (web-search routes refused at the proxy; C2's domain). `policy.rs` 380–395 (`EgressRule` with ports, protocols, methods, decision, justification).
Retrieved: 2026-11-06T23:25Z
Implication for the spec: Oracle-observed **agent-phase** hosts outside (i) and (ii) are not granted automatically. The reference solution's fetch path can reach answer-adjacent material, so each is listed as `withheld_host{reason}` in the pre-registration for an operator grant (OQ-2). Verify-phase observed hosts are granted, since they are verifier machinery. After the first paid slot the allowlist is frozen; a change is a dated amendment and a new epoch (A1 F8's lesson).
Outline delta: EXTENDS OL-09 — each arm's declaration carries its egress posture and allowlist digest.
Limitations: The observe pass needs a host-side view of a bridged cell's connections (B3's recipe) or the permissive `ask→log` alternative. Neither is built.

### F13 — The 900 s cap is a single-path derivation with a single-task admission; per-task declared deadlines need a separate admitted path that leaves the legacy path byte-identical
Claim: `verifier_deadline_ms = min(declared, 900 s) × 1000` binds every caller. `admit_verifier_bound` is named-task scoped and bounded by `≤ 900`. Its doc states that a larger need is "a separately recorded admission, never a wider silent bound". ADR-0422 grounds the cap in the evidence supplement's frozen §9.2 envelope ("verifier ≤15 min", pilot "30 min admitted wall"). Those are round-5 stage envelopes, not properties of TB 2.1. On TB 2.1, 39/89 verifier timeouts and 39/89 agent timeouts exceed 900 s (B1 census).
Status: VERIFIED (current code and ADR scope); UNVERIFIED (the proposed admission)
Evidence kind: source inspection + design proposal
Evidence: `crates/hh-env/src/verifier_bound.rs` 11–37 (contract), 53–56 (cap), 150–156 (`verifier_deadline_ms`), 197–205 (cap refusal). ADR-0422 Decision 1–4 and its revisit trigger ("A second task needs a relaxed verifier bound: … a new recorded admission"). `docs/evidence-and-adoption-spec.md` 597–620 (§9.2 stage envelopes). B1 F10 and Appendix A.
Retrieved: 2026-11-06T23:26Z
Implication for the spec: **ADR-B2-DEADLINE content (answers Q-X32):**
- **(1) Scope.** Slots whose protocol declares `deadline_source: task_declared` and whose task belongs to an admitted dataset pin (TB 2.1 at the B1 three-part pin). Round-5 subjects and the TB-1.0 channel keep `verifier_deadline_ms` unchanged.
- **(2) Rule.** Agent deadline = `[agent].timeout_sec × m_agent`; verifier deadline = `[verifier].timeout_sec × m_verifier`. The multipliers are integer ppm, canonical `1_000_000`. A setup multiplier is declared per run, and HF's ×3 on setup only is a declared deviation (B1 F16). Values are read verbatim from the pinned `task.toml` bytes. A malformed or absent value refuses typed and never becomes a default.
- **(3) Ceiling.** The protocol declares `max_task_deadline_sec`. A declaration above it refuses (`Unsupported{deadline.task_declared:exceeds_protocol_ceiling}`), so it is excluded or pre-registered, never clamped. This replaces the silent `min` for admitted slots.
- **(4) Home.** A new `hh-env` function, `task_declared_deadline_ms(decl, multiplier_ppm, ceiling) -> Result<u64, EnvError>`, beside `verifier_deadline_ms`, with a transactional `admit_task_declared_deadlines(record, admission)`. It stamps `ext["deadline_admission"]{source: "task.toml", dataset_pin, task_digest, agent{declared_sec, multiplier_ppm, enforced_ms}, verifier{…}, ceiling_sec, protocol_id}` and a slot echo, mirroring `slot_echo`. `VERIFIER_BOUND_CAP_SEC` and `admit_verifier_bound` are untouched.
- **(5) Envelope.** The ADR states that the program's own supplement, written by O1, defines its stage envelopes. The frozen round-5 §9.2 envelope is neither amended nor reused.
- **(6) Agent timeout.** On lapse the helper terminates the agent process group (term → kill), and the slot proceeds to verification. This matches Harbor's treatment of agent timeouts as valid rewards (B1 F13); C4 confirms in Harbor source.
Outline delta: EXTENDS OL-16 — an admission, never a silent bound.
Limitations: The helper's own deadline ladder has no cap per the reads made (`hh-helper/src/exec.rs` 10, 132–147). A cap elsewhere in the dispatch ladder (`deadline.rs`) was not ruled out beyond the lines read.

### F14 — Digest-pinned images work through the existing `ForeignDigest` path; the S4 cells ran on unpinned tags
Claim: `ImageRef::ForeignDigest{scheme, value, source}` exists, and `start_helper` passes a resolved `Foreign` image's `value` verbatim as the podman image argument. A `docker.io/alexgshaw/<task>@sha256:<d>` spelling therefore needs no schema change. Its reproducibility claim caps at R0/R1/R3, never R2. The S4 driver used `ImageRef::Tag` plus `unpinned`, which is R0.
Status: VERIFIED
Evidence kind: source inspection
Evidence: `crates/hh-env/src/record.rs` 123–139, 173–193 (`supports_r2` only for `ContentAddress`). `driver.rs` 823–833. `s4-cell/src/main.rs` 274–286 (`ImageRef::Tag(spec.image)`, `unpinned.insert`).
Retrieved: 2026-11-06T23:26Z
Implication for the spec: Digests are resolved once per backend at selection freeze, from the platform-specific manifest, and recorded with platform. Slots pull by digest only. A tag drift between freeze and run is caught by the digest check (REQ-XB-003). The record's reproducibility claim is labelled R1, not R2.
Outline delta: EXTENDS OL-02.
Limitations: Multi-arch status and image sizes are B3's measurement (B1 Q-B1-6).

### F15 — The oracle re-proof proves the execution surface only; it must carry `parity: none` and the admission code's own refusal
Claim: Evidence spec §4.4 lists "oracle transport equality alone" among the shapes that fail authentic parity. `parity_admission` encodes this as `InauthenticEvidence::OracleTransportEquality` and refuses it. The re-proof's honest outputs are:
- per-task admit/exclude/discordance;
- a frozen allowlist validated under the paid posture;
- surface-level agreement counts.

It is not a parity result.
Status: VERIFIED (contract); design per §1.6 UNVERIFIED
Evidence kind: source inspection + design proposal
Evidence: `docs/evidence-and-adoption-spec.md` 284 ("Empty maps, wrappers, supplied model-I/O fixtures or oracle transport equality alone fail … Until admission, only product smoke with `parity=none`"). A1 §4 (`parity_admission.rs` 190–194, 856–862; not re-read here).
Retrieved: 2026-11-06T23:26Z
Implication for the spec: §1.6. Every task the paid subset draws must have `admit` from the re-proof on the same backend recipe (REQ-XB-005), so the re-proof gates phase 2.
Outline delta: CONFIRMS OL-06's honesty boundary; EXTENDS OL-02.
Limitations: R = 3 is a cost-vs-flake judgement, not derived from a power analysis (none applies).

### F16 — Engine outcome classes and the program's typed dispositions map one way only
Claim: The engine settles into a closed six-class `OutcomeClass`. Retries apply only to `infrastructure_failure`; `oracle_failure` regrades and never re-runs (`RETRYABLE_CLASSES`). The program's disposition vocabulary maps onto these without relabelling.
Status: VERIFIED (classes); mapping is a design proposal
Evidence kind: source inspection
Evidence: `crates/hh-ontology/src/control.rs` 249–277, 632–649. `eval.rs` 2243–2249. `central_protocol.rs` 80–82.
Retrieved: 2026-11-06T23:26Z
Implication for the spec:

| Slot fact | Subject `StopReason` → `OutcomeClass` | Disposition |
|---|---|---|
| Valid reward (any agent end-cause including timeout or budget refusal) | `Completed` → `scored` | `executed{resolved\|unresolved}` with `end_cause` |
| Verifier unavailable after regrades | oracle-failure fact → `oracle_failure` | `inconclusive{verifier_unavailable:<c>}` |
| Provision, cell death, runner crash, relay death | `InfrastructureFailure{…}` → `infrastructure_failure` | `inconclusive{infra:<c>}`; replanned as a new attempt within `ReattemptPolicy` |
| S0 admission refusal | `Refused{…}` → `refused` | `withheld{admission:<reason>}` |
| Operator pause or cancel | `Cancelled` → `cancelled` | `inconclusive{cancelled}` |

A model-spend slot refusal (C2 402) ends the agent phase. The slot still verifies, so it is scored with `end_cause: budget_refused`. D1 decides whether the estimand treats such slots as missing.
Outline delta: EXTENDS OL-16.
Limitations: How `terminal_of` reads an oracle-failure fact from the payload (`engine.rs` 3721) was not read in full.

### F17 — Crash recovery comes from durable state, and concurrency is resource-packed within the engine's recorded order
Claim: Two properties make crash recovery and concurrency compatible with the existing records. Recovery can rest entirely on durable state: the engine view, the slot stores and the phase runs. Packing within the engine's recorded order changes start times, never assignment.
Status: UNVERIFIED (design proposal)
Evidence kind: design proposal over F2/F4 and B1 F9, C3 F14
Evidence: `engine.rs` 1–43 (re-projection; `claim_expired`). `experiment.rs` 444–507 (`Pool` limits; `start_stagger_ms`). B1 F9 (memory packing, ≈28 GiB on 32 GiB). C3 F14 (Ollama PAYG 3 concurrent requests). `driver.rs` 77–82 (`PRESERVE_UNTIL_TTL_MS`).
Retrieved: 2026-11-06T23:27Z
Implication for the spec:
- **Recovery.** On restart, the coordinator re-attaches the engine. For each claimed or launched attempt without a settle:
  - a sealed, finished slot store is bound and settled;
  - otherwise the attempt is closed as `InfrastructureFailure{runner_crash}`, its labelled container is swept, and the engine replans it within bounds.

  A paid agent phase is never resumed mid-run: proxy state and the model conversation are not resumable honestly. Helper `preserve_until` is used only for teardown.
- **Concurrency.** Plans start in the engine's recorded order with first-fit packing, under three bounds:
  - Σ `memory_mb` ≤ the host budget and Σ `cpus` ≤ the CPU budget, both declared by B3;
  - pool `model_snapshot:<ref>` limit = provider concurrency (3 on PAYG), applied to agent phases only;
  - a declared lookahead window `w` (proposed 4): a later plan may start ahead of a waiting one by at most `w` positions.

  Actual start order and lookahead use are recorded per slot. Packing never changes assignment, only start time.
Outline delta: EXTENDS OL-17.
Limitations: The packing rule interacts with C2's FIFO and semaphore. The two must agree on which pool admits an agent phase.

### F18 — C2's `model_proxy` profile and Q-X30's registry allowlist compose into one profile; the conflicts are ownership, socket count and store topology, not mechanism
Claim: C2 (33 F5, §1.1, ADR-C2-C) admits a `model_proxy` profile: `--network none`, `mediated`, one bind-mounted socket, and an in-cell relay as the main process. The note says "each cell sees exactly one socket and nothing else on the network". Q-X30 requires package registries too. The two compose without changing mechanism:
- a second granted socket (`/run/hh/egress.sock`);
- the relay generalized from one port⇄socket pair to a declared closed list of pairs;
- the per-phase schedule (F10).

Three conflicts are recorded for H1:
- (a) **Ownership.** ADR-C2-C and this note's ADR-B2-EGRESS would both admit a cell egress profile. Recommendation: one ADR, owned by B2 per the ledger assignment, with C2's model channel as a named component, and ADR-C2-C folded into it.
- (b) **"Exactly one socket."** This becomes "exactly the sockets the phase schedule grants". The model socket is closed at agent end, which C2's design permits as a per-slot listener lifecycle.
- (c) **Store topology.** See F4.

C2 assumes rootless podman. The S4 GCE backend was rootful (A1). B3 decides, and both designs must be validated on that choice.
Status: VERIFIED (both designs as written); the composition is UNVERIFIED
Evidence kind: design comparison
Evidence: `docs/design/33_model-mediation.md` §1.1 topology box, F2, F5, §3 (ADR-C2-C row), §4 ticket 5. Ledger change log 2026-11-06T23:15:26Z ("B2 owns cell egress-profile composition … C2 owns the proxy endpoint itself").
Retrieved: 2026-11-06T23:27Z
Implication for the spec:
- The relay binary (C2: `hh-model-relay` in hharness-helper) becomes `hh-cell-relay` with `--pair <port>=<socket>` ×n. Its pairs come from a closed set: `model`, `egress`.
- C2's ticket 5 and this note's egress ticket merge, or are sequenced so that the relay and the helper mount flag land once.
- No C2 decision is overridden: the proxy, its token, its budget and its receipts stay C2's.
Outline delta: EXTENDS OL-02, OL-16 (one owner per contract).
Limitations: C2 was read once, at 2026-11-06T23:27Z. Later revisions of 33 are not reflected.

## 3. Ticket shaping hints

Each seam fits one fresh-context `implement-spec` run. Acceptance is offline and podman-free unless marked live. Live legs are `#[ignore = "live — ADR-0363: scripts/live-verify.sh <leg>"]`. Code lands under `crates/`, so CI runs full mode, while the runner's packets under `docs/build/reports/` are docs paths (ADR-0362).

1. **RUNNER-CORE: crate, coordinator/worker topology, slot record.** ADR-B2-RUNNER. AC:
   - fixture executor plus fake verifier drive a 2-arm × 3-task × 2-replicate experiment through engine `next/claim/launch/settle`, and the slot records validate against `hh-experiment-slot/1`;
   - kill-point battery: worker death before seal, after seal before bind, and coordinator death mid-claim each recover to the correct disposition with no duplicate attempt;
   - two writing processes never open one store root;
   - the packing unit test respects memory, CPU, pool and lookahead bounds, and the recorded start order is reproducible from the seed.
2. **VERDICT: `harbor_reward_file/1` + `tb10_pytest_log/1` + the F7 classifier.** AC:
   - golden reward files (`1`, `0`, `0.5`, empty, garbage, a multi-key json) map to ppm or typed `oracle_failure`;
   - the F7 truth table over fixture byte sets;
   - an in-cell regrade bound;
   - the TB-1.0 channel reproduces the committed `receipt.parsed` on any archived log A2 commits as a fixture, and is refused for TB 2.1 pins.
3. **CELL-SPEC: helper flags (image WORKDIR, cpus/memory, copy-in for `/tests`/`/solution`) + digest images.** Joint ADR with item 4, or a separate one. AC:
   - a `podman` stub on PATH (the `tests/r5_56.rs` pattern) pins each argv and the default argv is byte-identical;
   - copy-in refuses `/work` aliasing;
   - a tag-only image ref refuses for TB 2.1 slots.
4. **EGRESS: `allowlist_mediated` profile, relay generalization, `hh-egress-mediator`, tunnel leg, sealed phase layers.** ADR-B2-EGRESS, absorbing ADR-C2-C. AC:
   - transactional admission (a refusal leaves policy and record byte-identical);
   - a std `TcpListener` fixture upstream proves CONNECT allow/deny with `requested` durable before decision and `decided{allow}` durable before the first tunnelled byte;
   - a denied host yields `decided{deny}` and a 403 to the client;
   - `set_phase(verify)` closes the model route and swaps rules;
   - the residual list is complete.
   Live leg: on the B3 VM, a cell reaches only the two sockets (a direct `curl 1.1.1.1` fails; an allowed host succeeds through the proxy).
5. **DEADLINE: ADR-B2-DEADLINE + `task_declared_deadline_ms` / admission.** AC:
   - declared 12000 s with multiplier 1.0 is enforced at 12 000 000 ms under a ceiling of 12000;
   - above the ceiling refuses;
   - a malformed decl refuses;
   - the legacy `verifier_deadline_ms`/`admit_verifier_bound` tests (`r5_57`, 13/13) are unchanged;
   - the slot echo carries the members.
6. **SELECT: `task-selection/1` in hh-eval.** AC:
   - same seed and declaration give an identical `selection_id`;
   - an outcome-derived filter refuses;
   - an exclusion with an unadmitted basis refuses;
   - stage nesting holds;
   - the `catalogue_digest` recomputes from fixture `dataset.toml` bytes.
7. **HARBOR-O: job-config writer, invocation wrapper, read-only ingest, cause map.** AC:
   - a committed fixture job tree (hand-built from B1's layout) ingests into `hh-harbor-ingest/1` with per-file sha256;
   - every pinned `error_policy` cause maps;
   - the job tree is byte-unchanged after ingest;
   - a missing `--upload` guard is asserted;
   - image-digest drift yields `withheld{image_drift}`.
8. **REPROOF (live, operator-gated, $0 inference, compute per B3 quote).** Runs §1.6 on the stage-1 stratum: observe → freeze → prove. It produces the discordance register, the frozen allowlist, the exclusion rows, the committed `parity_admission` refusal and, if registered, `hh-live-evidence/1`. It needs GATE approval for provisioning (B3). Its outputs gate any paid ticket.

Order: 1 ∥ 3 ∥ 5 ∥ 6 → 2 → 4 (after C2 ticket 3) → 7 → 8.

## Answer and completion assessment

**Answer.** The committed runner is a new `hharness-runner` crate and `hh-runner` binary.
- **Where it sits.** It drives the existing `ExperimentEngine` and adds no second scheduler or store. Cells run through `EnvDriver`/`hh-helper` in per-slot worker processes; verdicts run through hh-bench's `OracleRunner` with a Harbor reward-file executor. It sits above hh-env and below hh-embed, out of the CLI (F2–F4).
- **Inputs.** The selection comes from a sibling `task-selection/1` materializer (F5) and feeds `lab.experiment.expand`; arms and replicates are `ArmSpec`s.
- **Lifecycle and outputs.** §1.2 gives the slot lifecycle and §1.4 the slot envelope. Retries are the engine's infrastructure-only replans; verifier failures regrade in-cell; nothing is relabelled (F16, F17).
- **Egress (Q-X30).** One profile, `allowlist_mediated`: `--network none`, canonical `mediated`, two bridged sockets (C2's model route and B2's registry mediator) and sealed per-phase layers. It is enforced and logged per request, with honest residuals (§1.3, F9–F12, F18).
- **Timeouts (Q-X32).** A separate task-declared deadline path, admitted by ADR, leaves the 900 s legacy path byte-identical (F13).
- **Oracle re-proof.** The $0 re-proof is O versus H-public versus H-allow at R = 3, labelled execution surface with `parity: none` (§1.6, F15).

**Settled.** The layering, the reuse map (engine, `OracleRunner`, `set_phase`, `EgressMediator`, `ForeignDigest`, `ADR-0421` pattern), the verdict rules and dispositions, the deadline ADR content, the re-proof design and the ticket seams.

**Technical blockers before the paid phases:**
- (1) the process/store topology spike (F4);
- (2) the CONNECT tunnel leg (F11);
- (3) helper cell-spec flags and copy-in (F8);
- (4) the relay static build, shared with C2;
- (5) four ADRs (below).

**Acceptable uncertainty for this row.** Bootstrap signature lists, podman cgroup and SELinux behaviour, image sizes, and Harbor's agent-timeout verification semantics. The re-proof or the implementing tickets resolve each.

**ADRs needed** (numbers allocated by the coordinator):
- ADR-B2-RUNNER: crate, placement, slot-record schema;
- ADR-B2-EGRESS: the `allowlist_mediated` profile, absorbing ADR-C2-C;
- ADR-B2-CELL: helper WORKDIR, limits, copy-in (may merge into EGRESS);
- ADR-B2-DEADLINE: Q-X32.

No canonical-spec amendment is needed for the egress design, because it stays inside §5g.4's `mediated`. No new external dependency is needed. Harbor is an external pinned tool recorded as `runner_ref`.

## Open questions

1. **OQ-1. Single egress-profile owner.** Owner: H1. Affects C2 and C4. Operator decision: no, unless H1 cannot reconcile. Fold ADR-C2-C into ADR-B2-EGRESS (F18).
2. **OQ-2. Granting oracle-observed agent-phase hosts.** Owner: D1. Affects B2, C4 and F2. **Operator decision: yes.** Hosts outside the base list and the instruction scan are withheld by default and listed for grant. `github.com` is granted only to tasks whose instruction requires it, with an `approved_host_body` residual plus scan.
3. **OQ-3. Flaky-oracle tasks (O 1–2/3).** Owner: D1. Should they be excluded as `runner_infeasible:flaky`, or kept as a covariate? Operator decision: no; confirm at ratification.
4. **OQ-4. Store topology.** Owner: H1 with C2. Proxy and mediator get separate store roots, versus C2's "in the store" (F4). Operator decision: no.
5. **OQ-5. Rootful vs rootless podman and runner co-location.** Owner: B3. Affects C2 and C4. The runner, proxy and mediator must run on the podman host, which retires S4's `gcloud ssh` mirror. Operator decision: no.
6. **OQ-6. Copy-in for `/tests` and `/solution`; Terminus-2 placement.** Owner: C4/E2. Terminus-2's host-side model calls bypass the cell's egress profile and go to C2's loopback listener (33 F17). Operator decision: no.
7. **OQ-7. Re-proof evidence label.** Owner: H1. Register a `live-verify.sh` leg so the re-proof can earn `live-executed`, or label it recorded execution only? Operator decision: yes, at the re-proof gate.
8. **OQ-8. Harbor pin.** Owner: B2/C4. Stable `v0.25.0` as `runner_ref`; is a tool admission record enough without an ADR? Operator decision: no.

## Spec requirements emitted

| ID | Requirement (candidate) | Rationale / source | OL | Acceptance |
|---|---|---|---|---|
| REQ-XB-101 | Every executing phase (2–4) and any fresh TB-1.0 tranche runs through one committed runner (`hharness-runner`). Scratch drivers are never used for a recorded slot. | F1, F3; A1 F15 | OL-02, OL-16 | Packets name the runner commit and binary digest; review |
| REQ-XB-102 | The runner drives `hh-experiment`'s engine (`next/claim/launch/settle`) and keeps no scheduler, retry counter or authoritative store of its own. | F2; CC1 | OL-16 | Code review plus a test that a deleted runner cache changes no outcome |
| REQ-XB-103 | The runner adds no kernel semantics to `hh` (K-2) and no edge into hh-embed, hh-cli or hh-hosting. | F3 | OL-16 | `cargo tree` check in CI |
| REQ-XB-104 | Slot concurrency is process-level. No store root has two writing processes. The model proxy and egress mediator write their own phase runs. | F4, F18 | OL-16, OL-17 | Two-process kill-point battery |
| REQ-XB-105 | Resume and recovery derive only from durable records. An interrupted attempt is closed as `inconclusive{infra:runner_crash}` and replanned within bounds. A paid agent phase is never resumed mid-run. | F17, F1 | OL-16 | Kill-point battery; no file-existence skip in the code |
| REQ-XB-106 | Each slot follows the §1.2 lifecycle. `/tests` (and `/solution` except for the oracle) is absent until its phase and is uploaded through a kernel-mediated copy-in, never through `/work`. | F1, F8; B1 F6 | OL-02, OL-06 | Fixture cell test listing the container fs per phase |
| REQ-XB-107 | Cells run in the image's declared WORKDIR and with the task's declared `cpus`/`memory_mb` as limits, recorded per slot. | F8 | OL-02 | Podman-stub argv test |
| REQ-XB-108 | Every slot emits one `hh-experiment-slot/1` record with the §1.4 sections. `costs.model` is C2's fold and `costs.compute` is B3's, referenced, not duplicated. | §1.4; A1 F11 | OL-02, OL-17 | Schema check; fold recompute test |
| REQ-XB-109 | The TB 2.1 verdict is `harbor_reward_file/1` through `OracleRunner` with `shared` isolation, under the §1.5 parse rules. Reward bytes are retained verbatim. | F6; B1 F6, F7 | OL-02, OL-06 | Golden reward-file tests |
| REQ-XB-110 | A zero reward without a pytest report is `oracle_failure{verifier_unavailable:bootstrap:*}` only with recorded egress or bootstrap evidence. Otherwise it is scored 0 and flagged `infra_suspected`. The rule is a pure function of the recorded bytes. | F7 | OL-02, OL-03 | Truth-table fixture tests; records-in recompute |
| REQ-XB-111 | A verifier-unavailable slot regrades in the same live cell up to the frozen `max_attempts` and never re-runs the participant. | F6, F7, F16 | OL-16 | Regrade-bound test |
| REQ-XB-112 | `tb10_pytest_log/1` exists only for TB-1.0 pins (phase-1 reproducibility) and is refused for TB 2.1. | §1.5; A1 REQ-XA-009 | OL-01, OL-02 | Channel-selection refusal test |
| REQ-XB-113 | Paid TB 2.1 cells use the single `allowlist_mediated` profile: `--network none`, `net.mode = mediated`, granted sockets only, sealed per-phase layers and declared residuals. It is admitted by one ADR owned by B2. | §1.3, F9, F10, F18; Q-X30 | OL-02, OL-09 | Admission tests; live reachability leg |
| REQ-XB-114 | Every agent- and verify-phase egress request is decided by `decide_egress` and recorded as `security.egress.requested/decided` before any byte flows. Per-slot per-host counts are folds over those rows. | F11; Q-X30 | OL-02, OL-15 | Ordering test with a fixture upstream |
| REQ-XB-115 | No kernel attribution token enters the cell. Registry-channel attribution is by per-slot socket identity, declared as a `side_channel` residual. | F9 | OL-15 | Env-projection test; residual-list check |
| REQ-XB-116 | The allowlist is derived from base registries, static task-file scans and oracle-observed hosts, frozen with its digest before the first paid slot. Withheld hosts are listed with reasons. A later change is a dated amendment and a new epoch. | F12 | OL-09, OL-16 | Pre-registration packet carries the digest; a changed list refuses under an unchanged protocol_id |
| REQ-XB-117 | Task images are pulled by digest only and recorded as `ForeignDigest` with platform. The reproducibility claim is labelled R1. | F14; B1 REQ-XB-003 | OL-02 | Tag-only refusal test |
| REQ-XB-118 | Admitted TB 2.1 slots enforce task-declared agent and verifier deadlines × declared multipliers through a new admission. Values above the protocol ceiling refuse. The legacy 900 s path stays byte-identical. | F13; Q-X32 | OL-02, OL-16 | Deadline tests; `r5_57` unchanged |
| REQ-XB-119 | On agent-deadline lapse the agent process group is terminated and the slot proceeds to verification with `end_cause: agent_timeout`. | F13, F16 | OL-02 | Fixture test |
| REQ-XB-120 | Slot facts map to engine outcome classes and dispositions by the F16 table, with no relabelling. Only `infrastructure_failure` replans. | F16 | OL-16 | Mapping table test |
| REQ-XB-121 | Concurrency is first-fit in recorded order under memory, CPU and model-pool bounds and a declared lookahead window. The actual start order is recorded. | F17; B1 F9; C3 F14 | OL-17 | Packing unit test |
| REQ-XB-122 | Task selection uses a `task-selection/1` materializer with strata from declared `task.toml` fields only. `central-protocol/1` is not widened. | F5; Q-X31 | OL-09 | Selection determinism and refusal tests |
| REQ-XB-123 | Harbor runs as an external pinned tool from a runner-written job config with `-r 0`, never `--upload`. Job trees are ingested read-only into `hh-harbor-ingest/1` with per-file sha256 and a pinned cause map. | §1.7; B1 F12–F14 | OL-02, OL-06 | Fixture job-tree ingest test |
| REQ-XB-124 | Both surfaces run identical image bytes: pre-pull by digest, tag locally, verify after each Harbor job. Drift marks trials `withheld{image_drift}`. | §1.7 | OL-06 | Drift fixture |
| REQ-XB-125 | Before any paid slot, the §1.6 $0 oracle re-proof (O, H-pub, H-allow; R ≥ 2, default 3) admits every drawn task. Discordances go in an append-only register. A machinery discordance is repaired, never excluded. | F15; B1 REQ-XB-005 | OL-02, OL-06 | Re-proof packet plus register reviewed at a gate |
| REQ-XB-126 | The re-proof record is labelled execution surface with `parity: none` and includes `parity_admission`'s committed typed refusal. `live-executed` appears only with an `hh-live-evidence/1` record. | F15; ADR-0363 | OL-06, OL-12 | `check-live-evidence.sh`; phrase pin |
| REQ-XB-127 | Containers carry an experiment label, and sweeps remove only containers with that label. A teardown receipt is recorded per slot. | F1 | OL-17 | Podman-stub sweep test |

## Handoff

- **Dependencies resolved.**
  - A1: F1, F5, F8, F9 and F15 are consumed. REQ-XA-008/009/010/016 are honoured by REQ-XB-101/106/108/112.
  - B1: F1–F20 are consumed. REQ-XB-002/003/005/006/007/009/011/012/018 are refined by REQ-XB-109/110/117/118/123–125.
  - C2: read and reconciled (F18).
- **Supplies:**
  - **B3:** the process topology (coordinator, k workers, proxy, mediator on the podman host), the packing inputs, the observe-pass capture need and the re-proof N.
  - **C4:** the S2 setup hook, the copy-in verb, the relay pair list and the Terminus-2 placement question.
  - **D1:** the selection interface, the F7 ambiguity branch, the flaky-oracle question, the end-cause stratification and the allowlist grant policy.
  - **D2:** `hh-harbor-trial/1` and ATIF refs.
  - **E2:** the tunnel leg and any closing-row class.
  - **F2:** the slot envelope and ingest records.
  - **H1:** four ADRs, OQ-1/4/7 and the ticket order.
- **Proposed follow-up rows.** None. B1's suggested "B1a image and bootstrap measurement" folds into ticket 8's observe pass.
- **Conflicts with other notes.**
  - C2 33 F2/§1.1: store topology (F4).
  - C2 33 F5/ADR-C2-C: profile ownership and the one-socket statement (F18).
  - A1's suggested runner home (test binary or xtask): rebutted in F3.
- **Capability/portfolio dispositions.** None changed. Nothing here is executed evidence.
