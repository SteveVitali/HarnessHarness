# 06_demo-and-study-portfolio — One task chain, several honest kinds of proof

Stream: C — Portfolio and feasibility  
Researched: 2026-10-29 UTC  
Author: /root/c1_portfolio  
Ledger rows covered: C1  
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved uncommitted planning package and accepted A1–A3/B1–B2 notes. No implementation changes.  
Confidence overall: medium — strong source basis for constraints and known gaps; task feasibility, effort and audience value remain unverified.

**Proposal for Q-2, not a portfolio approval:** serve a technically prepared harness evaluator deciding whether one context policy changes useful outcomes under fixed resources. Build one reusable chain: external task → native run → preserved submission → independent upstream check → Lab observation → analysis → native receipt. Teach inspection, replay, failure diagnosis and participation from that chain. Keep the real two-arm compaction comparison central; let E1/C2 expose its engineering and resource cost before committing execution.

The proposed first wave has a fixed foundation (one useful task and its evidence, a small offline Lab lesson, bounded operational cases and a bug/verification path), a central study contingent on feasibility and authorization, and two optional additions (one packaged component and one foreign receipt consumer). These are slices of shared infrastructure, not six independent integrations. There is no proposed new leaderboard, viewer, task suite, universal agent tutorial or large model-by-policy factorial.

## Basis and prioritization

Read [brief and approval](../brief.md), [C1/dependencies and decisions](../research-ledger.md), [conventions](../research/CONVENTIONS.md), [outline trace](../OUTLINE_TRACE.md), root/docs instructions and the external `synthesize-spec` run workflow. Accepted inputs: [A1 capability inventory](../research/01_capability-inventory.md), [A2 evidence audit](../research/02_evidence-audit.md), [A3 newcomer audit](../research/03_newcomer-journey.md), [B1 audiences/jobs](../research/04_audiences-and-jobs.md), [B2 ecosystem](../research/05_ecosystem-and-benchmarks.md). Citation shorthand below refers to those notes and their numbered findings, not fresh execution of their sources. Also read [evaluation contract §§10.1–10.5](../../spec/sections/10-evaluation-reproducibility-plan.md) directly. Inspection completed **2026-10-29T18:28:17Z**; all `Retrieved` fields use that checkpoint. External sources were not re-surveyed.

Prioritize by (1) the founding useful-task/controlled-comparison obligation, (2) reuse of the same execution and evidence chain, (3) independently checkable outcomes, (4) known integration and maintenance burden, then (5) incremental coverage or outside-reader value. These are ordinal judgments, not estimated utility scores or effort promises. A low-effort screenshot cannot outrank the task/grader bridge merely because it is easy.

| Order / candidate | Reader question and proposed disposition | Main prerequisite and cost driver |
|---|---|---|
| Foundation — S1 useful task → inspect/replay/share | Can I do a useful task, explain one decision and give someone a checkable failure? **Fixed first.** | Real task/environment/grader bridge; closure and R0 repair; storage and repeatable environment setup. |
| Flagship — S3 native live compaction | Does `evict_oldest` versus `clear_tool_results` change task outcomes and mechanism metrics at matched caps? **Central first-wave objective, contingent execution.** | S1 plus parity, detectors, repeated real runs, independent grading and frozen analysis; model, tool/environment and instrument cost. |
| Teaching — S2 offline Lab | Can I follow scheduling, outcome classes and comparison without model spending? **Fixed first**, small fixture derivative of S3's shape. | Supported runner/CLI inputs and checked analysis outputs; inexpensive execution but real integration/drift work. |
| Operational — S4 bounded recovery/authorization | What survives one interruption, and what prevents one forbidden action? **Fixed first**, tightly bounded cases on S1's apparatus. | Controlled effects/fault hooks, permission response and invariant detectors; environment/platform validation. |
| Participation — S5 report/verify/extend | Can I verify a claim, reproduce a bug, or change one component? **Fixed report/verification path; contingent component exercise.** | Shareable native receipt and existing contribution route; optional build/seal/load/invoke tooling adds portability cost. |
| Interchange — S6 one external consumer | Can I inspect this receipt in a tool I already use? **Optional first-wave ceiling of one direction/consumer.** | Pinned Inspect JSON reader or ATIF validator, explicit losses; dependency/version upkeep. |

