# 09_measurement-standard — Common measurement and claim rules

Stream: D — Scientific methodology\
Researched: 2026-10-29 UTC\
Author: /root/d1_measurement\
Ledger rows covered: D1\
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved planning and accepted dependencies.\
Confidence overall: medium; proposals need validation.

**Decision:** candidate common rules for D2/D3/F1/E3; Q-3–Q-6 remain open. Null/negative/inconclusive results satisfy research. Favorable effects are neither engineering acceptance nor default adoption.

| Decision / consumer | Required acceptance evidence | Still conditional |
|---|---|---|
| D2 efficacy | Effective treatment, executable grade, parity, enforced caps, frozen task-aware inference | Q-3/Q-4 selection/resources/margins; separate execution authorization |
| D3 operations | Fault opportunities, independent sink, controls and prefix observations | Bounded case matrix |
| F1/F3 receipts | Immutable attempts/grades/analysis, reconciled denominators, tested procedures | Q-5/Q-6 access/retention/participation |
| E3 integration | Existing owners, correct bounds/cost folds, typed unknowns/refusals | Engineering/ADR gates below |
| Reporting | Classes/vetoes/missingness/activation/cost/tails/uncertainty | Claims bounded by evidence |

## Basis and current contract

Read [brief](../brief.md), [ledger](../research-ledger.md), [conventions](../research/CONVENTIONS.md), [trace](../OUTLINE_TRACE.md), [A2](../research/02_evidence-audit.md), [B2](../research/05_ecosystem-and-benchmarks.md), [C1](06_demo-and-study-portfolio.md), [E1](07_execution-seams.md), [C2](08_feasibility-and-scope.md), root/docs instructions and `synthesize-spec` run mode at skills revision `e981ec57d487a2c41d416672483b5a4df15b1233`. Repository retrieval checkpoint **2026-10-29T19:52:55Z**, baseline above; no reruns.

[Frozen §§10.1–10.5](../../spec/sections/10-evaluation-reproducibility-plan.md) retain paired TB **2.0**, suite A held-out manifest (89-task catalogue does not select it), native `evict_oldest`/`clear_tool_results`, fixed model/roles, `fault_profile=none`, `search_budget=0`, `matched_cap`, `same_snapshot`, `cold_start`, `{tokens.input.total,tokens.output.total,model_calls,time.wall_ms}`, tolerance `0.10`, **R ≥ 5** per task/arm, and class/contamination strata. [ADR-0213, 2026-10-22 amendment](../../research/decisions/ADR-0213.md), OQ-113/342/360/366/367/375, adopts registered margins/parity subset, packaged retries, confirmatory draws ≥2000, global `T_CLT=100`/`T_BCA=30` and admission policy. Old DEFERRED markers are history. Departures require ADRs; Q-3/Q-4 supply data.

## Material findings and proposed standard

### F1 — A claim needs a declared estimand and evidence boundary
Claim: Subjects nest within planned task/arm/replicate slots; tasks are the comparison clusters.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: §10.1; [ADR-0158 D1–D3](../../research/decisions/ADR-0158.md); [EvalRun](../../crates/hh-eval/src/runs.rs), 64–156, identifies factors; C1 F2/F4 and A2 F3–F7 separate claims.\
Retrieved: 2026-10-29T19:52:55Z.\
Implication for the spec: Freeze estimands before outcomes; bind observations to them.\
Outline delta: EXTENDS OL-05, OL-09–OL-11, OL-15–OL-16.\
Limitations: Registration is not execution or external validity.

Key rows by `(protocol,phase,task,split,stratum,arm,replicate,plan,attempt,subject)` and full factors/class/granularity. Call/verifier IDs are children. Retries/replay/regrades/events are not replicates. Report task/slot/first-subject/all-attempt/gradeable/denominator counts.

