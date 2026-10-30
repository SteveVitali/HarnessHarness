# 05_ecosystem-and-benchmarks — Adjacent systems and first-wave integration choices

Stream: B — Audiences and ecosystem

Researched: 2026-10-29 (UTC)

Author: Codex research worker `/root/b2_ecosystem`

Ledger rows covered: B2

Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved planning package remains uncommitted; concurrent research notes are outside this worker's scope.

Confidence overall: medium

The recommended first wave is a small native harness comparison with independently checked task outputs, a version-pinned external task/runner compatibility check, and inspectable receipts. Harbor and Inspect are integration candidates; they already provide substantial execution, evaluation and inspection functionality. HarnessHarness should explain its explicit runtime, experimental and evidence contracts without claiming that those individual capabilities are unique. No external system or benchmark was executed for this note.

## Scope and source record

This is a bounded decision survey, not a comprehensive market map. Selection covers the frozen benchmark anchor, two evaluation/interop systems, an adjacent durable runtime, optimization work, and recent harness studies. It does not rank products or establish user demand. The founding brief, including its approval addendum, note conventions, B2 row and outline obligations were read. `bash scripts/deferrals-open.sh` returned zero open rows.

Every source below was opened and its relevant text read. S1–S11 and the GEPA repository landing page were re-fetched in one confirmation batch completed at **2026-10-29T18:10:45Z**; S12's pinned README was fetched at **2026-10-29T18:10:52Z**. These are retrieval completion timestamps, not publication dates or execution evidence. Unversioned documentation is a dated observation, not a reproducible release pin; a feasibility check must select and lock the installed package revision.