## Material findings and scenario contracts

### F1 — One evaluator job provides a coherent selection rule

Claim: The first wave should optimize for a harness evaluator's bounded policy decision, with integration developers, replicators and contributors using the same run and evidence.

Status: UNVERIFIED  
Evidence kind: design proposal  
Evidence: B1 F1–F8 separates the operator's observed intent from untested external demand; B2 F10 supports contract-based positioning without uniqueness claims; A3 F1–F3 separates setup success from useful output and exposes selected-route defects.  
Retrieved: 2026-10-29T18:28:17Z.  
Implication for the spec: Select one primary job at Q-2; test reader understanding and usefulness later through G3/Q-6, without treating a working pipeline as adoption evidence.  
Outline delta: CONFIRMS OL-01, OL-03, OL-19; EXTENDS OL-14 — breadth comes from accountable family dispositions.  
Limitations: No users observed, no demand ranking, no validated time-to-value target; other audiences are not judged incapable of using the project.

The proposed useful-task selection brief is an existing, bounded terminal task with a concrete file/patch/end state and an upstream executable check; prefer a small project or pipeline repair that a reader can understand. E1 must find an eligible actual task and Q-3 pins its identity. This is a task-shape preference, not a claim that a named TB2 task already satisfies it. Do not author a private benchmark pool, alter upstream tasks for a better demonstration, or choose the confirmatory set because pilot outcomes favor compaction.

### F2 — S1 needs a real task and a repaired evidence path

Claim: A useful task, annotated trace, replay and shareable receipt should be one scenario with distinct acceptance steps, rather than independent showcases presumed complete.

Status: UNVERIFIED  
Evidence kind: design proposal  
Evidence: A1 F6/F8 identifies task/grader and replay boundaries; A2 F2–F7 finds overwritten attempts, limited native closure and infrastructure passes with format failures; A3 F1/F4–F6 executes fixture inspection/replay but observes successful-fixture R0 lifecycle drift.  
Retrieved: 2026-10-29T18:28:17Z.  
Implication for the spec: Budget engineering for executor/grader integration, attempt identity and export/reproduction semantics before promising the full path.  
Outline delta: CONFIRMS OL-05–OL-07, OL-15–OL-18; CORRECTS OL-03's effortless/pure-documentation framing.  
Limitations: These source facts do not settle E1's implementation choice or imply every historical bundle is affected.

**Job and claim:** “Produce one useful artifact and explain why its observed outcome is trustworthy.” A successful example proves that task on that configuration; an incorrect-output example proves the checker rejects that output. Neither establishes general capability, a causal policy advantage or reader time savings.

**Engineering and useful acceptance:** preserve actual task/submission/verifier artifacts; independently execute the oracle rather than author a success row; annotate real event IDs and causal links; report terminal, grade, veto and infrastructure status separately. Execute through the chosen supported boundary in scratch environments. Repair the R0 `finished`/`succeeded` mismatch and validate fresh export → clean-store reproduction, including an altered-status failure. Preserve every attempt with an immutable identity; date-plus-leg overwrite is unsuitable for study collection. Receipt-design units F1/F3 must specify content closure, publication rights and sanitization.

**Proof separation:** expose inspection, hash verification, analysis recomputation, deterministic replay and fresh execution as separate procedures. Start with proven R0 and selected replay scope; higher R1–R3 claims need their own evidence. Replay can append lifecycle records; check the intended decision/assembly sequence and original prefix, plus absence of new model/effect rows where promised, rather than whole-ledger byte equality. A clean local store alone does not establish independent reproduction. Cost drivers are environment setup, actual task calls/checks, retained blobs and portability validation, not prose alone.

### F3 — S2 is a small teaching derivative with its own interface work

Claim: An offline Lab lesson should teach the same factor and evidence vocabulary through explicit fixtures without being presented as task efficacy or as an already working CLI recipe.