For metric m, `x_at` is the declared within-task reducer over eligible first subjects; `d_t=x_test,t−x_control,t`; `Δ_m=Σw_t d_t`, equal `w_t=1/T` within each declared stratum. Publish task values/weights, binary percentage-point effects and native cost units/direction. No extra weight for retries, events or detector opportunities. Zero-opportunity mechanism values are unavailable; task success remains eligible. Partial pairs report coverage/bounds, never a full-set estimate.

Target the registered manifest/configuration/envelope. Population generalization needs a sampling frame/selection argument; model/environment/family transfer needs held-out levels. Bootstrapping a convenient subset does not make it representative. Keep named-task demonstration, population efficacy, bounded operational proof and G3 adoption evidence separate.

### F2 — Outcome accounting must preserve failures, vetoes and missing slots
Claim: Lifecycle, task grade, outcome class, veto and availability need separate denominator policies.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: §10.1; [scorecard](../../crates/hh-eval/src/scorecard.rs), 139–170/410–556; [compare](../../crates/hh-eval/src/compare.rs), 317–382; [settle](../../crates/hh-experiment/src/engine.rs), 1617–1675; [overlays](../../crates/hh-results/src/projection.rs), 202–283, implement distinct outcome paths.\
Retrieved: 2026-10-29T19:52:55Z.\
Implication for the spec: Reconcile planned slots, scheduler, projection and analysis populations.\
Outline delta: EXTENDS OL-10–OL-11, OL-15–OL-16, OL-20.\
Limitations: Proposed selection rules need E3 settlement/overlay integration.

| Observation | Capability denominator and reporting rule |
|---|---|
| Executable pass/fail | `scored`; oracle 1/0, not process/lifecycle success. |
| Subject cap/deadline | `budget_exhausted`, in denominator; valid grade retained, absent grade=0 only under `count_as_failure`. |
| Environment/provider fault | `infrastructure_failure` with reason/detector; excluded from default denominator, counted beside/in bounds. |
| Grader crash/timeout/empty/nonfinite/judge refusal | `oracle_failure`, not task zero; preserve submission for declared regrade. |
| Refused/unsupported | Attempted=`refused`; prelaunch=planned/not-run; metric inapplicability typed. Never drop task or invent grade. |
| Cancellation/collection stop | Launched=`cancelled`, otherwise not-run; reason/time distinguishes subject deadline from shutdown. |
| Pass with veto | Preserve pass/veto IDs; exclude from headline, count success-with-veto and disclose denominator exclusion. |
| Missing/corrupt data or detector | Durable status plus typed unknown/unavailable; never failure/clean veto/zero by default. |

Base N=`scored ∪ budget_exhausted`; current headline excludes vetoes. Show base/vetoed passes and N, clean N, per-veto tested/unknown. Add separately named **joint task-pass-without-observed-veto rate** over base N: veto=0; required unobserved checks=unknown. This reveals selection, not absence of tampering. E3 reconciles `compare` (no scorecard veto filter) with headline policy.

**Attempts:** first subject per slot is primary; secondary recovery uses first admissible completed retry, retaining failure/cost, never best-of. Freeze retry classes/backoff: packaged `max_per_plan=2` permits two reattempts, fraction cap .10; Q-4 funds allowance. Oracle failure regrades same submission; first valid verifier attempt wins under frozen rule, failures/costs persist. Freeze watermark; changes supersede analysis. Current successful overlays do not clear earlier oracle-failure class: E3 must resolve. Short cells remain `not_run`; retries never increase independent n.

**Bounds:** per task/arm, M planned first slots = s known successes + f failures + u unresolved. Binary all-planned bounds `[s/M,(s+u)/M]`; contrast `[Σw(L_test−U_control),Σw(U_test−L_control)]`. Known pass-with-veto is s for raw task success, f for joint success; declared cap failures are f; other missing verdicts/slots are u. These bounds differ from eligible-only points and sampling intervals. Continuous missing values use frozen [L,U]; unbounded cost/rate has no finite upper bound. `outcome_bounds` names population/metric: robust sign requires all completions preserve sign; practical robustness uses the margin. Sensitive cells remain reported with affected task/class IDs.

