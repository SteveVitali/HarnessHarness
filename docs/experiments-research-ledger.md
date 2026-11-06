# HarnessHarness experiments program — research & design ledger

- **Status vocabulary:** `open` → `in-progress` → `done` | `blocked-on-operator` | `dropped(reason)`.
  Completed rows require evidence: an accepted note, a source actually read,
  a spec section, or an explicit decision with rationale.
- **Owner vocabulary:** `R` research · `D` design · `A` architecture · `P` product · `S` synthesis.
- **⚑** marks an operator-fixed constraint (not up for redesign).
- **Nothing cited that was not read.**

Read the [founding brief](experiments-brief.md), the [outline obligations](experiments-OUTLINE_TRACE.md)
and the [note conventions](research/CONVENTIONS.md) (shared with the round-5 program; its
finding format, closing sections and evidence rules apply unchanged). This ledger is
living until operator ratification, then frozen. It is a sibling of the frozen round-5
[research ledger](research-ledger.md) and upstream of the [build ledger](build/LEDGER.md),
whose state it never writes.

## CURRENT STATE

```text
projectStatus:   IN_PROGRESS
nextUnit:        B2
lastCompleted:   A2
blockedOn:       (nothing)
pauseRequested:  false
returnPass:      (none)
brief:           docs/experiments-brief.md
specOut:         docs/experiments-spec.md
memoryRoot:      docs
autonomy:        checkpoint-per-stream
round:           1
updatedAt:       2026-11-06T23:20:37Z
```

**Current phase:** wave 1 (A1, B1, C1, C3, E1, F1) dispatched in parallel to fresh-context workers; `nextUnit` names the first of them.

**Working location.** The shared build worktree `/Users/stevenvitali/MetaHarness-harnessharness`
belongs to the session driving the round-5 chain. This program works only in the separate
worktree `/Users/stevenvitali/MetaHarness-harnessharness-experiments` on branch
`claude/experiments-spec` (base `main` `75883daa`). Workers never run `git checkout`, `git stash`
or any branch-moving command in either worktree.

## 0. Working theses (to be tested, not assumed)

- **T1 — The executed $0 record supports a narrower claim than its summary states, and $0 hardening can strengthen it.** The writeup must match what ran (which surfaces, which code, which R, which estimators). (A1, A2)
- **T2 — HH can be the execution surface for third-party harnesses, with every model call mediated, at bounded engineering cost.** The hosting adapter shapes exist only as fixtures; the cost of making them real is unknown. (B2, C2, C4)
- **T3 — Terminal-Bench 2.1 through Harbor can be re-proven at $0 on HH cells, on budget-feasible compute.** Task format, verdict channel and resource needs differ from TB 1.0. (B1, B2, B3)
- **T4 — Harness differences with a fixed open model are small relative to run-to-run noise; an affordable tranche may honestly report inconclusive.** A sound null/inconclusive result is admissible. (C1, C3, D1)
- **T5 — HIR can faithfully represent mini-swe-agent (then Terminus-2) once named kernel gaps close, and deterministic request-level conformance is achievable.** (D2, E1, E2)
- **T6 — One-factor-at-a-time sweeps over HH-native knobs yield interpretable effects only when the knobs are real slots and the workload exercises them.** (D3, E2)
- **T7 — A records-generated static site can match the reference sites' quality without new dependencies and without displaying any figure the records do not hold.** (F1, F2)

## Execution contract

One row is a bounded unit for a fresh context, ending in its listed note and an explicit
answer to its question. A worker reads the brief, this ledger's row and dependencies, the
relevant repository instructions (root and nested `AGENTS.md`), then the necessary sources.
Read-only fan-out inside a row is allowed; a row writes exactly one note at its reserved path
and nothing else.

Only the coordinator (this session) changes `CURRENT STATE`, row status, the §Q register, the
change log and the outline trace. Before dispatch the coordinator records an assignment in the
change log (unit, baseline, allowed writes, inputs, dependencies, output, stop conditions). An
assignment authorizes only its recorded scope.

