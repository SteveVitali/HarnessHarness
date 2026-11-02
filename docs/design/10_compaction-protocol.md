# 10_compaction-protocol — A bounded native compaction study

Stream: D — Methodology\
Researched: 2026-10-29 UTC\
Author: /root/d2_compaction\
Ledger rows covered: D2\
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved planning and accepted dependencies.\
Confidence overall: medium; unexecuted design.

**Recommendation:** engineering/receipts first; the central comparison remains conditional. Propose one provider/fixed role set, six-task pilot (60 subjects) and 30-task confirmation (300), two arms × R5. R5 conservative parity cannot establish ±10pp even with perfect agreement: Q-3/Q-4 funds adequate parity, admits a reviewed alternative, or limits first-wave claims to product smoke/mechanisms. No confirmation spend before admission.

## Basis

Sources read: [brief](../brief.md), [ledger](../research-ledger.md), [conventions](../research/CONVENTIONS.md), [trace](../OUTLINE_TRACE.md), [C1](06_demo-and-study-portfolio.md), [C2](08_feasibility-and-scope.md), [D1](09_measurement-standard.md), [E1](07_execution-seams.md), [E2](12_feasibility-findings.md), root/docs instructions and `synthesize-spec` run mode at skills revision `e981ec57d487a2c41d416672483b5a4df15b1233`. Inspection: **2026-10-29T20:39:27Z**. Preserve [frozen §§10.1–10.5](../../spec/sections/10-evaluation-reproducibility-plan.md); no implementation/amendment claimed.

## Protocol findings

### F1 — Contrast and target
Claim: Compare native policies on a fixed manifest under canonical caps.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: Frozen §10.5.1; [ADR-0077 D2/D4/D8](../../research/decisions/ADR-0077.md); D1 F1.\
Retrieved: 2026-10-29T20:39:27Z.\
Implication for the spec: Freeze contrast and target.\
Outline delta: CONFIRMS OL-09–OL-11; EXTENDS OL-21.\
Limitations: No selected tasks/models or population claim.

A=`evict_oldest`; B=`clear_tool_results`. Freeze IDs/parameters, common compaction-capable control/builder/trigger, permissions, required-item protection and fallback. No task-conditioned rules/padding. Retain fixed model roles including unused summarizer profile, `fault_profile=none`, `search_budget=0`, `same_snapshot`, `cold_start`, `matched_cap`, tolerance `0.10`, and `{tokens.input.total,tokens.output.total,model_calls,time.wall_ms}`. Equal caps need not yield equal consumption.

H0: each primary Δ=0; H2: mechanism rates differ despite negligible task-success effect. For each metric `d_t=mean_B,t−mean_A,t`, `Δ=Σw_t d_t`, with `w_t=1/T` within each contamination stratum. Task success uses percentage points; lower rates/cost are better. Target expected first-attempt performance on fixed tasks. Tasks are clusters/pairs; repeats estimate task/arm rates. No population/transfer/C4 claims.

### F2 — Selection and phases
Claim: Selection and phase separation prevent pilot-based cherry-picking.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: Frozen §§10.1/10.3; D1 F3/F6; C2 F4; E1 F10 candidate task.\
Retrieved: 2026-10-29T20:39:27Z.\
Implication for the spec: Freeze inclusion and phase identities.\
Outline delta: EXTENDS OL-09–OL-11, OL-15–OL-16, OL-21.\
Limitations: Pins, rights and eligibility require Q-3/Q-5.

Import TB **2.0** and its 89-task catalogue identity/unchanged oracle. Outcome-blind eligibility: immutable task/image/verifier, executable original runner, allowed resources/isolation/network, protected task surfaces and validity/rights records. List exclusions before outcomes; retain hard/untriggered tasks. `cancel-async-tasks` is only a candidate.

Assign dev/held-out before pilot access. Seeded sampling without replacement selects 30 eligible held-out tasks under frozen contamination/metadata quotas/weights; no arm-effect/activation selection. Too few eligible tasks means pre-confirmation revision or stop. Separate diagnostic/activation/pilot/parity/confirmation/replication IDs. Pilot cannot tune policies under zero search or become confirmation. Tuning needs separate exploratory scope. Post-freeze defects retain original rows/dated revisions; material oracle/environment changes require a new cohort.