### F3 — Independent grading and parity precede component headlines
Claim: Task claims require executable upstream grading; component headlines additionally require admitted original-runner parity.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: §10.3; [BenchmarkAdapter](../../crates/hh-bench/src/adapter.rs), 1–26/426–481; [parity_report](../../crates/hh-bench/src/parity.rs), 1–43, wraps supplied comparison; E1 F6/F10 and adopted OQ-342 fix execution/parity boundaries.\
Retrieved: 2026-10-29T19:52:55Z.\
Implication for the spec: Gate admission on executed task/oracle/parity evidence.\
Outline delta: CONFIRMS OL-05, OL-09, OL-15; EXTENDS OL-10, OL-16, OL-20–OL-21.\
Limitations: Wrapper/fixture equality does not prove original-runner parity.

Pin task/runner/verifier/images, three task surfaces, submission and validity/contamination records. Execute the unchanged oracle as instrument work; retain invocation, exit, outputs and grade mapping. Correct/wrong/broken-oracle controls must diverge. Protect held-out material and agent/verify network policies. Independence is authority separate from participant claims; allowed shared verification stays shared with tamper `n/a{no_detector}`. Judged/human metrics stay separate/non-headline; invalid tasks/oracle defects are listed and versioned.

Parity uses the same participant/tasks/snapshots/budgets, registered R3-style margins and declared original/replay runs; missing replay is unmatched. Preserve oracle/environment/network semantics and failures. Q-3/Q-4 select design/margins; adaptive estimated parity cannot discharge headlines. Before admission: `product` smoke, `parity=none`. Later validity exclusions apply symmetrically and retain original/revised reports; invalidated estimands require new confirmation.

### F4 — Treatment fidelity and mechanism rates require event-level evidence
Claim: Effective provider-input treatment, activation and detector availability are separate measurements.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: [ADR-0077 D4](../../research/decisions/ADR-0077.md) defines rates; [context_metrics::fold](../../crates/hh-eval/src/context_metrics.rs), 99–220, and [catalogue](../../crates/hh-eval/src/catalogue.rs), 454–475, supply counts/approximations; E1 F3/F4 identifies treatment gaps.\
Retrieved: 2026-10-29T19:52:55Z.\
Implication for the spec: E3 resolves detector/registry semantics; D2 freezes windows/reducers.\
Outline delta: EXTENDS OL-09–OL-11, OL-15–OL-16, OL-20.\
Limitations: Association is not causal forgetting; zero needs detector plus denominator.

Trace policy → invocation → retained IDs/view → assembly → **actual dispatched payload**, linked to call/attempt; check required groups/tool pairs. Hashes need inspectable content. Capture both policies/fallback/ineffective/refusal; retain controlled-access live input/served identity. Instruments never feed back. Untriggered runs remain in efficacy; activated-only analysis is descriptive post-treatment selection.

| Quantity | Required numerator / denominator and unavailable rule |
|---|---|
| Trigger/activation/fidelity | Starts/eligible pressure opportunities; completed/started; changed dispatched requests/completed with a next dispatch. Also triggered runs/all first subjects. Freeze trigger and dispatch joins. |
| Ineffective/fallback/count | Counts/statuses plus ineffective/completed and fallback/started; actual fallback is a visible treatment deviation. |
| `grounding.reacquisition_rate` | ADR-0077 matching deliveries/reads after c / `card(forgotten(c))`; retain `(c,event,item)` joins. Current unique-ID count differs. Repeated delivery can yield >1: intensity, not Bernoulli probability; never clamp to ppm. |
| `reliability.repeated_action_rate` | ADR-0077 matching pre-c idempotency-key effects after c / all effects after c; retain `(c,effect)` joins/keys. Capability/args equality is only the current approximation. |
| Rate window/reducer | Proposal: each completed c through termination, retaining overlapping c/event pairs; sum numerators/denominators within subject, then equal subject/task means. D2 freezes; changed windows/dedup need distinct definition/ADR. Zero denominator=`estimator_undefined`; absent IDs/keys/detector=`no_detector`, with coverage. |
| Recall/labels/summary | Recovered/reachable extracted facts / extractable forgotten facts; no extractor=`no_detector`. Retain pre/post labels, summary-followed detector class (no summary=inapplicable), opacity delta and every required metric. |
| Overhead/latency | Subject compaction resource / same-unit subject total, separate token/call/spend shares. Actual elapsed compaction/run clocks; no event: count 0, conditional latency unavailable. Oracle costs are separate. |