Status: UNVERIFIED  
Evidence kind: design proposal  
Evidence: A1 F6/F7 distinguishes real scheduling/analysis from synthesized subject closure; A3 F3 reproduces `compare report` missing `/spec` and inspects claim/launch/settle missing fields; B1 F3 and B2 F6 separate example defaults from scientific justification.  
Retrieved: 2026-10-29T18:28:17Z.  
Implication for the spec: E1 chooses a minimal supported SDK/RPC runner or selected CLI repairs; unit F2 checks the actual workflow, inputs, semantic reports and intentional failures.  
Outline delta: CONFIRMS OL-08, OL-17; EXTENDS OL-20 — interface work is implementation scope.  
Limitations: A3 did not execute a full Lab lifecycle; underlying-engine presence does not remove runner work.

**Job and claim:** “Understand how an experiment turns declared arms and outcomes into a comparison.” Use a tiny deterministic fixture with two arms, repeated observations and at least one non-scored outcome; mark planted submissions/metrics and synthetic differences conspicuously. Reuse the live study's document structure and analysis path, not its conclusions. Acceptance requires clean-store open/claim/launch/settle/close, class-aware rows, persisted analysis and recomputation with expected typed failures; exact fixture values remain unit F2 work. A one-task illustration may correctly yield `n/a{estimator_undefined}`; never suppress a refusal to make a chart. Fixture execution and drift checks are cheap relative to live collection, but command-shape repair and truthful outputs are prerequisites.

### F4 — S3 remains the central real comparison under the existing contract

Claim: The flagship should begin with the native canonical two-arm compaction contrast and earn each scientific claim through the complete task/grader/parity/evidence chain.

Status: UNVERIFIED  
Evidence kind: design proposal  
Evidence: B2 F1/F2/F4/F6/F9 and the directly read evaluation contract §§10.1–10.5.1 fix benchmark, matching, oracle and admission conditions; A1 F3/F6 and A2 F5 show useful mechanisms but no identified controlled compaction efficacy receipt.  
Retrieved: 2026-10-29T18:28:17Z.  
Implication for the spec: D1/D2 design a pilot and frozen confirmation; E1/E3 identify actual executable bindings, graders, metering and admission gaps; C2 presents the cost of meeting the contract.  
Outline delta: CONFIRMS OL-09–OL-11, OL-15, OL-20–OL-21.  
Limitations: No task set, installed runner, model snapshot, budget or sample size is selected by this proposal; no effect is predicted.

**Job and claim:** “Should I prefer one of these two policies for this task population and resource envelope?” Prefer one fixed model level with roles fixed, `fault_profile=none`, and native `evict_oldest` versus `clear_tool_results`; avoid a new summarizer, cross-framework policy or search loop in the primary contrast. Report task success, reacquisition, repeated actions and the other required mechanism/accounting metrics; missing detectors remain typed `n/a`, never inferred wins.

**Contract floor:** retain Terminal-Bench **2.0**, its §10.3 stratum A definition (89 tasks), suite A held-out split and **at least five replicates** for the canonical contrast; D1/D2 resolve the precise task/split manifest and may require more repetitions for the intended precision. Preserve `matched_cap`, `same_snapshot`, `cold_start`, dimensions `{tokens.input.total, tokens.output.total, model_calls, time.wall_ms}`, tolerance `0.10`, and `search_budget=0` from §10.5.1. These binding values are not evidence that power, practical-effect margins or resources are adequate. Pin role accounting, including subject-charged summarizer/validation calls versus instrument-charged independent benchmark verification, without silently conflating the two.

**Feasibility and scientific gates:** first trace one pinned external task through the original runner and HH path as a diagnostic proposal for E2; until an admissible `ParityReport`, §10.3 allows only **product-granularity smoke with `parity=none`**, not a component headline. Preserve upstream oracles, agent/instrument isolation rules, hidden tests, network policy, immutable attempts and suite-validity/contamination strata. Then separately justify parity design and a pilot capable of showing whether compaction activates under useful task conditions. An exploratory pilot or selected demonstration subset does not satisfy canonical full confirmation or suite-level claims. Any new benchmark/version, task-set change that departs from the contract, or reduced contractual floor requires an explicit amendment disposition, not a silent substitution.