### F3 — Assignment and denominators
Claim: Randomization and first slots preserve failures/drift.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: Frozen §10.1; [ADR-0158 D1/D3](../../research/decisions/ADR-0158.md); [ADR-0213 adopted OQ-360/366/367](../../research/decisions/ADR-0213.md); D1 F2/F6.\
Retrieved: 2026-10-29T20:39:27Z.\
Implication for the spec: Freeze assignment, retries and denominators.\
Outline delta: EXTENDS OL-10–OL-11, OL-15–OL-16.\
Limitations: Random order does not remove shared shocks.

Bind each protocol/phase/task/stratum/arm/replicate slot to first subject, attempts, grades and costs. Randomize tasks; a fair task-level coin assigns arms to identical lanes interleaved over five rounds. Balance host/calendar; ≤2 concurrent subjects, one/arm. Commit seed/schedule and retain actual order. Reset environment/session; provider cache bypass/enforced cold namespace required, unknown warmth fails matching.

Distinct seeds by default, requested/honoured status retained. Pair tasks only; replicate numbers do not create common-random-number or 2×2 pairing. Drift stops collection; shared shocks invalidate independence despite balance.

First subjects are primary. Packaged retries: ≤2/plan, ≤10% of plans, frozen infrastructure classes/backoff, no wrong-answer retries. Regrade sealed submission under first-valid-grade rule. Neither raises n; no paid retry without quoted allowance.

Preserve all outcome classes and planned/not-run. Capability base=scored+budget-exhausted; missing cap grade=0 only under `count_as_failure`, other missing grades unknown. Preserve raw grades/vetoes/unknown checks; canonical headline excludes success-with-veto, counted beside it. Also show D1's joint pass-without-observed-veto endpoint. Absent checks are not clean.

Confirmation requires registered first-slot coverage/eligible counts in both arms. Short/unequal cells retain descriptive values plus unavailable inference/`InsufficientReplicates`. With M planned, s successes, u unknown: `[s/M,(s+u)/M]`; contrast `[Σw(L_B−U_A),Σw(U_B−L_A)]`. Include unrun tasks and raw/joint veto distinctions; no prior imputation.

### F4 — Launch gates
Claim: Treatment, caps, independent grading and parity gate component claims.
Status: PARTIALLY VERIFIED\
Evidence kind: source inspection and design proposal\
Evidence: E1 F3–F7/E2 F2–F5 identify missing composition; frozen §§10.2–10.3 impose gates.\
Retrieved: 2026-10-29T20:39:27Z.\
Implication for the spec: E3 resolves gates before collection.\
Outline delta: EXTENDS OL-09–OL-10, OL-15–OL-16, OL-20–OL-21.\
Limitations: No executed end-to-end proof.

Prove policy→retained IDs→assembled view→actual payload, legal tool/result groups and retry/restart behavior. Keep untriggered/ineffective/impossible/fallback outcomes; fallback is an assigned-arm deviation. Instruments never feed subjects.

Subject Account reserves canonical input including cache/provider overhead, visible+reasoning output bounds and every call attempt; reconcile actual/ambiguous usage. Enforce real deadlines without reset. Derived totals are not primary charges; unknown/reported-only bounds refuse. Reuse deduplicated accounting; separately retain physical billing/reverted-effect semantics. Subject validation/repair/retries count; grading/detectors have instrument caps.

**Clock gate:** M1=created→finished, including preterminal grading. E3 needs explicit ADR for participant-stop/grade-pending settlement/separate subject time while retaining M1, or enforces the entire current interval. No subtraction/backdating.

Execute unchanged pinned upstream oracle on sealed submission; retain invocation/exit/output/grade, correct/wrong/broken controls and authorized isolation/shared tamper `n/a{no_detector}`. Parity compares **same participant via HH versus original runner**, matched tasks/model/budgets, registered margins, original runs/replay verdicts. Oracle agreement is insufficient; no parity means product smoke only.