Freeze registry names/units/bounds/direction, events/window/reducer, detector version and outcome policy; golden traces cover positive/negative/missing IDs. Missing primary detectors block that claim but stay in multiplicity. Artificial pressure is diagnostic unless admitted as a distinct factor; never silently alter tasks to force activation.

### F5 — Equal budgets require enforced dimensions and nonduplicated actual charges
Claim: Matching requires actual enforcement; resource consumption and study outlay need distinct, nonduplicated totals.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: §10.2/10.5.1; [Account](../../crates/hh-budget/src/account.rs), 285–407/445–478/581–625/975–1015, propagates then deduplicates charges; [settle](../../crates/hh-experiment/src/engine.rs), 3050–3070, and [Results](../../crates/hh-results/src/projection.rs), 110–116, sum propagated rows; [accounting](../../crates/hh-eval/src/accounting.rs), 1–32; E1 F5.\
Retrieved: 2026-10-29T19:52:55Z.\
Implication for the spec: Enforce four dimensions and reuse scoped owner folds; propagated copies are not extra spend.\
Outline delta: EXTENDS OL-09–OL-10, OL-15–OL-16, OL-20–OL-21.\
Limitations: Nested-budget risk is source-derived, not measured overcharge or live enforcement.

Freeze MeteringFormula/caps/roles/prices/confidence. [§8.2](../../spec/sections/08-cross-cutting-contracts.md), 167–172, makes token totals derived: reserve/charge admitted primary components, never new total counters. [M1](../../spec/sections/05h-measurement-evolution.md), 72, measures created→finished wall time; preterminal grading extends it. A separate instrument budget does not change that clock; any phase exclusion needs explicit records/ADR. Reserve admitted input/output/call bounds before dispatch, reconcile actual response/failure usage and enforce real elapsed deadline; unknown bounds cannot support `enforced`. Retain token categories, seed support, served identities, routing/fallback/cache state. Labels, fixed counters/socket timeouts are insufficient. Charge in-flight/committed work and disclose overruns; never erase usage to meet caps.

Subject includes task/summarizer/repair/validation and failed/retried usage; independent oracle/probes are instrument work with separate caps/network policy. Distinguish call from transport-attempt accounting. Reuse/expose existing `Account::totals` (975–1015), which deduplicates leaf/ancestor charges by source/dimension, with explicit subject scope; do not create another reducer. Preserve its documented reverted-effect treatment while separately retaining actual billed outlays. Reconcile subject+instrument totals; retain cost confidence/unknowns and all phase/role/attempt/environment/storage costs. Equal ceilings ≠ equal use: median deviation >0.10 is reported `imbalanced` with distributions/utilization/frontier; unequal/unenforced caps refuse. Underutilization alone is not efficiency.