**Useful acceptance:** a real run produces submission → independent grade → accounted result → persisted analysis; a frozen confirmatory design reports paired effects, task-cluster uncertainty, tails, outcome bounds, multiplicity and all failures/exclusions. Null, negative or inconclusive findings can complete honest research and its engineering; a favorable effect is not a release test. Adoption of a new default remains a separate decision under applicable evidence gates. Dominant costs are task/environment reliability, parity runs, two-arm repeats, instrument checks and artifact retention; C2/Q-4 must price these rather than inherit old live spend or promise a weekend implementation.

### F5 — S4 should expose bounded operational invariants, not become another benchmark

Claim: Recovery, authorization and budget coverage should use a few controlled cases on the shared task apparatus with explicit assumptions and separate outcomes.

Status: UNVERIFIED  
Evidence kind: design proposal  
Evidence: A1 F5 bounds decision-point budgets and recovery/security mechanisms; A2 F6/F9 preserves skipped sublegs and narrow leak checks; A3 F4 shows that an allow trace alone proves neither denial nor an approval pause; B2 F5/F8 separates mechanism evidence from causal/general security conclusions.  
Retrieved: 2026-10-29T18:28:17Z.  
Implication for the spec: D3 defines the smallest fault/attack matrix and negative controls; E1/E3 verifies observable effect and permission boundaries before choosing an injection point.  
Outline delta: CONFIRMS OL-12, OL-15; EXTENDS OL-14 and OL-20.  
Limitations: No resilience, containment certification, prompt-injection resistance or exact-once guarantee is awarded by this design.

**Recovery slice:** one controlled interruption around a durable effect boundary, plus a matched no-fault control; inspect durable-prefix preservation, actual external effect count, terminal/grade and explicit unknown/unsupported states. Resume success is insufficient if a duplicate effect or missing evidence occurred. The injection point and recoverability guarantee are E1/D3 questions, not assumptions about the host.

**Authorization/budget slice:** one effect denied before preparation, one real permission pause/respond/resume case, and one budget-exhaustion case at a documented decision point. Observe the external sink as well as ledger decisions, prove denial/limit enforcement within the stated setup, and account for already committed effects. Reuse S1's environment/checker and S5's receipt format; deterministic fault/sink controls may establish bounded operational behavior, not live-model task efficacy. A gate-off control follows the applicable contract only in a confined test setup. Full AgentDojo utility/attack evaluation and HAL-style fault coverage remain later work. Cost is concentrated in reproducible fault hooks and platform assumptions; if these need a new environment backend, return scope to Q-2 rather than silently growing the first wave.

### F6 — S5 and S6 offer participation without a second product surface

Claim: The first wave should make one native failure/receipt verifiable and reportable, with a real component exercise and foreign-consumer check optional behind the core chain.

Status: UNVERIFIED  
Evidence kind: design proposal  
Evidence: A1 F9/F11, A2 F8/F9 and A3 F7 distinguish actual package invocation, partial client compatibility and narrow foreign conformance; B1 F5–F8 supports a small contribution lane; B2 F2/F3 recommends bounded exchange instead of rebuilding evaluation tooling.  
Retrieved: 2026-10-29T18:28:17Z.  
Implication for the spec: Units F3/G2 define report and replication recipes; E3/G2 validate one selected package path; units F1/F3 admit at most one external consumer if it provides concrete value within Q-2 scope.  
Outline delta: CONFIRMS OL-13, OL-16, OL-18–OL-19; EXTENDS OL-04.  
Limitations: A recipient checking a package is not necessarily an independent scientific replicator; no outreach, upstream submission or dependency admission is authorized.

**S5 fixed slice:** a skeptical reader verifies the distributed S1/S3 artifact, recomputes supported analysis, and files a minimal mismatch report through existing contribution policy. Acceptance includes exact versions/commands, semantic verdicts, artifact access, expected limitations and a useful negative report; fresh replication separately declares model/environment/resource requirements and follows unit F3/Q-6. No paid repeat or positive replication is required to contribute. Even a diagnosed setup failure can enter the contribution path.