### F5 — Mechanism definitions
Claim: Canonical rates require explicit event joins, windows and denominators.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: ADR-0077 D4; frozen §10.5.1; D1 F4/E2 F5 distinguish counts from rates.\
Retrieved: 2026-10-29T20:39:27Z.\
Implication for the spec: Register definitions before collection.\
Outline delta: EXTENDS OL-09–OL-11, OL-15–OL-16.\
Limitations: Association does not prove causal forgetting.

Completed c uses `(completion_seq(c), subject_execution_stop_seq]`, including subsequent compactions; retain overlapping `(c,event)` joins. `forgotten(c)` is committed omitted IDs/content addresses. Freeze detector/extractor versions.

| Metric | Numerator / denominator |
|---|---|
| Reacquisition | Matching post-c deliveries/fs/memory reads / forgotten items; repeats count, intensity may exceed 1. |
| Repeated action | Post-c effects matching pre-c idempotency keys / all post-c effects; argument similarity is a separately named approximation. |
| Activation/fidelity | Starts/pressure opportunities; completions/starts; changed requests/completions with next dispatch; triggered subjects/all. Retain intended/served policy. |
| Recall/labels/summary | Recovered/reachable facts / extractable forgotten facts; preserve labels/summary applicability/opacity. |
| Cost/latency/counts | Compaction/subject resource in same unit; actual durations; total/ineffective/fallback counts. |

Missing required IDs/keys/extractor=`no_detector`; zero denominator=undefined. No compaction: count 0, latency/rates unavailable. Sum numerator/denominator within subject, divide, average available subject ratios within task, then equal task contrasts. Publish coverage; never weight by opportunities or pool events as trials. Untriggered runs stay in task success; activated-only analysis is exploratory. Three primaries remain registered; unavailable detectors block their claims and stay in multiplicity.