**Standing limits for every worker:** no spend of any kind (no model inference, no paid API
call, no provisioning); no external writes, posts, issues or PRs; no `git` commands that move
branches or touch the stash; no edits outside the reserved note path; no secret values in any
note (credentials are `provided: yes/no`). Free read-only metadata endpoints are allowed only
where an assignment names them. Web sources: primary sources first, exact URL and retrieval
time, `INACCESSIBLE` when a page cannot be read.

A row is complete when its note answers the question, records limitations, emits candidate
requirements (or explicitly none) and lists open questions with owners. A negative or
inaccessible finding can complete a row; it cannot discharge dependent claims.

**Candidate requirement IDs** use `REQ-X<stream>-<n>` with per-row hundreds pre-allocated in
the tables below (e.g. A1 emits `REQ-XA-001…099`, A2 `REQ-XA-101…199`). Synthesis folds them
into the spec's convention verbatim; IDs are append-only and never renumbered.

## A. Phase 1 — the executed record

| id | item | owner | status | evidence |
|---|---|---|---|---|
| A1 | **Executed-record audit.** Exactly what the $0 program executed: per record (S3 tranches 1–3, S4 confirmation/egress/completion/30-slot repairs, R5.59 closure if landed), which surfaces and code paths ran (scratch vs committed; HH cells vs direct podman), R, participants, arms (intended vs served), estimators actually invoked, repairs, costs, and which claims each record does and does not support. Reconcile against the brief §2 and the public surfaces (`docs/evidence.md`, `docs/status.md`, README). Inventory the scratch drivers (archive `~/tb-work/scratch-archive-2026-11-06/` + sha256 manifest). Reqs `REQ-XA-001…`. | R | done | [24_executed-record-audit.md](research/24_executed-record-audit.md) |
| A2 | **Methodology record and $0 hardening design.** Driver promotion (what is committed where, tests, reproducibility recipe), the records-in HH-cell×`tb` verdict cross-check, running the landed `parity_admission`/`kl_equivalence` over the records with honest outcome, record structure, README link, `docs/evidence.md` card transitions and the `check-position.sh` changes they need. Depends A1. Reqs `REQ-XA-101…`. | D | done | [30_methodology-record.md](design/30_methodology-record.md) |

## B. Benchmark and execution infrastructure

| id | item | owner | status | evidence |
|---|---|---|---|---|
| B1 | **Terminal-Bench 2.1 + Harbor ground truth.** Dataset identity/pinning (Harbor Hub vs git), task format, per-task resources/GPU/internet needs, the oracle agent, podman backend and egress controls, custom model base URL, trajectories (ATIF), attempts conventions, leaderboard integrity rules, task feasibility on a single GCE VM, contamination canaries; the TB 1.0→2.1 differences that matter to HH. Reqs `REQ-XB-001…`. | R | done | [25_tb21-harbor.md](research/25_tb21-harbor.md) |
| B2 | **Committed experiment runner and execution-surface port.** One in-repo runner shared by every phase (slot scheduling, cell lifecycle via `EnvDriver`/`hh-helper`, verdict channels — TB 1.0 parser and Harbor reward file — receipts, O-surface invocation of `tb`/`harbor`, records), its layering under K-2/CC1, and the $0 oracle re-proof design on TB 2.1. Depends A1, B1. Reqs `REQ-XB-101…`. | A | in-progress | → `design/31_experiment-runner.md` |
| B3 | **Compute backend and cost accounting.** GCE recipe as a committed re-runnable artifact, sizing against B1's resource census, quote method, unskippable teardown with receipt, a structured instance-hour cost field, and reconciliation with model spend. Depends B1. Reqs `REQ-XB-201…`. | A | in-progress | → `design/32_compute-backend.md` |

## C. Model mediation and harness hosting