**S5 contingent extension:** build, seal, load and invoke one existing compaction plugin as a packaging/conformance exercise; check actual invocation markers, altered-package rejection and absent-helper handling. Do not add it as a third scientific arm or advertise “no compilation.” Avoiding a kernel rebuild is a narrower claim to test. Defer if it needs an independent runner/environment; later trigger is a working S1 receipt and bounded packaging cost.

**S6 contingent exchange:** prefer the existing Inspect JSON seam for one real receipt and a pinned reader, if unit F3 confirms value and compatibility; choose ATIF instead only if it reduces the selected task integration burden. Preserve the native artifact and explicit loss/direction/version report. Acceptance is consumer validation plus field/loss inspection, not resumability or score equivalence. No new viewer or leaderboard; web screenshots may illustrate the same receipt only after setup/claim checks. Defer exchange if it competes with task/grader closure; revisit on a concrete external-reader job.

### F7 — Full-family coverage does not require a separate experiment per family

Claim: Every A1 family can receive an explicit disposition while concentrating first-wave work on one evidence chain and reserving broader causal comparisons for later.

Status: UNVERIFIED  
Evidence kind: design proposal  
Evidence: A1 capability matrix/F1–F12 is the coverage denominator; brief §5 names additional study interests; A2's first-wave gap table, A3's fourteen suggestion dispositions, B1 F6 and B2's decision table supply readiness and reuse rationale.  
Retrieved: 2026-10-29T18:28:17Z.  
Implication for the spec: Carry all 22 rows into C2/H2; a first-wave slice is not evidence for every feature in its family.  
Outline delta: CONFIRMS OL-03, OL-14, OL-20–OL-21.  
Limitations: These are proposed dispositions, not new historical deferrals or closure of existing contract obligations.

| A1 family (complete denominator) | Disposition / covered slice | Reason and explicit expansion/revisit condition |
|---|---|---|
| Run lifecycle and durable history | **First-wave:** S1/S4 | Foundation for every observation; expand platforms/faults only after one environment and invariant set work. |
| Definition assembly, identity and registry | **First-wave:** S1/S3 pinned edit/validate/compile loop | Make the changed policy identifiable; defer registry catalogue expansion until a second reusable definition needs it. |
| Providers, model profiles, routing and caches | **First-wave:** S1/S3 one lane/model level and declared cache policy | Control confounding and setup; later routing/cache/model study after stable accounting and a demand-backed question. |
| Context building and compaction | **First-wave:** S3 canonical two-arm contrast | Central obligation; broader builders/provider-native compaction later after binding/detector parity and the first report. |
| Retrieval, memory lifecycle and consolidation | **Later:** L1 | Adds corpus validity, embedding and cross-episode state; revisit with isolated memory assertions and a pinned task/corpus. |
| Skills and Procedure IR | **Later:** L2 | A procedure lesson could become a new executor; revisit once shared task ports can run one admitted procedure and check it. |
| Control and planning | **First-wave:** fixed baseline control in S1/S3; comparative study **later L3** | Needed to execute, held constant to identify compaction; revisit after first contrast and executable alternate bindings. |
| Delegation, topology and coordination | **Later:** L4 | Additional ports, interference and team-budget accounting; revisit after a real bound topology and matched solo baseline exist. |
| Value of computation / budgeted choice | **Later:** L5; fixed compute in S3 | Adaptive selection changes estimand and cost; revisit with split discipline and complete search/inference accounting. |
| Resource budgets and telemetry | **First-wave:** S1/S3 accounting, S4 bounded limit | Essential to interpret comparisons; defer optimization/frontiers beyond required reporting until metering is validated. |
| Authorization, approvals, containment and secrets | **First-wave:** S4 named deny/pause cases | Demonstrate observable controls; broaden only with an explicit threat model and environment coverage, never general assurance. |
| Verification and independent critics | **First-wave:** S1/S3 executable oracle | Independent grades are essential; broader/calibrated model-critic study later when a task demands it, with non-headline labels. |
| Recovery and environment lifecycle | **First-wave:** S4 one fault window | Bounded proof reuses S1; additional snapshot/hibernate/platform cases wait for advertised host capability and fault controls. |
| Replay, branching and causal attribution | **First-wave:** S1 selected replay; causal intervention **later L6** | Replay explains a receipt; attribution needs freshly executed matched counterfactuals, not supplied outcome rows. |
| Benchmark integration and grading | **First-wave:** S1/S3 selected external task, TB2/parity path | Highest execution uncertainty; expand suites only after independently graded bridge, parity and resource review. |
| Experiment scheduling and matching | **First-wave:** S2/S3 one supported runner | Reuse existing semantics and repair selected requests; distributed scheduling waits for a measured execution bottleneck. |
| Evaluation, results, comparison and analysis | **First-wave:** S2/S3 existing kernel and reports | Required for defensible outputs; new public ranking surface **excluded** absent an approved comparison/admission job. |
| Portable bundles and import/export | **First-wave:** S1/S5 native closure and corrected R0 | Makes evidence usable; higher levels/foreign formats expand only after exact procedures and access are feasible. |
| Plugins and external harnesses | **First-wave contingent:** S5 one compaction package; external participants **later L7** | Realistic extension path without another study; revisit hosted runners when task parity and class-scoped accounting work. |
| Fleet and external work sources | **Later:** L8 | C4 governance and external-system effects exceed one evaluator job; revisit with an operator/work-source use case and human gates. |
| Evolution, debt retirement, model drift and co-evolution | **Later:** L9 | Search/transfer/default gates multiply scope; revisit after baseline reports, pinned old/new models and governed candidate evidence. |
| Web, MCP, ACP, SDK and foreign conformance | **First-wave:** one supported SDK/RPC/CLI route; S6 optional exchange | Reuse native receipt; web supplementary, MCP/ACP/foreign lessons later after exact negotiation/update/corpus scope checks and a reader need. |