### F6 — Pilot learning and confirmatory inference must have different identities
Claim: Separate pilot learning from frozen, randomized confirmation and its inference assumptions.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: §10.1/10.3, ADR-0158 D1/D3/D8/D10, C2 F4; [Winkler et al., NeuroImage 92 (2014), version of record, pp. 383–388](https://wrap.warwick.ac.uk/65670/1/WRAP_1-s2.0-S1053811914000913-main.pdf) requires permutation invariance/sign symmetry and dependence blocks.\
Retrieved: Repository 2026-10-29T19:52:55Z; PDF 2026-10-29T19:40:45Z (PMC blocked; published PDF readable).\
Implication for the spec: Freeze assignment/inference together; Q-3/Q-4 choose resources.\
Outline delta: EXTENDS OL-09–OL-11, OL-16, OL-20–OL-21.\
Limitations: General methodological evidence; no HH pilot/coverage/power execution.

Separate diagnostic/pilot/parity/confirmatory IDs. Dev/search pilots measure activation, variance, class failures, costs and estimator behavior for planning; never select tasks by favorable arm effect or relabel pilots as confirmation. Preserve changes/results. Canonical `search_budget=0` remains: tuning/adaptive selection needs a separate exploratory design/amendment. Split precedes search; held-out manifest freezes before confirmation.

Freeze hypotheses/`capability.task_success`/F4’s two rates, metrics/detectors/denominators, tasks/weights/strata, pins/roles/caps/R, assignment/seeds, retry/regrade/exclusion, methods/draws/levels/families, margins/stops/code before confirmation. Changes fork cohorts. Randomize task/within-block arm order; balance time/host/provider/concurrency/cache, retain schedule. Task pairing always; replicate pairing only with declared honoured seeds. No flattened replicates/events or equal-seed assumption. Declare shared shocks and valid clustering/exchangeability; otherwise refuse IID inference.

### F7 — Existing estimators need a scientific readiness gate
Claim: Inspected estimators need corrections or admitted alternatives before the dependent claims.
Status: VERIFIED\
Evidence kind: source inspection\
Evidence: [stats](../../crates/hh-eval/src/stats.rs), 191–205, 237–369 and 440–500; [scorecard::cell_interval](../../crates/hh-eval/src/scorecard.rs), 174–267; [compare](../../crates/hh-eval/src/compare.rs), 781–895; [analysis multiplicity](../../crates/hh-analysis/src/multiplicity.rs), 20–87; ADR-0158 and [ADR-0277 D6](../adr/ADR-0277-s3.3-eval-kernel-and-bench-adapter-rulings.md).\
Retrieved: 2026-10-29T19:52:55Z.\
Implication for the spec: E3 fixes/admissions use existing owners, never runner-side substitute statistics.\
Outline delta: EXTENDS OL-10–OL-11, OL-15–OL-16, OL-20–OL-21.\
Limitations: Bounded source audit; numerical validation below is future work.

| Inspected behavior | Required implementation/acceptance before dependent inference |
|---|---|
| Wilson/Beta pool `(c,n)` while point averages task means; Beta function is one posterior | Not paired/clustered Beta-Binomial. Validate target/coverage with heterogeneous tasks and unequal n. |
| `compare` always bootstraps without floors; bootstrap fixes 4000 draws and accepts one task | Enforce <2-task refusal, floors and actual declared seed/draws across analysis paths; one task permits raw observations only. |
| Bounds use `point ± pooled excluded fraction`, also for cost units | Implement F2. Symbolic counterexample, not execution: each of 2 tasks, A=5 passes+45 unknown; B=25 passes+25 failures. Current Δ=.5±.45 wrongly sign-robust; planned bounds [−.4,.5]. |
| Tail effects are absolute deltas, without intervals | Add signed/arm tails, denominator/censoring and uncertainty; max is observed, not a population bound. |
| Multiplicity covers only the current call | Bind frozen family across reports/strata; unavailable members remain in correction accounting with unavailable results. |

**D2 method contract:** use F1 Δ. Below `T_CLT=100`, canonical binary inference requires paired/clustered Beta-Binomial with declared `Beta(1,1)` and `credible{prior}`; Wilson companion only for a valid binomial target, not pooled repeats. D2 specifies likelihood, task/pair effects, prior placement, fitting/draw algorithm and target; E3 validates. Independent per-task Beta posteriors are not automatically hierarchical population inference. Replacement needs explicit ADR/admission. Until resolved, confirmatory binary uncertainty is unavailable, not improvised pooled/bootstrap output. ≥100-task clustered CLT still needs assumptions/coverage; do not enlarge TB2 merely to cross its floor.

Mechanism rates use task reducers, not event-level binomial n. Continuous/heavy-tail Δ uses task-paired bootstrap: BCa ≥30 tasks, percentile below; retain whole task clusters and declared nesting. Task sign flips require the registered exchangeability/randomization argument: exact enumeration ≤20 tasks, otherwise seeded Monte Carlo with +1 correction. A weak-null mean test is not automatically exact sharp-null inference. Declare ≥2000 confirmatory draws and actual seed/method. Degeneracy, missing keys or inadmissible pairs trigger refusal/`estimator_undefined`/`not_run`, not improvised substitution. Record target, version, assumptions, floors/fallback, interval type/level, priors and inputs.

Retain `(c,n)`, pass^k=`C(c,k)/C(n,k)`, pass@k=`1−C(n−c,k)/C(n,k)` for k≤n; otherwise unavailable. Use first subjects, not retries. Show effects/intervals/sign counts; P95/P99/max cost/wall/severity; P50/P95/max signed and absolute effects. Nearest-rank quantiles retain n/censor fraction; small-n P99 may equal max. Cluster-bootstrap tail summaries, label degeneracy; observed max is not a population bound. Missing terminal usage/time is right-censored: lower bound plus flag, never pretend cap-valued completion. Critical-run Wilson remains a descriptive companion with clustering caveat, not zero-risk proof. Holm all frozen primaries, BH exploratory families; distinguish interval coverage from multiplicity-adjusted testing.

### F8 — Decision thresholds must respect uncertainty and bounded scope
Claim: Engineering acceptance, scientific conclusions, stopping and adoption require separate decisions.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: Brief §1/7; frozen §10.2 equivalence/C4 gates; [benefits::equivalence_run_full](../../crates/hh-eval/src/benefits.rs) and its `NoRegisteredMargins`/`MarginNotPreRegistered` refusals; C1 F4/F5; C2 F2/F4.\
Retrieved: 2026-10-29T19:52:55Z.\
Implication for the spec: Separate verdicts; Q-4 supplies margins before confirmation.\
Outline delta: CONFIRMS OL-11–OL-12; EXTENDS OL-10, OL-15–OL-16, OL-19–OL-21.\
Limitations: No margins, model/task choice, power or budget set; default adoption remains separate.

Freeze gain δ, harm h, precision/levels; reverse cost direction. Harm requires interval support below −h. Practical benefit requires interval and multiplicity support beyond δ plus veto/outcome evidence; positive point/p alone is insufficient. Equivalence uses registered ±ε and TOST: inside=equivalent, outside band=not_equivalent, overlap=inconclusive. Nonsignificance is not equivalence. Two one-sided α tests correspond to a matching 1−2α confidence interval, distinct from ordinary reporting; credible intervals are not automatically TOST. Missing margins refuse. D2/E3 resolve small-n equivalence method before parity claims; no post-hoc margins.

Prefer fixed planned tasks/R within hard ceilings. Broken isolation/accounting/treatment/integrity stops collection with reason/partial cohort/costs; ceiling exhaustion means incomplete coverage, not a smaller retrospective manifest. No favorable-sign stopping/repeated fixed-sample tests. Pilot simulations assess precision/power across task heterogeneity, missingness and plausible effects; ≥5 is not power. Sequential/futility/precision stopping needs a pre-registered valid alpha-spending or confidence-sequence method/schedule and implementation; otherwise refuse. Futility is not equivalence; uncertainty is a valid result.

D3 counts attempted/reached injections, observable sink/prefix checks, refusals and unavailable cases separately. Zero violations means `0/N tested opportunities under named assumptions`; unreachable points are untested. Kp4/control, deny, actual pause/respond/resume and exhaustion need ledger plus independent sink, retaining duplicate/missing/unknown effects. Fixtures prove bounded behavior, not general containment/exact-once or live utility; hashes/replay cannot replace observation.

## Answer and completion assessment

D1 answers common measurement/claim rules. Binary inference, bounds, veto consistency, resampling, detectors and cost projection need engineering/admission. No fixes or reopened historical debt are claimed; D2/Q-3–Q-6 remain open.

## Open questions

| Question | Owner / consumers | Boundary |
|---|---|---|
| TB2/model/control/oracle/parity/strata? | D2/E3 → F1/F3 | Q-3 |
| Binary/TOST estimator, rates/windows/registry/family? | D2/E3 → F1 | Validation/ADR before freeze |
| Pilot/n/power/margins/retry/verify/resource caps? | D2/operator → E3 | Q-4; execution separate |
| Attempt/grade/input closure, access, replication? | F1/F3/E3/operator | Q-5/Q-6; no publication/outreach |
| Sink/fault opportunities, detector reachability? | D3/E3 | Approved scope; expansion to operator |

## Spec requirements emitted

Candidates only; H1/O1 reconcile overlap with E1/C2 and retain existing identities.

| ID | Candidate / rationale | Findings / outline | Proposed acceptance |
|---|---|---|---|
| REQ-D-001 | Bind task/slot estimand and claim population. | F1; OL-09–OL-11, OL-15–OL-16 | Recompute weights; retries never raise n. |
| REQ-D-002 | Separate classes/vetoes/denominators and first attempts. | F2; OL-10–OL-11, OL-15 | Mixed/short/veto fixtures reconcile all reports. |
| REQ-D-003 | Planned-task bounds retain missing/censored outcomes. | F2/F7; OL-10–OL-11, OL-15 | Counterexample [−.4,.5]; no invented finite bound. |
| REQ-D-004 | Executable oracle/parity admission. | F3; OL-05, OL-09, OL-15–OL-16 | Correct/wrong/crash controls; no-parity headline refuses. |
| REQ-D-005 | Policy-to-payload and versioned detector/rates. | F4; OL-09–OL-10, OL-15–OL-16 | Policy/fallback/missing-ID golden traces. |
| REQ-D-006 | Enforce dimensions; reuse scoped cost authority. | F5; OL-10, OL-15–OL-16, OL-20 | Nested/retry totals; unenforced dimension refuses. |
| REQ-D-007 | Separate phases; freeze assignment/analysis/stops. | F6/F8; OL-09–OL-11, OL-20–OL-21 | Hash predates subjects; changes fork cohorts. |
| REQ-D-008 | Admit task-aware estimators/floors/families/tails. | F7; OL-10–OL-11, OL-16, OL-20 | Known answers/heterogeneous coverage; undefined refusals. |
| REQ-D-009 | Separate effect/equivalence/operations/adoption. | F8; OL-11–OL-12, OL-15, OL-19 | Null accepted; missing margin/opportunity not pass. |
| REQ-D-010 | Bound receipt/replay claims to tested procedures. | F1/F3/F8; OL-07, OL-15–OL-16 | F1/F3 independently check each promise. |

## Handoff and verification

D2 owns protocol; D3 opportunities; E3 existing-owner fixes/ADRs; F1/F3 receipts. Portfolio unchanged. **Hashes** establish commitment consistency, not truth/completeness/authenticity without trust. **Recomputation** checks supplied-row arithmetic, not collection/oracles. **Replay/R1** checks recorded derivations/decisions, not fresh solving or zero cost. **R0**, **R2 re-execution**, **R3 equivalence** need separate §10.4 procedures/pins/budgets/verdicts. Sanitization supersedes with losses, not byte identity. None proves adoption benefit.

Checks passed: deferrals-first empty; planning validator 0 violations/0 warnings (locale fallback); scoped checks 34 relative links, 8 complete findings, 10 unique IDs, four closing sections/table widths; added-file diff reviewed, no whitespace diagnostics (no-index exit 1 denotes added content). **0 Rust tests/builds/runtime/provider/task/verifier executions.** Only this note written; no commits/external mutations. Coordinator owns final docs checks/state.