| id | item | owner | status | evidence |
|---|---|---|---|---|
| C1 | **Ollama Cloud and `glm-5.3-flash` ground truth.** Endpoints (OpenAI chat, Anthropic messages), tool calling, seed/temperature/determinism, served-model identity/fingerprinting, prompt caching per endpoint, concurrency and rate limits, usage/balance endpoints and their latency, spend-cap controls, retirement risk, off-peak pricing; implications for receipts and budget enforcement. Free metadata endpoints (`GET /api/tags`, `/api/show`, `/api/balance`) allowed with the operator's key read from `~/MetaHarness-harnessharness/.env` — never printed, never recorded; no inference. Reqs `REQ-XC-001…`. | R | done | [26_ollama-glm.md](research/26_ollama-glm.md) |
| C2 | **Model-call mediation proxy (adapter B made real).** An HH-gateway-backed OpenAI-chat + Anthropic-messages endpoint reachable only from cells; per-slot budget enforcement through `hh-budget` `Account`; verbatim model-call receipts; model identity records; spend reconciliation with Ollama `/api/usage`; a `model_proxy` egress profile; the `evaluate_costs` $0-rule amendment and the slot-record fields paid runs need. Depends C1. Reqs `REQ-XC-101…`. | A | in-progress | → `design/33_model-mediation.md` |
| C3 | **Harness survey and admission criteria.** Candidates from the HF harness leaderboard, Harbor's agent list and Ollama's integration list (and others found): license/ToS (incl. automated benchmarking of Claude Code), headless mode, pinned versions, Harbor adapter, base-URL support for Ollama's OpenAI/Anthropic endpoints, caching dependence, install reproducibility, trajectory export, known issues; a pilot shortlist with expected $/trial. Reqs `REQ-XC-201…`. | R | done | [27_harness-survey.md](research/27_harness-survey.md) |
| C4 | **Harness hosting (adapter C made real).** Installing a pinned harness into the HH cell (reuse of Harbor's install templates vs own), offline-buildable images, configuration through the model proxy, trajectory capture, and the per-harness runner check (HH cell vs Harbor). Depends B2, C2, C3. Reqs `REQ-XC-301…`. | A | open | → `design/34_harness-hosting.md` |

## D. Methods and admission

| id | item | owner | status | evidence |
|---|---|---|---|---|
| D1 | **Comparative-evaluation method.** Estimands, reuse of the admitted §4.3 machinery (finite-manifest Beta-binomial, whole-task paired bootstrap, Holm), HF-style metrics (ties with leader, flip rates, all-k/any-k, $/solve) and which need method admission, the pilot→quote→gate design with a precision simulation, seeded TB 2.1 task selection, pre-registration packet, infra-failure missingness, reward-hacking/cheating scan policy, stop rules. Depends B1, C1, C3. Reqs `REQ-XD-001…`. | D | in-progress | → `design/35_comparative-method.md` |
| D2 | **HIR parity method.** L1 request-level conformance (recorded-response replay, canonicalization), L2 paired task-verdict KL parity with gate-set margin and pilot-quoted N, L3 descriptive trace shape; dimension map and §4.4 admission for an LLM participant ("same participant" with a served model lacking a digest). Depends C1, E1. Reqs `REQ-XD-101…`. | D | in-progress | → `design/36_hir-parity-method.md` |
| D3 | **Sweep method.** OFAT explore on development tasks → pre-registered held-out confirmation; factor/level registry; multiplicity and winner's-curse controls; when a knob is exercised (e.g. compaction under a large context window); budgets. Depends D1, E2. Reqs `REQ-XD-201…`. | D | open | → `design/37_sweep-method.md` |

## E. HIR native representation

| id | item | owner | status | evidence |
|---|---|---|---|---|
| E1 | **Reference-harness anatomy.** mini-swe-agent (version the HF board pins, 2.4.6) and Terminus-2: exact loop, templates/prompts, action parsing, observation formatting, limits, submit protocol, error/format-retry handling, summarization, configuration, and how Harbor runs each. Reqs `REQ-XE-001…`. | R | done | [28_reference-harness-anatomy.md](research/28_reference-harness-anatomy.md) |
| E2 | **HIR and kernel gap closure.** Exec tool over `hh-embed/1` to podman cells, system-prompt/template slot, tool schemas on the wire, slot-configurable `StrategyParams`, `compaction_strategy` slot, observation-render policy, submit marker, Lab→cell→analysis glue for sweeps; mapping of each reference harness to an HIR document; which changes need ADRs or canonical-spec amendments. Depends E1, B2. Reqs `REQ-XE-101…`. | A | open | → `design/38_hir-native-gaps.md` |

## F. Results publication and site

| id | item | owner | status | evidence |
|---|---|---|---|---|
| F1 | **Reference-site study.** The HF harness leaderboard (Space source, blog draft, charts, metrics, interactions) and Artificial Analysis's coding-agents page (layout, charts, typography, colour, interaction); what makes each effective; accessibility and performance; what an HH-native visual identity could draw on. Reqs `REQ-XF-001…`. | R | done | [29_reference-sites.md](research/29_reference-sites.md) |
| F2 | **Results-site design.** Information architecture and pages, visual identity, components and charts, the records→site data contract (every figure recomputed from committed packets), a zero-dependency generator, Pages deployment under ADR-0362 path modes and `check-ci-gating.sh`, claims-language checks applied to the site, publication gates. Depends F1, D1. Reqs `REQ-XF-101…`. | D | open | → `design/39_results-site.md` |

## H. Program plan

| id | item | owner | status | evidence |
|---|---|---|---|---|
| H1 | **Program plan and synthesis readiness.** Phase ordering and dependency DAG, every gate (provisioning, spend, parity margin, publication) and HUMAN row, budget envelopes, the amendment/ADR register (incl. the TB 2.0→1.0 substitution never recorded, `evaluate_costs`, claims graduation), sizing for decomposition, cross-note conflict resolution. Depends on all other rows. Reqs `REQ-XH-001…`. | A | open | → `design/40_program-plan.md` |

## O. Spec synthesis & review process

| id | item | owner | status | evidence |
|---|---|---|---|---|
| O1 | Synthesize `docs/experiments-spec.md` from the accepted notes (front matter, §0 conventions with `req_id_pattern: REQ-X[A-H]-[0-9]+`, decision register, body by phase, appendices A–F incl. outline-superset proof). | S | open | — |
| O2 | Fresh-context adversarial review; dispositions in Appendix E (`accept/adapt/rebut/defer`). | S | open | — |
| O3 | Operator ratification; freeze this ledger; write `docs/experiments-decomposition-prompt.md`. | S | open | — |
| O4 | (Downstream, not a synthesis unit) `decompose-spec mode=extend` appending Round 6 after row 251, on top of `main` once R5.59 has landed (Q-X3/Q-X18). | S | open | — |

**Dispatch waves.** Wave 1 (parallel, no dependencies): A1, B1, C1, C3, E1, F1. Wave 2: A2, B2,
B3, C2, D2. Wave 3: C4, D1, E2, F2. Wave 4: D3. Wave 5: H1. Then O1–O3.

## Q. Operator decisions register

Answers are recorded verbatim (selected option label, plus any free text) with receipt date and
channel. Channel for Q-X1…Q-X19: the planning conversation, Claude Code session of 2026-11-06,
interactive question dialog; recorded 2026-11-06T22:49:08Z. Recommendations live in the notes;
an agent recommendation is never recorded as an operator answer.

| id | question (abridged) | decision (verbatim) | unblocks / new rows |
|---|---|---|---|
| Q-X1 | Process shape | "Full synthesize-spec (Recommended)" | this ledger; O1–O4 |
| Q-X2 | Phase 1 given scratch S3 H side, R=1, estimators not run | "Writeup + $0 hardening (Recommended)" | A1, A2 |
| Q-X3 | Ordering vs R5.59 / GCE teardown | "After R5.59; teardown as planned (Recommended)" | B3, O4 |
| Q-X4 | Funding model for paid phases | "Staged: pilot → quote → gate (Recommended)" | D1, H1 |
| Q-X5 | Phase-2 integration architecture | "HH cells + install + model proxy (Recommended)" | C2, C4 |
| Q-X6 | Model policy | "I actually have some Ollama credits we can use, $60 to start. The API key lives at ~/MetaHarness-harnessharness/.env. So let's do open models with Ollama" — then, choosing among Ollama models: "glm-5.3-flash (Recommended)". Credential `OLLAMA_API_KEY`: provided: yes (value never recorded). | C1, C2 |
| Q-X7 | Harness set | "Terminus-2, mini-swe-agent, Heavyweight OSS agent, Claude Code (closed)" + "All of the above, depending on how much these experiments cost. I'd even like ideally to try multiple of the OSS harnesses (and you may consider doing some research into which to look into. For example, one place to start might be this recent article from huggingface https://huggingface.co/spaces/joelniklaus/harness-leaderboard but I believe there are more harnesses than this worth looking into as well perhaps" | C3, C4 |
| Q-X8 | Benchmark | First answer: "I'm not sure offhand. I need to understand what these three benchmark task sets are and where they come from. We may consider reviewing https://huggingface.co/spaces/joelniklaus/harness-leaderboard as well for inspiration, but let's reason through this together to make sure we make the right choice. research as you see fit"; second answer (after a plain explanation of the editions and of `tb` vs Harbor): "TB 2.1 tasks (Recommended)" | B1, B2 |
| Q-X9 | Results website build | First answer (benchmark dialog): "…note that I do want to take a lot of heavy inspiration from the HuggingFace harness comparison blog. In fact, I'd like as part of this spec we're drafting to include a small website that effectively does what the HF blog post does but for the HH results, hosted on github itself from the repo"; build answer: "I like the idea of it being static, generated from records. But I'd also like it to be beautiful, ideally. You should use https://huggingface.co/spaces/joelniklaus/harness-leaderboard as inspiration primarily, though I'd also note that you can look at the style of https://artificialanalysis.ai/agents/coding-agents for additional UI/UX/styling inspiration. Ours should be as beautiful as either of those, but unique and appropriate for the HarnessHarness repo in some sense" | F1, F2 |
| Q-X10 | Site content | "Leaderboard + CIs + ties, Noise analysis, Per-task grid + trajectories, Methodology + honesty pages" + "All of the above, plus whatever else you critically decide makes sense, based on research and our own best judgement " | F2 |
| Q-X11 | Budget scope ($60 credit vs compute) | "$60 is a pilot only" | B3, D1, H1 |
| Q-X12 | Independent-replication anchor vs the HF board | "No" | D1 (out of scope: formal external replication) |
| Q-X13 | HIR target | "Both, staged" (mini-swe-agent first, Terminus-2 second) | E1, E2, D2 |
| Q-X14 | Phase-3 parity design | "Layered: conformance + verdicts (Recommended)" | D2 |
| Q-X15 | Phase-4 sweep design | "OFAT explore → held-out confirm (Recommended)" | D3 |
| Q-X16 | Publication posture | "In-repo record, gated site (Recommended)" | A2, F2, H1 |
| Q-X17 | Compute | "Fresh GCE VM per phase (Recommended)" | B3 |
| Q-X18 | Landing vs the other session | "Docs now, manifest after R5.59 (Recommended)" | O4 |
| Q-X19 | Check-in cadence | "checkpoint per stream works, but you should only pause for review or input from me if you need my judgement call or approval or aren't sure about a set of plausible alternatives" | §Execution contract |
| Q-X21 | Ollama credit mechanics for the pilot: on exhaustion of the $60 included credit with $0 purchased, does Ollama refuse or auto-bill; the included credit resets 2026-11-22T16:30:34Z without rollover — is the post-reset allowance authorized for the pilot, and is the account HH-exclusive during runs (settlement)? (C1 F10–F12) | | pilot-spend gate (raise at gate; no spend before) |
| Q-X22 | Reasoning-effort pin for `glm-5.3-flash` (thinking always on; unknown effort names silently map to `max`): pin `high`, pin `max`, or make effort a factor? (C1 F5) | | C2, D1 (recommendation in design; confirm at ratification) |
| Q-X23 | Authorize a cents-scale paid smoke before the pilot to verify seed honouring, cached-token fields and error shapes? (C1 open question 5) | | first phase-2 build gate (not before) |
| Q-X24 | Phase-1 hardening depth: records-in HH-cell×`tb` cross-check only, or also a fresh $0 TB-1.0 tranche with ≥5 attempts/task on HH cells? (A1 open question 1) | | A2 recommends; confirm at ratification |
| Q-X25 | Record the S4 supplement epochs (retries beyond the frozen bound, mixed arch/network) as a dated protocol amendment, and the TB 2.0→`terminal-bench-core==0.1.1` substitution as an ADR, before publication? (A1 open questions 2–3) | | A2, H1 |
| Q-X26 | Admit Claude Code as the closed-source arm under these terms: unmodified pinned binary, no Claude account, black-box arm only (not an HIR target), name without logo, Anthropic's "not supported for non-Claude models" disclosed? (C3 F6, open question 1) | "Admit under these terms (Recommended)" (received 2026-11-06, planning conversation dialog) | C4, D1; raise with stream-C checkpoint or at ratification |
| Q-X27 | Version-pin policy for admitted harnesses: latest stable at pre-registration, or the HF board's pins? (C3 open question 4) | | D1 recommends; confirm at ratification |
| Q-X28 | Pilot breadth within $60: e.g. 8 harnesses × 10 tasks × 2 attempts vs 6 × 15 × 2 (C3 open question 5) | | D1 recommends; confirm at pilot gate |
| Q-X29 | Reference configuration for each original harness: Harbor's defaults at the pinned Harbor/agent versions (mini-swe-agent has no step/cost limit there), or an explicit declared limit set; and pin LiteLLM (which builds mini-swe-agent's requests) or record the installed version? (E1 open questions) | | D2/E2 recommend; confirm at ratification |
| Q-X30 | Agent-phase network policy for paid runs (TB 2.1 allows internet; answer-fetching risk) | "Allowlist + scan (Recommended)" — agents reach only the HH model proxy plus declared package registries, enforced and logged per slot; trajectory scan still flags leaked-answer behaviour; not leaderboard-comparable | B2, C2, C4, D1 |
| Q-X31 | Long tasks in the paid subset | "Let's keep all nominally, but when it comes time to start running experiments, since there are so many tasks and model/harness/etc. combos and so on. perhaps we stratify and just start out with the lowest time / cheapest ones" | D1 (staged estimand by cost stratum), H1 |
| Q-X32 | ADR admitting each task's declared agent/verifier timeouts above HH's 900 s cap | "Yes, per-task declared (Recommended)" | B2, H1 |
| Q-X33 | Card text after phase 1: whether the useful-task card changes once S1 is re-folded, and the operations card's reason text (A2 Q-A2-1, Q-A2-2) | | ratification (A2 recommendations stand until then) |
| Q-X20 | Ratify the reviewed spec and hand it to decomposition | | O3 |