The later catalogue below adds the user question, proposed proof and engineering/cost driver to those dispositions; none is an implementation commitment. Prerequisites and revisit conditions are cumulative with the family table.

| Candidate / priority within later work | Reader question / bounded claim to test | Proof, prerequisites and dominant cost |
|---|---|---|
| L1 — memory/retrieval correctness before efficacy | Can I retrieve the right durable item without stale, invalid or leaked memory? | First deterministic lifecycle/retrieval assertions; then matched task comparison. Needs pinned corpus, embedding path, leakage detectors and per-episode reset; indexing and long traces drive cost. |
| L2 — one skill/procedure before a strategy survey | Can I execute and adapt one admitted procedure with observable control? | Build/compile/invoke/end-state conformance, then optional efficacy. Needs real workflow/program ports and versioned procedure; packaging and failure diagnostics dominate. |
| L3 — control/planning/context builders | Does a richer strategy improve this task family under the specified match mode? | Canonical control design before extra variants; separate context-builder intervention. Needs native bindings, required model levels, isolated factors and repeats; model cells and task runtime drive cost. |
| L4 — delegation and topology | Does this admitted team outperform a matched solo configuration, and where does coordination fail? | Controlled task comparison plus coordination invariants. Needs bound delegate ports, supported topology, total team accounting and task-level replication; parallel environments and interference drive cost. |
| L5 — value of computation/constrained optimization | When is extra computation worth its total cost? | Matched caps/frontiers and separately registered search design. Needs budgets, held-out splits, full selection spend and usable predictors; tuning and evaluation multiply runs. |
| L6 — branching/causal attribution | Did one recorded decision cause a different downstream result under a declared intervention? | Executed counterfactual arms with matched conditions and uncertainty. Needs effect-safe fork boundary, recorded treatment and real outcomes; branched runs/validity work drive cost. |
| L7 — external harnesses/benchmark participation | Can an outside runner consume tasks or receipts with known losses and parity? | Pinned runner/adapter conformance and parity before comparative claims; resolve Harness-Bench manifest discrepancy before participation. Additional environments and compatibility upkeep dominate. |
| L8 — fleet/work-source operations | Can an operator reconcile one external work source with durable, accountable decisions? | Confined source/webhook trials before organizational outcomes. Needs opt-in C4, credentials, cleanup and human accountability; external-state fixtures and repeated service checks dominate. |
| L9 — retirement, drift, evolution/co-evolution | Does a rule still earn its cost after a model change, and can a proposed replacement pass governance? | Retirement/transfer comparison with old/new snapshots, full search spend and applicable C4 gates. No weight-training claim. Selection, held-out families and repeated validation drive cost. |