| ID | Primary source and inspected locator | Publication/version information |
|---|---|---|
| S1 | [Terminal-Bench benchmark catalogue](https://www.tbench.ai/benchmarks), version entries | Lists 2.0 (2025-11-07), 2.1 (2026-05-06), 3.0 (2026-07-30), 4.0 (2026-08-28); living page |
| S2 | [Harbor task overview](https://docs.harborframework.com/tasks/overview), task layout, rewards, multi-step tasks, ignored files | Undated, unversioned documentation |
| S3 | [Harbor ATIF](https://docs.harborframework.com/agents/atif), structure, validation, versions | Documents ATIF-v1.7 and acceptance of v1.0–v1.7; page undated |
| S4 | [Inspect introduction](https://inspect.aisi.org.uk/), Welcome | Undated, unversioned documentation |
| S5 | [Inspect log files](https://inspect.aisi.org.uk/eval-logs.html), EvalLog, score history, recomputation | Describes EvalLog format version 2; page undated |
| S6 | [Inspect compaction](https://inspect.aisi.org.uk/compaction.html), Overview and Basic Usage | Undated, unversioned documentation |
| S7 | [LangGraph checkpointers](https://docs.langchain.com/oss/python/langgraph/checkpointers), Replay and Durability modes | Undated documentation; not pinned to an installed package release |
| S8 | [AI Agents That Matter](https://arxiv.org/html/2407.01502v1), §§2, 5–6 and Appendix C; [metadata](https://arxiv.org/abs/2407.01502) | arXiv v1, 2024-07-01 |
| S9 | [Harness-Bench rendered paper](https://arxiv.org/html/2605.27922v2), §§3.3–4.1 and Appendices A–B; [abstract record](https://arxiv.org/abs/2605.27922v2) | arXiv v2 dated 2026-10-15; fetched surfaces disagree on study population (F7) |
| S10 | [Trial by Trace](https://arxiv.org/html/2610.32285v1), §§3.2–3.6, 4.1–4.2; [metadata](https://arxiv.org/abs/2610.32285) | arXiv v1, submitted 2026-10-23; paper title page also says 2026-10-14 |
| S11 | [GEPA paper abstract and version record](https://arxiv.org/abs/2507.19457v2) | v2, 2026-02-14; full HTML fetch failed with a content-length error, so only the abstract supports this note |
| S12 | [GEPA README at 80a62c8](https://github.com/gepa-ai/gepa/blob/80a62c879ecdeb8741faac72a844dceb1c18691f/README.md), Quick Start, optimize_anything, Adapters | Commit `80a62c879ecdeb8741faac72a844dceb1c18691f`; GitHub commit API read at 18:10:44Z reported commit date 2026-10-28T03:48:37Z |

Local evidence was inspected at the baseline above. Initial source reads completed by **2026-10-29T18:10:52Z**; bounded test-source inspection followed, and the evaluation-plan follow-up is timestamped separately below. All are source inspection, not execution. The main locators are:

- [Evaluation plan source section](../../spec/sections/10-evaluation-reproducibility-plan.md), lines 22–65, inspected 2026-10-29T18:15:40Z: immutable designs, outcome classes, permitted headline oracles, task-cluster uncertainty and matched-budget rules. These are existing binding rules to reuse, not new obligations inferred from external literature.
- [Canonical specification](../../spec/CANONICAL_SPEC.md), lines 9343–9418: §10.3 benchmark contract, §10.4 reproducibility levels and §10.5.1 compaction exemplar.
- [Architecture](../architecture.md), “One boundary for clients,” “Decisions and durable state,” and “The Lab”; [status](../status.md), implemented surfaces and replay/client qualifications. These describe existing surfaces, not a fresh verification run.
- [Fixture adapters](../../crates/hh-bench/src/adapters.rs), lines 1–60; [adapter contract](../../crates/hh-bench/src/adapter.rs), `ExportFormat` and `BenchmarkAdapter` (lines 265–289, 426–495).
- [Bundle foreign formats](../../crates/hh-bundle/src/foreign.rs), direction declarations, `lift_inspect` (264–337) and `export_inspect` (725 onward); [benchmark lowering](../../crates/hh-bench/src/foreign.rs), `export` (130–217).
- [R4.9 test source](../../crates/hh-bundle/tests/r4_9.rs), `inspect_eval_log_lifts_header_scores_and_losses` (334–367), and [benchmark acceptance source](../../crates/hh-bench/tests/acceptance.rs), fixture process setup (1–45). Assertions were read, not executed.

## Material findings

### F1 — The current benchmark catalogue differs from the binding anchor

Claim: Terminal-Bench's current version catalogue does not authorize replacing the frozen Terminal-Bench 2.0 anchor.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S1 lists later releases. Canonical §10.3 binds stratum A to Terminal-Bench 2.0, with 89 tasks and at least five replicates; §10.5.1 uses that family for the paired compaction exemplar. §10.3 requires a parity report before adapter headline eligibility and admits no-parity smoke only at product granularity.

Retrieved: external 2026-10-29T18:10:45Z; local inspection by 18:10:52Z.

Implication for the spec: Resolve exact task revision, runner, verifier and image pins. Retain the anchor unless the operator accepts an ADR amendment; publish later versions as separate strata if approved.

Outline delta: CONFIRMS OL-21; EXTENDS OL-04 with a concrete version decision.

Limitations: The catalogue establishes version availability, not task validity, licensing, runner compatibility or superiority. This note does not revalidate the historical 89-task count against a downloaded suite.

### F2 — Harbor offers useful task and trajectory boundaries

Claim: Harbor's published task layout and ATIF are plausible integration boundaries, but artifact exchange is distinct from runnable compatibility.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S2 separates instruction, environment, solution and tests, with verifier rewards written to defined log paths. S3 documents messages, tool calls, observations and metrics in ATIF-v1.7, schema validation, and distinct capabilities for producing versus loading trajectories. Local `BenchmarkAdapter` separates materialization, exposure, submission collection and grading; the inspected concrete adapter family is explicitly fixture-backed.

Retrieved: 2026-10-29T18:10:45Z; local by 18:10:52Z.

Implication for the spec: E1 should map a selected external task onto the existing three-surface task contract, preserve upstream grading, and test both runner paths before promoting claims. F1 should specify which native evidence an exchange omits.

Outline delta: EXTENDS OL-04, OL-16 and OL-20.

Limitations: No task container or ATIF validator ran. The presence of `harbor_trial_dir` in source does not demonstrate compatibility with the current Harbor installation. Bundle-plane trial import and benchmark-plane trial lowering are different operations and must not be conflated.

### F3 — Inspect is already an evaluation and receipt ecosystem

Claim: Inspect supplies composable evaluation and analysis facilities, while HarnessHarness already has a narrower Inspect JSON exchange seam worth validating.

Status: PARTIALLY VERIFIED

Evidence kind: source inspection

Evidence: S4 describes datasets, agents, tools, scorers, external-agent execution and sandbox extensions. S5 exposes evaluation metadata, sample scores, errors, usage statistics and score-edit history. Local `lift_inspect` retains header/sample/score claims while recording losses for plans, statistics and transcript interpretation; `export_inspect` lowers native verdict/metric events. The R4.9 fixture test asserts these claims and losses.

Retrieved: 2026-10-29T18:10:45Z; local test source inspected after 18:10:52Z.

Implication for the spec: Prefer a small real-log import/export and viewer check over building another evaluation viewer. Preserve the original artifact, correction history and typed loss report; foreign scores remain claims.

Outline delta: EXTENDS OL-04, OL-06 and OL-16.

Limitations: JSON support is not proof of native `.eval` archive support, installed-reader acceptance, complete semantic equivalence or task execution. No current Inspect package was installed or run.

### F4 — Compaction is a shared experiment topic, not a uniqueness claim

Claim: Inspect already exposes several context-reduction strategies, so a HarnessHarness compaction study must establish a scoped experimental result rather than novelty of the feature.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S6 documents automatic/native, summary, edit and trim strategies, configurable thresholds, and retention of core history while model-visible input changes. Canonical §10.5.1 defines the native paired `evict_oldest` versus `clear_tool_results` comparison with matched caps and zero search budget.

Retrieved: 2026-10-29T18:10:45Z; local by 18:10:52Z.

Implication for the spec: Preserve that interpretable native contrast first. Any later cross-framework comparison needs an explicit mapping of thresholds, fallback, tool-output handling, accounting and effective model input; similarly named strategies are not equivalent treatments.

Outline delta: CONFIRMS OL-09 and OL-10; EXTENDS OL-04.

Limitations: Documentation does not show equal compaction semantics or relative performance. Cross-framework implementation differences would confound a component-only efficacy claim.

### F5 — Replay and durability terminology needs explicit semantics

Claim: Another durable runtime's replay operation can mean fresh downstream execution, making an unqualified cross-framework replay claim misleading.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S7 says LangGraph replays from checkpoints by re-executing subsequent nodes, including model/API calls, and describes exit, asynchronous and synchronous persistence modes. Local architecture describes HarnessHarness replay as re-driving recorded behavior; canonical §10.4 separately defines R1 derivation, R2 semantic re-execution and R3 statistical reproduction.

Retrieved: 2026-10-29T18:10:45Z; local by 18:10:52Z.

Implication for the spec: Name the procedure, external calls, effect handling and comparison criterion whenever using “replay,” “resume” or “reproducible”; document failure windows in bounded recovery claims.

Outline delta: CONFIRMS OL-07 and OL-12.

Limitations: This is a documented semantic comparison, not a crash-consistency test or a ranking of persistence mechanisms. No absence of stronger guarantees elsewhere in LangGraph is asserted.

### F6 — Evaluation design should join cost, generalization and reproducibility

Claim: Published agent-evaluation methodology supports matched resource reporting and holdouts chosen for the intended generalization claim.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S8 discusses accuracy–cost tradeoffs, holdouts at different levels of task generality, and evaluation-script differences that undermine comparability. Its benchmark survey is historical evidence about the surveyed systems, not a current census. Canonical §§10.1–10.2 already require outcome classes, task-cluster uncertainty and matched budgets; §10.3 binds split-before-search and contamination stratification, and §10.5.1 binds the first compaction contrast's resource dimensions.

Retrieved: 2026-10-29T18:10:45Z; local §§10.1–10.2 inspected 18:15:40Z.

Implication for the spec: D1/D2 should state the population to which a result may generalize, report failures and cost beside success, retain the upstream oracle, and distinguish selection cost from final-arm execution cost.

Outline delta: CONFIRMS OL-10, OL-11 and OL-15.

Limitations: The paper does not determine HarnessHarness's sample size, minimum useful effect or budget. These require the selected task population and pilot evidence.

### F7 — Harness-Bench is relevant but its fetched population claims disagree

Claim: Harness-Bench supports configuration-level study design, but the fetched abstract and HTML cannot support one unqualified numerical population statement.

Status: PARTIALLY VERIFIED

Evidence kind: source inspection

Evidence: S9's v2 abstract record describes 106 tasks and 5,194 trajectories; its v2 HTML describes 300 tasks, seven harnesses and ten backends in the general-task block. The rendered §3.3 uses final-artifact module checks; Appendix A says each task/model/harness cell is evaluated once. Both surfaces study harness configuration effects.

Retrieved: both versioned URLs re-fetched 2026-10-29T18:10:45Z.

Implication for the spec: Treat it as a later external participation/replication candidate; resolve the authoritative release and data manifest before numerical reuse or integration. Repeated native comparisons should estimate variability rather than borrow its reported rankings.

Outline delta: EXTENDS OL-04; CONFIRMS OL-15 and OL-16.

Limitations: The disagreement's cause is unresolved; no correction by the authors is inferred. No performance number is adopted, and the current external study has not been reproduced here.

### F8 — TraceTrial offers a useful separation of outcomes and mechanism evidence

Claim: TraceTrial-Bench's layered evidence model is relevant to bounded recovery trials, but its attribution labels are not established causal conclusions.

Status: VERIFIED

Evidence kind: source inspection

Evidence: S10 §3.4 separates apparatus admission, deterministic criteria and semantic judgments. §3.5 explicitly qualifies stored attribution labels as operational summaries. §4 records selected executions, including replacements, without within-cell repeatability estimates; scheduling and judge-budget differences limit some comparisons.

Retrieved: 2026-10-29T18:10:45Z.

Implication for the spec: D3 should separate task success, mechanism activation, sufficient evidence, confirmed invariant failure and candidate explanations. Semantic judging remains outside the canonical headline-oracle set (§10.1); retain its distinct detector and evidence class. Keep every attempt and its disposition if recovery reruns occur.

Outline delta: EXTENDS OL-12 and OL-16; CONFIRMS OL-15.

Limitations: This recent preprint is methodological input, not independent validation of HarnessHarness. Its released runtime and archive were not exercised; no numeric score or causal effect is transferred.

### F9 — Optimization should remain a separate later study

Claim: GEPA offers a plausible external candidate-generation interface, but incorporating adaptive search would change the first compaction experiment's estimand and cost accounting.

Status: PARTIALLY VERIFIED

Evidence kind: source inspection

Evidence: S11's abstract describes reflection over trajectories to propose prompt updates. S12 documents evaluator-driven optimization, train/validation inputs, evaluation caps and a `GEPAAdapter` interface. Canonical §10.5.1 fixes the initial compaction comparison at `search_budget = 0`; §10.3 requires split assignment before adaptive search.

Retrieved: S11 at 2026-10-29T18:10:45Z; pinned S12 at 18:10:52Z.

Implication for the spec: C1 should put optimization/evolution later. A future adapter can propose candidates while HarnessHarness enforces admitted definitions, held-out evaluation, resource ceilings and provenance; feasibility remains untested.

Outline delta: EXTENDS OL-04 and OL-14; CONFIRMS OL-10.

Limitations: The GEPA full-paper HTML was inaccessible to the fetch tool; no headline efficiency numbers are used. No dependency admission or working adapter is proposed as already achieved.

### F10 — Differentiation should be a testable contract-level value proposition

Claim: The supported positioning is a runtime and Lab for inspecting controlled harness changes under explicit evidence contracts, not exclusive ownership of logging, replay, evaluation or optimization.

Status: UNVERIFIED

Evidence kind: design proposal

Evidence: F2–F9 establish overlap; local architecture and canonical §§10.3–10.5 establish the intended combination of native durable records, distinct task/instrument surfaces, matched comparisons, evidence admission and reproduction levels. Existing fixture adapters and narrow foreign lifts constrain current readiness.

Retrieved: source synthesis completed 2026-10-29; underlying inspections timestamped above.

Implication for the spec: Demonstrate one controlled change from executed task through independent grade and inspectable receipt; use A1/A2/E1 to decide what engineering remains. Ask prospective users whether this workflow helps them diagnose or replicate a real comparison.

Outline delta: EXTENDS OL-04 and OL-19; CONFIRMS OL-02 and OL-15.

Limitations: This is a positioning hypothesis, not verified user benefit, a uniqueness claim, or proof that all contract requirements are live-reachable.

## Comparison and first-wave decision table

Dispositions below are proposals for C1/C2 and the operator checkpoint, not authorization to install, run, publish or contact anyone.

| Adjacent surface | Relevant provision established above | HarnessHarness seam and missing evidence | Proposed disposition |
|---|---|---|---|
| Terminal-Bench / Harbor | Versioned task family, execution-oriented task layout (F1–F2) | Existing benchmark contract and adapter interface; actual selected task/environment/grader path and parity remain to be shown | First-wave feasibility priority: one pinned task through the original runner and HH path; diagnostic smoke only, not a parity or efficacy result; preserve TB2 contract |
| Harbor ATIF | Versioned trajectory exchange (F2) | Native evidence needs explicit lowering/loss mapping and external validation | Small optional receipt spike after task/grader feasibility; no assertion that export implies resumability |
| Inspect | Evaluation components, logs and analysis (F3) | Existing JSON import/export and fixture assertions; installed-reader/viewer and real-log acceptance unverified | First-wave bounded interchange check if it does not delay native study execution |
| Inspect compaction | Comparable strategy family (F4) | Name similarity does not establish identical treatment | Later external replication/control; native two-arm contrast first |
| LangGraph | Checkpoint/resume/replay semantics (F5) | Different durability unit and effect boundaries | Reference for terminology and recovery protocol; defer integration until an audience job justifies it |
| Harness-Bench | Native configuration comparison (F7) | Version/population inconsistency and participant adapter unresolved | Later participation/replication; resolve manifest before reuse |
| TraceTrial-Bench | Mechanism evidence alongside task outcomes (F8) | Oracle, observation-boundary and grading alignment required | Borrow design questions for D3; defer full benchmark adapter |
| GEPA | Evaluator-driven candidate search (F9) | Search budgets, split discipline and candidate admission required | Later optimization study, not a first-wave compaction arm |

## Practical implications and collaboration opportunities

1. **Prove the execution seam before expanding the catalogue.** E1 should identify whether a pinned external task can actually execute, produce a submission, invoke its original verifier and enter the intended Lab comparison. Source symbols and a fixture round trip are insufficient. If blocked, record the engineering gap; do not replace the controlled live comparison with fixture efficacy claims.
2. **Make one portable receipt useful to an existing tool.** F1/F3 can choose Inspect JSON or ATIF, test a pinned external consumer, and expose every omitted native field. Retain native receipts as authoritative; avoid rebuilding complete foreign viewers or treating their logs as lossless native state.
3. **Offer small, reviewable contributions later.** Potential collaboration artifacts are an adapter parity report with a minimal failing task, an interchange conformance fixture and loss map, or a reproduction package for one study contrast. These are more concrete than proposing a new public leaderboard. Outreach and upstream submission need separate authorization; none occurred.
4. **Record release and redistribution decisions.** A public paper, accessible task directory or documentation page does not establish permission to redistribute every task, container, model trace or solution. F1/F3 must check the selected artifacts' licenses and contamination restrictions before selecting publishable bundle members. This note performs no license clearance.

## Answer and completion assessment

B2's research question is answered sufficiently for the portfolio checkpoint: adjacent systems overlap substantially; Harbor task/runner compatibility and Inspect interchange are the most immediate integration candidates; recent harness studies sharpen the methodology but need not expand the first wave. The key differentiating hypothesis is explicit evidence and experiment discipline around native harness changes, to be demonstrated and tested with users.

The research is complete, subject to coordinator acceptance. Technical blockers for execution are task/grader reachability, external consumer compatibility and parity, owned downstream by E1/E3; they are not blockers to this bounded survey. Exact release selection, licensing and protocol resources remain decisions, not fabricated results. The survey is selective, documentation can drift, preprints are not independent replications, and there are no empirical product rankings or adoption measurements here.

Verification for this note: source inspection only; no Rust build/tests, benchmark execution, provider calls, crash tests or paid runs. The source/test references above do not claim a passing run. `bash scripts/check-living-docs.sh` passed (387 files scanned); nine original relative-link targets existed and all ten material findings had every required field. The final added source-section link was also checked. The no-index file diff was inspected; no runtime verification is claimed.

## Handoff

- Resolves B2's primary-source input to C1/C2, D1–D3, E1/E3, F1/F3 and G3; does not resolve A1/A2's implementation/evidence audit.
- Priority follow-up within existing rows: E1 maps the real task-to-verifier seam; C2 offers an explicit retain-TB2 versus amend-version decision if needed; F3 validates one pinned external reader.
- No new research row is required before the portfolio checkpoint. Later benchmark participation and optimization can be explicit catalogue dispositions.
- Potential conflict requiring reconciliation: any inherited claim that the current Terminal-Bench release is 2.0, that an adapter name proves external execution, that foreign export is lossless, or that current documentation proves efficacy. This note changes none of the frozen contracts or historical records.

## Open questions

| ID | Question | Owner / affected units | Operator decision needed? |
|---|---|---|---|
| B2-Q1 | Can the bound TB2 revision and original verifier be run under the required containment/accounting contract, or is a justified amendment needed? | E1/E3, C2, D2 | Yes if changing the frozen anchor; feasibility inspection itself needs no new decision |
| B2-Q2 | Which one pinned external consumer offers enough reader value to justify first-wave interchange work? | F1/F3, C2, G3 | At portfolio/resource checkpoint if it changes scope or budget |
| B2-Q3 | What authoritative Harness-Bench code/data manifest reconciles the fetched abstract and HTML population discrepancy? | G3 or a later external-replication unit | No for read-only resolution; yes before funded participation/outreach |
| B2-Q4 | Which selected upstream task, image, solution and trace artifacts may be redistributed, and which require restricted references? | F1/F3, E3 | Yes for publication policy; no authorization to publish is implied |

## Spec requirements emitted

These are candidates for synthesis, not amendments to the frozen specification. Existing §§10.1–10.4 requirements remain authoritative; these candidates make their application to the new examples, adapters and receipts explicit.

| ID | Candidate requirement and rationale | Findings / obligations | Proposed acceptance method |
|---|---|---|---|
| REQ-B-101 | Every benchmark-dependent claim names exact suite/task, runner, grader and environment revisions; contract-version changes require an explicit amendment disposition. Prevent silent substitution. | F1–F2; OL-04, OL-21 | Check a proposed study manifest against canonical §10.3 and reject missing pins or undocumented version changes |
| REQ-B-102 | Interchange claims name format/version, direction, external consumer, preserved fields and losses; foreign imported claims cannot acquire native authority from conversion. | F2–F3; OL-04, OL-16 | Round-trip one real artifact through a pinned external reader, inspect losses and exercise unsupported direction/version cases |
| REQ-B-103 | Public comparisons distinguish native component interventions from whole-system comparisons and document semantic/budget mismatches. | F4, F6–F8; OL-09, OL-10, OL-15 | Protocol review enumerates treatment differences, task/grader parity, model identities, repeats, failures and accounting before execution |
| REQ-B-104 | Every replay/reproducibility example names its procedure, external-call behavior, evidence level and comparison criterion. | F5; OL-07, OL-16 | Follow the example from a clean store and verify that its observed procedure matches its advertised guarantee |
| REQ-B-105 | Reliability reports distinguish task outcome, mechanism activation, evidence sufficiency, invariant violation and causal hypothesis; replacement attempts remain visible. | F8; OL-12, OL-15 | Review a success-with-violation, missing-evidence case and failed/retried run without coercing them into one success flag |
| REQ-B-106 | Adaptive optimization is a separate, budgeted design with pre-search splits and selection accounting; it cannot silently enter the zero-search compaction contrast. | F6, F9; OL-10, OL-14 | Validate design fields and accounting; reject unregistered candidate search or held-out feedback into optimization |
| REQ-B-107 | Ecosystem positioning links each claimed benefit to demonstrated behavior and includes overlap, current limitations and a scoped participation path. | F2–F10; OL-04, OL-19 | Editorial/source audit plus one newcomer or researcher walkthrough; do not mark usefulness verified solely from documentation |