### F6 — Finite-task binary analysis
Claim: Task-stratified Beta-Binomial inference answers a finite-task question.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: ADR-0158 D1–D3; [ADR-0277 D6](../adr/ADR-0277-s3.3-eval-kernel-and-bench-adapter-rulings.md). [Agresti/Hitchcock author-hosted survey §§2.1/4.1/4.4](https://people.stat.sc.edu/Hitchcock/bayesfinal.pdf) describes Beta conjugacy and independent versus paired likelihoods; manuscript publication date unstated.\
Retrieved: 2026-10-29T20:39:27Z.\
Implication for the spec: Explicitly admit the finite-task method.\
Outline delta: EXTENDS OL-10–OL-11, OL-16, OL-20–OL-21.\
Limitations: Conditional independent Bernoulli runs; no population model.

For each fixed task/arm use `c_at|p_at ~ Binomial(n_at,p_at)`, independent `p_at~Beta(1,1)`, posterior `Beta(c_at+1,n_at−c_at+1)`. Pair each task's rates: `Δ^(b)=Σw_t(p_Bt^(b)−p_At^(b))`. This is task-stratified Beta-Binomial inference, not one pooled Beta; pairing is stratification, not correlated replicate modeling. No task-population variance/partial pooling is claimed.

Use integer-Beta inverse-CDF bisection, canonical-input-seeded xorshift64*, 8,000 posterior draws, 95% equal-tail credible interval and analytic mean. Separately label observed `Σw(c_B/n_B−c_A/n_A)`. Wilson companions: per task/arm only. Freeze numerical-error doubling to 64,000 draws, record error, never select seeds. Missing keys, one task or unsupported dependence refuse. Predeclared Beta(2,2) sensitivity is exploratory; it reuses integer bisection, never replaces the registered prior.

**ADR proposal:** admit this finite-task estimator/F7 TOST with existing-owner metadata, not D1's hierarchical population fallback. ADR-0277 bisection remains a primitive; pooled Beta/current bootstrap are insufficient. No new fitter/dependency. E3 validates; H1 routes admission; confirmation waits.

### F7 — Equivalence and feasibility
Claim: Equivalence needs its own frequentist procedure.
Status: UNVERIFIED\
Evidence kind: design proposal; analytical check\
Evidence: ADR-0158 D7/ADR-0213 OQ-113/342; [R stats 4.6.0 binom.test](https://www.stat.ethz.ch/R-manual/R-devel/library/stats/html/binom.test.html) documents conservative Clopper–Pearson coverage.\
Retrieved: 2026-10-29T20:39:27Z.\
Implication for the spec: Admit construction; test feasibility before spending.\
Outline delta: EXTENDS OL-10–OL-11, OL-16, OL-21.\
Limitations: Binomial marginal assumptions; potentially severe conservatism.

For T tasks, α=.05, `η=α/(2T)`, cell bounds are `L=BetaQuantile(η;c,n−c+1)` (0 if c=0), `U=BetaQuantile(1−η;c+1,n−c)` (1 if c=n). Contrast bounds: `LΔ=Σw(L_B−U_A)`, `UΔ=Σw(U_B−L_A)`. Bonferroni over 2T bounds gives one-sided coverage ≥1−α, joint ≥1−2α; it permits dependent estimators but requires valid binomial marginals.

TOST rejects `Δ≤−ε` if `LΔ>−ε` and `Δ≥ε` if `UΔ<ε`; both required. Interval inside band=equivalent, disjoint=not-equivalent, overlap=inconclusive. Missing margins refuse. Separate 95% reporting/90% equivalence intervals. All registered parity dimensions must pass; binary bounds do not cover continuous cost/rates, which require admitted paired methods/margins.

**Resource blocker:** identical all-pass R5 cells give `±[1−η^(1/5)]`: T=2 ±.584; T=6 ±.666; T=30 ±.758. Even this extreme example requires R≥42/53/68 respectively for ±.10. Arithmetic, not power/sample advice. Never fund R5 parity expecting this gate to pass. Inspect existing parity first; otherwise quote adequate parity, admit an efficient alternative by ADR, or stop at smoke. No post-hoc margins/oracle-only substitute.

### F8 — Simulation and resources
Claim: Pilot and simulation support a bounded resource decision, not guaranteed power.
Status: UNVERIFIED\
Evidence kind: design proposal\
Evidence: ADR-0158 D4/D8/D10; frozen §10.1; D1 F7/F8 and C2 F4/F6.\
Retrieved: 2026-10-29T20:39:27Z.\
Implication for the spec: Freeze methods, stops and quote before confirmation.\
Outline delta: EXTENDS OL-09–OL-11, OL-15–OL-16, OL-20.\
Limitations: Activation, rates, costs and variance unmeasured.

Review defaults: useful gain δ=.05, harm h=.05, parity ε=.10, desired 95% half-width .10. Q-4 approves these/unit-aware rate margins; tolerances remain provisional. Pilot estimates activation/grades/failures, task/repeat variance, tails and costs.

Simulate exact pipeline at T={10,20,30}, R={5,10}; null/±.05/±.10, easy/hard mixtures, heterogeneous effects, zero activation, informative missingness/vetoes/shared shocks. ≥2,000 datasets/scenario; retain parameters/seeds/failures. Report coverage/prior sensitivity, frequentist type-I error, widths/verdict probabilities, availability and Monte Carlo error; propagate pilot uncertainty. ADR paired-binary formula is a cross-check. Unsupported precision means stop/requote or explicitly approved limited study. **R≥5 is not power.**

Holm α=.05 covers three primaries/all confirmatory strata, including unavailable members. Task sign-flips require lane randomization/exchangeability: exact ≤20 tasks, otherwise 8,000 draws, `(extreme+1)/(draws+1)`. This tests sharp no-effect, not weak-mean/practical-margin nulls. BH only frozen exploratory families. Whole-task paired bootstrap for rate/cost uses BCa ≥30 tasks, percentile below, declared seed/draws/degeneracy. Its task-mixture sensitivity differs from fixed-manifest binary uncertainty; do not combine into one efficacy verdict.

Retain (c,n), pass^k/pass@k for k≤n, sign counts, P95/P99/max cost/wall/severity and signed tails/intervals; disclose censoring/max limits, outcome bounds and vetoes. Benefit/harm states interval target/type, threshold, multiplicity and sensitivity; point/p alone insufficient. Null/negative/inconclusive research completes; adoption separate.

| Proposed stage | Hard envelope |
|---|---|
| Offline validation | No provider/cloud/tasks/containers; ≤8 h, 4 cores/8 GiB, 32 core-h, 2 GiB. Implementation authorization needed. |
| Activation | New one-task quote; Q-2 approved offline E2 only, no task/provider/container setup. Unset approval/bounds=zero calls. |
| Pilot | ≤60 first subjects; each ≤100 calls, 1M input/100k output total tokens, 30 min admitted wall, verifier ≤15 min. ≤2 concurrent subjects, ≤4 cores/8 GiB each; tranche ≤200 core-h/50 GiB. |
| Parity | No automatic tranche; demonstrate method/margin feasibility, then quote original-runner design. |
| Confirmation | After parity/method/cap/detector admission only: ≤300 subjects, same individual caps, ≤900 core-h/200 GiB/14 days. Larger T/R needs new quote. |

Pin tariffs/providers/roles; quote worst-case token/call, environment/verifier/storage, failures/retries/cancellation exposure. Operator supplies numeric currency ceiling/local-cloud split; absent=$0/no calls. Reserve maximum known exposure before dispatch; insufficient/unbounded billing refuses. These are ceilings, not usage predictions; no automatic cloud fallback/pulls/replication.

Fixed N/R, one final analysis. Integrity/isolation/leak, treatment/cap failure, drift or any ceiling stops retain partial rows/costs. No favorable stopping, retrospective shrink or sequential/futility/equivalence peeking. Checkpoint supplies quote/admission matrix and stop recommendation where parity/precision is infeasible.

## Answer and completion assessment

Design complete; E3 treatment/account/clock, detectors, pins, methods and parity remain gates. Conservative TOST is a material resource obstacle; 300 subjects do not guarantee component evidence.

## Open questions

- **Q-3 — operator/E3 → F1/F3/H2:** choose TB2 pins/splits/strata, original runner/oracle/isolation, provider/roles/cache and parity route; accept finite-task claim scope.
- **Q-4 — operator → E3/F1/F3/H2:** approve margins, sample options and complete quote; fund adequate parity, review another admitted method, or limit claims. Execution approval separate.
- **Method/clock — E3/H1 → O1:** route explicit ADRs and validation before admission.
- **Q-5 — operator/F1/F3:** decide rights/access/retention/sanitization.
- **Q-6 — operator/G3/F3:** decide funded fresh replication/publication; no outreach implied.

## Spec requirements emitted

Candidates for H1/O1; existing identities unchanged.

| ID | Candidate | Source / outline | Acceptance |
|---|---|---|---|
| REQ-D-101 | Freeze native contrast/finite-task target. | F1; OL-09–11/21 | Manifest/weights/caps reconcile. |
| REQ-D-102 | Outcome-blind splits and phase separation. | F2; OL-09–11/15 | Selection replay retains negative/untriggered tasks. |
| REQ-D-103 | Randomized lanes/first slots/drift/retries. | F3; OL-10/15–16 | Schedule/duplicate/retry audit. |
| REQ-D-104 | Class/veto denominators and planned bounds. | F3; OL-10–11/15 | D1 counterexample plus unrun tasks. |
| REQ-D-105 | Gate treatment/caps/clock/oracle/parity. | F4; OL-09/15/20–21 | Wire/cap/grade/clock tests; absent parity refuses. |
| REQ-D-106 | Register rate windows/joins/coverage. | F5; OL-09–10/15 | Overlap/missing-ID/no-event traces. |
| REQ-D-107 | Admit finite-task Beta estimator/target. | F6; OL-10/16/20–21 | Known answers/sensitivity/numerical error. |
| REQ-D-108 | Separate TOST and parity feasibility. | F7; OL-10–11/21 | Exact-bound fixtures; R5 width check. |
| REQ-D-109 | Freeze simulations/families/stops/reporting. | F8; OL-10–11/16 | Null/heterogeneous/missingness cases. |
| REQ-D-110 | Require staged quotes/resource enforcement. | F8; OL-09/20 | Unset budget refuses; preserve partial costs. |

## Handoff

E3 owns integration/ADR reconciliation; F1 immutable receipts; F3 recomputation/fresh replication. H2 retains the parity blocker. No production/spec/ticket edits, commits, live runs or installs; coordinator owns state.