**Excluded from this program's first-wave promises:** production certification, turnkey fleet/no-code product, universal cross-client/runtime compatibility, an authored benchmark suite, a new leaderboard/viewer, and broad claims that HarnessHarness uniquely supplies logging/replay/evaluation. Revisit only for an explicit reader/operator problem, supported evidence and Q-2/Q-7 scope decision (plus an ADR if a frozen constraint changes). Advanced features remain registered candidates; exclusion of a promise does not delete their contracts. Full security/recovery suites, cross-framework compaction and multi-model factorials are later candidates requiring their own protocols, not hidden first-wave acceptance.

## Answer and completion assessment

C1 answers the catalogue question: six connected scenarios, nine later study groups and all **22/22 A1 families** have proposed coverage, reason and revisit conditions. S1/S2/S4/S5's fixed slices are the proposed foundation; S3 is the central contingent scientific deliverable, not a stretch goal quietly traded for fixture results. E1 may change sequencing and expose infeasibility; C2 must surface those changes for Q-2. No live study, package workflow or complete executor/grader bridge is claimed resolved.

**Minimum useful release if study-scale feasibility fails:** retain one actual useful task with an independent check, its annotated native receipt and tested verification/replay/R0 procedures, the small offline Lab lesson, and a report/recompute path; keep the small operational cases where their shared apparatus is feasible. Publish the study's precise unmet gates and a bounded next decision instead of an efficacy claim. If even the useful-task bridge is infeasible, only a diagnostic/teaching package is available: it may help learners and bug reporters, but it does **not** satisfy the useful-task or live-comparison objectives. Q-2 must consciously rescope rather than call the full first wave complete. Null/negative results from a sound feasible study are not this fallback case.

## Open questions

| Concrete question | Owner / affected units | Decision boundary |
|---|---|---|
| Which actual external task, original runner and upstream executable oracle fit the useful-task brief and TB2 anchor; what visible/held-out/instrument surfaces and image pins are required? | E1 → C2/D2/E3 | Q-3 selects pins; changing anchor/contract requires explicit amendment. |
| Who owns task materialization, launched subject writer, execution, submission capture, separate or family-declared shared verifier, terminal facts and trusted Lab rows? Where do fixtures/manual rows enter today? | E1 → C2/E2/E3 | Source-backed options now; any spike needs Q-2 scope. |
| Which native compaction bindings really execute, how are triggers/detectors exposed, and which role charges and matched dimensions can be enforced? | E1 → D1/D2/E3 | No assumption of model/provider equivalence; resource choices at Q-3/Q-4. |
| What is the smallest supported user-facing Lab runner; what repairs are needed for compare and claim/launch/settle, and how is R0 lifecycle vocabulary reconciled? | E1 → C2/F2/F3 | Engineering options, not code changes in this row. |
| How do immutable attempts and redistributable native closure connect existing live-evidence and bundle contracts without a second authoritative store? | E1/F1 → E3/F3 | Q-5 access/licensing; ADR/schema changes if needed. |
| Which recovery injection point, observable sink, deny/pause and budget cases fit the same apparatus without adding another environment backend? | E1 → D3/C2 | Q-2 bounds operational scope; D3 defines guarantees. |
| Can a parity design and canonical comparison meet the resource envelope; which optional package/consumer should be dropped first if not? | C2 with E1/B2 → D1/D2/F3/G2 | Q-2 selects portfolio; Q-3/Q-4 govern protocol/resources, not inferred authorization. |

## Spec requirements emitted