## Change log

- 2026-11-06T22:49:08Z — Seeded from `docs/experiments-brief.md` (round 1): theses T1–T7, streams A–F + H (17 rows), §O O1–O4, §Q with Q-X1…Q-X19 answered verbatim and Q-X20 open. Pre-charter inspection (coordinator, read-only) recorded in brief addendum A-2; scratch drivers archived copy-only to `~/tb-work/scratch-archive-2026-11-06/` (71 files, sha256 manifest `~/tb-work/scratch-archive-2026-11-06.sha256`; rebuildable `target/` dirs omitted). Incident: the coordinator's `git checkout -b` briefly moved the shared build worktree's HEAD off `codex/harnessharness-r5.59` (same commit, no file change, no commit misplaced); restored at once and all program work moved to a separate worktree.
- 2026-11-06T22:53:34Z — Wave-1 assignments (coordinator = this session; baseline `claude/experiments-spec` `abf26c97` on main `75883daa`): A1 → `research/24_executed-record-audit.md` (inputs: run/report records, R5.59 branch read-only via `git show`, scratch archive; no network); B1 → `research/25_tb21-harbor.md` (web primary sources; shallow public clones into the session scratchpad, read-only); C1 → `research/26_ollama-glm.md` (Ollama docs; free metadata endpoints `/api/tags`, `/api/show`, `/api/balance`, `/api/usage` only, key never printed; no inference); C3 → `research/27_harness-survey.md` (web + read-only clones); E1 → `research/28_reference-harness-anatomy.md` (read-only clones of mini-swe-agent @2.4.6 and Harbor; local tb 0.2.18 copy); F1 → `research/29_reference-sites.md` (web + read-only clone of the HF Space). Every worker: writes only its note; standing limits per the Execution contract; stop and report on any needed operator decision.
- 2026-11-06T23:04:54Z — Accepted A1 (`research/24`, 15 findings; spot-checked: S3's 30/30 = 22 both-resolved + 4 both-test-fail + 4 both-build-fail), C1 (`research/26`, 16 findings; spot-checked `hh-budget` integer micro-unit per-token rates vs $0.15/M), F1 (`research/29`, 13 findings) — each with header, finding format and four closing sections. Coordinator visual pass of both reference sites done (Chrome, narrow viewports; notes held in the session scratchpad and passed to F2's assignment). New pending §Q rows Q-X21…Q-X25 (none blocks wave 2). R5.59 observed merged to `main` as `09fe8523` (per A1). `nextUnit` stays A1's wave: B1 next open in-progress row.
- 2026-11-06T23:06:27Z — Wave-2 assignments: A2 → `design/30_methodology-record.md` (inputs: `research/24`, check-position battery, layout rules; no network/execution), C2 → `design/33_model-mediation.md` (inputs: `research/26`, hosting/gateway/budget/env/helper code; owns the proxy replay/record mode shared with D2/C4). Accepted C3 (`research/27`, 14 findings; shortlist mini-swe-agent, Terminus-2, Claude Code, Pi, Hermes Agent, OpenHands, Kimi Code, OpenCode; spot-checked the Cursor CLI exclusion against Harbor adapter capabilities as quoted). New pending §Q rows Q-X26…Q-X28. Codex/Responses pass-through is a C2 design input (C3 open question 2).
- 2026-11-06T23:06:49Z — Accepted E1 (`research/28`, pins mini-swe-agent v2.4.6 `a83fcae8`, Harbor `ff2a1ef4`/v0.25.0; both originals drivable against a replay endpoint unmodified; 17 HIR gaps). E1's proposed $0 "L1 capture spike" needs installs and execution, which research rows may not do: carried to H1 as a candidate first phase-3 build ticket, not a ledger row. Wave-2 assignment: D2 → `design/36_hir-parity-method.md` (inputs `research/26`, `research/28`, spec §4.4, parity_admission/directional_kl code; consumes C2's replay mode as an interface). New pending Q-X29.
- 2026-11-06T23:15:26Z — Accepted B1 (`research/25`, 20 findings; TB 2.1 = 89 tasks pinned git `12b75656` + Hub ref + per-task digests; spot-checked the 39-task verifier-timeout census). Stream A–C checkpoint presented to the operator; answers recorded verbatim: Q-X26 (Claude Code admitted under stated terms), Q-X30 (allowlist + scan), Q-X31 (keep all 89 nominally; start with the cheapest/shortest stratum), Q-X32 (per-task declared timeouts via ADR). Wave-2 assignments: B2 → `design/31_experiment-runner.md`, B3 → `design/32_compute-backend.md`. Ownership: B2 owns cell egress-profile composition (registry allowlist + proxy reachability); C2 owns the proxy endpoint itself.
- 2026-11-06T23:20:00Z — Accepted A2 (`design/30`, 14 findings, REQ-XA-101…127). Coordinator spot-check of A2's new HH-cell × `tb` cross-check: discordances `swe-bench-langcodes` and `grid-pattern-transform` (tb resolved, HH cells unresolved on both first slots) CONFIRMED from the committed packets; denominator differs (coordinator: 17 tasks/15 agree using the confirmation packet's `first` slots with resolved/unresolved receipts; A2: 16/14) — the exact slot-inclusion rule is an implementation-ticket acceptance item and an O2 review item. A2 recommendations recorded against Q-X24 (records-in for phase 1; optional gated phase 1b) and Q-X25 (dated deviation record + retroactive ADR); new pending Q-X33.
- 2026-11-06T23:20:37Z — Assignment: D1 → `design/35_comparative-method.md` (dependencies B1, C1, C3 accepted; inputs research/24–27, supplement §3/§4.1/§4.3/§4.5, analysis/eval code; local no-network simulations allowed).