Candidate requirements only; reconcile overlaps with A/B candidates during H1/O1 rather than duplicate authorities or amend the frozen specification here.

| ID | Candidate requirement / rationale | Findings / outline | Proposed acceptance |
|---|---|---|---|
| REQ-C-001 | Record one primary job and every family disposition, including first-wave slices, exclusions and revisit conditions. | F1/F7; OL-01, OL-03, OL-14 | Review maps all 22 A1 families and all brief candidates to an approved Q-2 disposition. |
| REQ-C-002 | Reuse one task-to-receipt chain across selected scenarios, identifying each producer, independent check and scenario-specific proof. | F2–F6; OL-05–OL-09, OL-16, OL-20 | E1/E3 map concrete boundaries; later one genuine task traverses them with incorrect-output and failure evidence. |
| REQ-C-003 | Treat selected CLI/R0 repairs, immutable attempts and native closure as explicit engineering prerequisites, with semantic regression evidence. | F2/F3; OL-07, OL-15–OL-17, OL-20 | Tests fail on the baseline defects; same-day retries persist separately; clean recipient performs every advertised procedure. |
| REQ-C-004 | Keep the canonical real compaction comparison central; preserve benchmark, parity, match/oracle/replicate rules and freeze confirmation separately from pilot exploration. | F4; OL-09–OL-11, OL-21 | Protocol/admission review rejects fixture efficacy, no-parity component headlines, silent reduced floors and unamended suite/version substitution. |
| REQ-C-005 | Separate research/engineering completion from a favorable treatment effect and from changing a product default. | F4; OL-11, OL-15 | Null/negative/inconclusive report can complete its scoped deliverable; default adoption still follows applicable gates. |
| REQ-C-006 | Scope operational proofs to declared faults, threat assumptions, observable effects and expected refusals, reusing the core apparatus. | F5; OL-12, OL-15 | D3 case matrix includes controls, negative cases, durable/side-effect evidence and separately reported task/invariant outcomes. |
| REQ-C-007 | Provide fixed bug/receipt participation and separately gated package/consumer exercises with actual prerequisites and loss/compatibility limits. | F6; OL-04, OL-13, OL-16, OL-18–OL-19 | A reader can produce a precise report; selected optional package invokes and rejects tampering; external consumer validates scoped output. |
| REQ-C-008 | Preserve unmet useful-task/study objectives in any reduced release and record the operator's rescope, rather than substituting fixture success. | F2/F4/F7; OL-05, OL-08–OL-11, OL-24 | C2/H2 show fixed, contingent and omitted deliverables plus remaining gates and exact Q-2 disposition. |

## Handoff and verification

Accepted A1–A3/B1–B2 resolve C1's inputs. E1 should answer the concrete seam questions above; it need not rerun the broad survey. C2 owns cost/effort ranges, proposed bounded spikes and Q-2 review. D/F/G own protocol, receipt, replication and reader details after their gates. No new row is required; the later catalogue remains proposed scope, not a hidden implementation backlog.

Material tensions are explicit: broad coverage versus one reusable chain; a central live study versus unproven task/grader closure; approachable teaching versus known CLI/R0 defects; and a small first wave versus binding benchmark/parity/replicate requirements. Existing live judge/plugin receipts remain useful positive evidence, while their narrower claims and fresh R0 failure coexist. No accepted-note contradiction requires rewriting history.

Verification scope: zero-open-deferrals query returned exit 0/empty; planning freshness validator returned no violations and zero validator warnings (Perl locale fallback printed). Note authored with Python and its added-file diff reviewed. `check-living-docs.sh` passed (387 files); `check-bm-tools.sh` passed; `check-live-evidence.sh` passed (34 records); `gen-revisit-triggers.sh --check` passed. Scoped checks passed: 10 relative links, seven findings with all eight fields, 22/22 exact family names and eight unique candidate requirements. These validate this documentation and existing records, not new product behavior. **0 Rust tests, 0 runtime examples, 0 live legs, 0 paid calls or spikes executed in C1.** Only this note was written in the repository; no central state, source, build ledger, historical record or gate edit, commit, external write or additional agent.
