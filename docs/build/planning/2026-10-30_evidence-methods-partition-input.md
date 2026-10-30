# Read-only methods/study decomposition memo

Baseline verified on disk: `001490c14a6568972d16a663bb04c07a4255d30d`; supplemental-spec SHA-256 verified `8f540171dd38711ad563459d1721277b84100b66a207379668f0efe6db8b208f`. Deferrals-first reader returned zero open rows. Read root/docs/crates AGENTS, decompose-spec at `/Users/stevenvitali/agent-skills/skills/decompose-spec/SKILL.md`, frozen decomposition prompt, supplement §§0/3/4/7/9 and all original/registry D rows, D1/D2/D3/D4, H2, relevant F1/F2/F3 sections, ADR-0158 and governing §10 sections, actual owning code and acceptance tests. No repository edits, branches, installs, compilation/tests, runtime, benchmark, provider or container execution. The sources are read-only evidence, not newly verified behavior. Every anchor below must be re-confirmed at implementation time.

## Main partition judgment

Keep these decisions separate: (a) planned-slot population/selection and missingness, (b) finite-manifest Bayesian efficacy, (c) whole-task task-mixture sensitivity and sharp-null families, (d) mechanism joins/reducers, (e) directional-KL equivalence and confidence labels, (f) authentic participant/runner/policy admission, (g) finite protocol scheduling/freeze and actual research stages. They share one lower-layer input and metadata contract, owned by the first analysis-population ticket; later tickets must consume it rather than invent parallel observations or report stores.

The early admitted Bayesian path is a necessary P8 slice for EX-02. The inspected exact path is not already an admitted equivalent: compare always bootstraps with fixed 4,000 draws, while small-task binary catalogue metadata describes something else. Do not defer all P8 until after P4; do not require live parity to teach the offline lesson.

The actual controlled study remains an explicit chain of independently gated activation/pilot, authentic parity and confirmation/reporting. Code, fixture, numerical and documentation acceptance never discharge its R layer. Valid negative/null/inconclusive results may discharge R; absent admission or collection cannot. Transfer to original-runner treatment effects is excluded unless separately admitted; the ratified first-wave target is HH-native finite-manifest efficacy.

## Verified seams and reasons for the cuts

1. `crates/hh-eval/src/compare.rs:155::CompareInput`, `:195::TaskEffect`, `:317::in_denominator`, `:327::run_value`, `:355::run_counts`, `:399::compare`: records-in computation consumes runs/tasks/design but has no complete planned-slot universe. At :781 it always calls paired bootstrap; :798 computes pooled excluded-fraction bounds; :883 emits empty floors. **Legacy effect direction is A−B** (:201 documentation, :759 code), whereas the new efficacy target is B−A. The new method's explicit direction must preserve legacy reports rather than silently reversing them.
2. `crates/hh-analysis/src/kernel.rs:65::AnalysisInput`, `:163::project_runs`, `:409::compare_input`, `:444::analyze`, `:1583::apply_multiplicity` call: only supplied ResultsRows are projected. Open rows become `not_run`, but wholly unlaunched planned slots cannot appear without an authoritative ExperimentView/plan input. `compare_input` omits AnalysisSpec estimator/resample declarations. Reuse P1 planned/grade disposition semantics and P3 all-planned reconciliation.
3. `crates/hh-analysis/src/project.rs:121::eval_run`: task keys are checked, but ~:224 defaults unknown/missing cache to cold-start and ~:240 derives `seed_honoured` from presence of seed/seed_material. These are not authentic parity evidence; add strict study admission rather than treating legacy projections as proof.
4. `crates/hh-experiment/src/view.rs:78::PlanState`, `:190::ExperimentView`, `:236::fold`, `:245::apply`: durable RUN_PLANNED and launched `attempts` are the existing planned-slot/first-subject authority; :265 populates every run plan. No new authoritative analysis schedule/store is warranted.
5. `crates/hh-eval/src/scorecard.rs:130::run_numeric`, `:167::cell_interval`, `:413::render_cell`: separate current denominator path; :189/:194 pool Wilson/Beta counts. Compare and scorecard must consume the same new declared population and retain old-mode compatibility. Tests `crates/hh-eval/tests/acceptance.rs:658::outcome_classes_counted_beside_headline`, `:708::vetoed_success_excluded_from_headline` currently cover narrower behavior.
6. `crates/hh-eval/src/stats.rs:206::XorShift64`, `:243::bootstrap`, `:267::bootstrap_paired`, `:339::bayesian_beta`, `:371::regularized_beta`, `:454::permutation_signflip_p`, `:503::benjamini_hochberg`, `:524::holm`: existing math/RNG owner. `BOOTSTRAP_RESAMPLES`=:4000; bootstrap accepts one task; Beta is one posterior, not equal-task posterior contrast. Exact sign-flip ≤20 and +1 Monte Carlo already exist, but admission/routing and actual draw declarations remain needed.
7. `crates/hh-lab/src/analysis.rs:195::AnalysisSpec`, `:66::ResamplePlan`, `:757::PairedEffect`, `:802::TestRecord`, `:821::TailEffects`, `:832::OutcomeBounds`, `:874::Multiplicity`, `:942::ComparisonReport`, `:1299::ParityReport`: additive metadata/report contract homes. `crates/hh-ontology/src/eval.rs:584::IntervalMethod`, `:651::EstimatorSelection`, `:1833::PreRegistration`, `:1872::Design` own vocabulary; unknown methods currently refuse. Public schema changes must originate in hh-embed-schema/codegen. No new codec, state authority, dependency or general statistics platform.
8. `crates/hh-analysis/src/multiplicity.rs:23::apply_multiplicity`: primary family is only current comparisons intersected with `primary_metrics`; it cannot retain a whole frozen family split among reports/strata. It unconditionally computes sharp-null sign flips for comparison tables. A method-specific path must not label Bayesian inference or KL parity as this test by default.
9. `crates/hh-eval/src/benefits.rs:366::registered_margins`, `:427::equivalence_run_full`: relative kind is parsed but raw interval is compared to ±margin_ppm at :457; inclusive boundaries pass at :458; the same interval drives negative and positive labels. New KL route must own range normalization, strict outward bounds, separate .90 TOST/.95 reporting and S11 negative allocation in one ticket.
10. `crates/hh-bench/src/parity.rs:19::parity_report` checks only ArtifactBenefit; `crates/hh-bench/tests/s3_12b.rs:556::adapter_parity_legs_over_the_corpus` replays recorded model I/O, builds empty model snapshot maps (:462), sets no equivalence margin (:498), asserts only match/metric (:667/:679). This fixture remains useful fixture evidence; it cannot admit S3. `crates/hh-bench/src/adapter.rs:426::BenchmarkAdapter` and :509::parity_ref are existing adapter seams.
11. `crates/hh-eval/src/facts.rs:74::EffectRow` has idempotency keys but no seq; `:278::CompactionRow`, `:530::LedgerFacts`, `:769::from_rows` and :1214 compaction fold are event projection homes. `crates/hh-eval/src/context_metrics.rs:102::fold` collapses forgotten IDs to one seq and counts unique reacquisitions; repeated actions use capability/arguments. `crates/hh-eval/src/catalogue.rs:454` registers old count names. New exact joins need preserved event IDs/sequences, source execution-stop boundary and versioned definitions; keep approximations unchanged.
12. `crates/hh-embed/src/analysis_ops.rs:175::lab_analysis_analyze` resolves AnalysisSpec/design, ResultsRows and manifests; :232 reads whole event streams and :280 accepts external seed/draws. New historical report acceptance must prove prefix/watermark-bound facts and selected grade versions, not latest-history leakage. `crates/hh-analysis/src/report.rs:51::generated_from`, `:86::report_id_for`, `crates/hh-analysis/src/engine.rs:24::analyze_and_record` already provide content identity and idempotent serving; extend exact inputs rather than bypassing them.
13. `crates/hh-lab/src/experiment.rs:492::SchedulingPolicy`, `:552::ReattemptPolicy`, `:846::ExperimentSpec`, `:2450::RunPlan`; `crates/hh-lab/src/expand.rs:472::expand` creates immutable cells/repeats and cold namespace salts. Generic scheduling exists, but it does not itself certify the required fair task-level arm/lane coin and five rounds; those require explicit deterministic plan assertions.
14. Existing real-route tests to extend, not treat as adequate evidence: `crates/hh-analysis/tests/kernel.rs:705::compare_produces_paired_report_through_analyze`, `:888::equivalence_verdict_within_margin`, `:1153::multiplicity_null_fwer_and_labels`, `:1224::report_id_deterministic_and_served`, `:1270::projection_refusals`; `crates/hh-embed/tests/conformance.rs:4570::lab_analysis_analyze_round_trip`, `:4720::lab_analysis_analyze_refusals`; `crates/hh-embed/tests/cap_2_composed.rs` analysis/experiment route around :1721–1980. Old null test has 200 datasets, not D2's full ≥2,000-per-scenario battery.

## Candidate bounded implementation contracts (local names only; coordinator allocates IDs)

Estimates are peak working-set tokens including bounded source/spec/test reads, implementation reasoning and verification headroom, not file LOC or elapsed-time estimates. Target 16–26k; use ≤30k soft maximum pending fresh review. Never load entire 40KB+ files. These can be fully scoped now as implementation work conditioned on exact ADR admission. If the admission changes payload/interface design, leave the affected contract as a skeleton until that admission resolves it; no Run line on a skeleton. No claim here authorizes implementation.

### M-A — Planned populations, immutable selection and analysis coordinates

Primary proposed IDs: **REQ-D-001, D-002, D-003, D-104**. Consumers: E-202/E-204 grade owners; F-003/F-004/F-203 receipts/recompute; all following methods; AC-3-1, AC-4-1 and AC-6-1/2. Depends on admitted P1 required-grade/selection and authoritative plan-disposition interface; may use its parameterized offline fixtures before external execution pins.

Scope: one analysis-facing planned-slot and metric-population contract; resolve first subject per slot and historical selected grades; preserve six classes/availability/veto axes, equal task weights, raw/canonical/joint outcomes and first-admissible-retry secondary. Feed compare/scorecard/export from this same declared selection. All-planned binary completion bounds and bounded/unbounded continuous/censored descriptors belong here. Exact method/range metadata and canonical-input identity are shared decisions owned here (with later vocabulary additions owned by individual method tickets). No estimator or new grade/settlement authority.

Anchors: seams 1–5/12; P1's existing plan view and selected-grade producers. Size **22–26k**; bounded edits across eval runs/compare/scorecard, analysis project/input, Lab/ontology payloads and narrow embed boundary/schema consumers. If expansion exceeds this, split at the established population contract vs consumer integration only after naming the first ticket sole contract owner.

Acceptance:
1. **E positive:** mixed scored/budget/infrastructure/oracle/refused/cancelled/veto/unknown dataset reconciles plans, first subjects, all attempts, gradeable, selected/base/clean denominators across scheduler/results/compare/scorecard/export; retries/regrades add zero n and zero duplicate charge.
2. **E positive:** D1 fixture returns planned contrast [-.4,.5] exactly; full unrun task remains in equal task weights; raw veto pass versus joint veto failure yield declared different bounds.
3. **E negative:** required pending grade, missing task/slot/grade key, duplicate/reused subject, incompatible selectors, short/unequal required cells and incomplete selected universe cannot become a full-manifest estimate. Keep partial descriptives and typed refusal.
4. **E negative:** unknown check never clean; unbounded cost/intensity receives no invented finite bound; terminal usage/time missing is right-censored lower bound, not cap completion.
5. **E positive/negative:** old watermark replays old grade selection and exact reports; later correction creates new identity and never rewrites old inputs. Explicit B−A new target and untouched A−B legacy path test sign reversal.
6. **R dependent:** actual study report must reproduce the same frozen selectors/counts; this implementation pass alone leaves registry R layer open.

### M-B — Early finite-manifest Bayesian comparison, routed and persisted

Primary **REQ-D-107**; consumes D-001/002/003, E-209 (method review owner), F-103 offline lesson. Hooks AC-4-2, AC-3-1, S10/AC-7-1. Depends M-A + admitted finite-task efficacy amendment §9.3; no live parity/provider/task pins dependency.

Scope: independent integer-Beta task/arm posteriors, equal within-stratum contrast; reuse xorshift64*/inverse-CDF primitive; analytic posterior mean, observed contrast, fixed 8,000 credible draws and registered numerical-error doubling through 64,000; fixed Beta(1,1) and separately labelled Beta(2,2) sensitivity; valid per-cell Wilson only. Actual eval→analysis→native persisted report route and exact metadata. No pooled/hierarchical/population/correlated model, TOST or practical-margin/omnibus certificate.

Anchors seams 1/2/5–8/12/14. Size **20–24k**. Numerical validation tolerances/doubling stopping/report representation must be frozen by method admission, not chosen after observed data or silently invented here.

Acceptance:
1. **E positive:** independent analytic/known-answer cell means and task contrast; heterogeneous cells show different output from pooled Beta; primitive unequal-n checks preserve prior per cell (they do not waive the full path's required-cell design/floors).
2. **E positive:** canonical inputs/seeds produce exact repeat IDs/body, recorded actual draws match computation; changing one valid observation changes result and identity; numerical-error/doubling record follows predeclared procedure.
3. **E positive:** all-success/all-failure cells retain nonzero valid uncertainty; EX-02 inputs T1 A1/5 B4/5 and T2 A3/5 B2/5 give observed B−A mean .20 and analytic Beta(1,1) mean 1/7, without requiring significance.
4. **E negative:** one task, missing keys, inadequate required cells, unsupported dependence/correlated-run assumption, unknown method, nonfinite/invalid counts and unadmitted selection refuse through lab.analysis.analyze and persisted report, not only helper tests.
5. **E negative:** report cannot advertise credible interval as confidence/TOST; sensitivity cannot overwrite primary prior; unsupported metadata-only method switch fails.
6. **E consumer:** EX-02 may become informative after this exact path's admission/validation and P1/P3 workflow prerequisites; full study simulation remains a later confirmation gate.

### M-C — Mechanism event joins, denominators and registry definitions

Primary **REQ-D-005, D-106**, with suggested **REQ-E-004, E-107** primary here if the execution delegate agrees. Consumers treatment-to-wire E-206 and analysis E-209; AC-2-2, AC-4-1. Depends admitted detector amendment + P6 actual retained/omitted/payload producer and P1 execution-stop boundary + M-A population/reducer contract.

Scope: completed-compaction exclusive-start/inclusive-execution-stop windows, overlapping (c,event,item)/(c,effect) joins, exact idempotency keys, repeated deliveries, native intensity/range units; eligible trigger/fidelity/fallback/ineffective counts, recall/extractor/summary applicability and same-unit overhead/latencies. Sum numerators/denominators within subject→ratio→available-subject mean within task→equal-task contrast; coverage retained. Never infer causal forgetting, clamp intensity to probability or relabel old approximations.

Anchors seam 11; `context_metrics::fold/emit`, `LedgerFacts::from_rows`, existing context compaction producer and catalogue. Size **20–25k**. Producer payload changes and math must remain together when they share event join decisions, or be owned explicitly by P6 and consumed verbatim here.

Acceptance:
1. **E positive:** golden overlapping compactions, repeated same-item deliveries, exact matching/nonmatching idempotency keys produce independently hand-countable joins and numerator/denominator sums; intensity >1 survives unclipped.
2. **E positive:** next-dispatch joins prove actual payload fidelity; assigned arm retains fallback/deviation, untriggered subjects stay in task success.
3. **E negative:** missing key/ID/extractor => no_detector; zero denominator => estimator_undefined; no summary => inapplicable; no compaction => count 0 but conditional latency/rate unavailable. Required unknown retains family membership.
4. **E negative:** post-execution grading events cannot enter window; duplicate event/key cannot silently weight tasks; old capability/argument approximation keeps its original registered name.
5. **R dependent:** actual protocol freezes detector/extractor versions, applicability/windows before collection; native receipt can recompute every numerator/denominator. No absent detector can be waived into a primary result.

### M-D — Task-mixture sensitivity, sharp-null tests, complete families and tails

Primary **REQ-D-008**; D-109 simulation owner consumes this implementation, as do F-004/F-203 and E-209. Hooks AC-3-1, AC-4-2. Depends M-A/M-B metadata/route contract, M-C for real mechanism inputs; synthetic numerical implementation can precede M-C if raw cluster contract is already fixed.

Scope: whole-task paired bootstrap, BCa ≥30 tasks/percentile below, actual declared seed/draws with ≥2,000 confirmation floor; explicit target/degeneracy. Sign flips only when registered lane/exchangeability argument admits sharp-null testing, exact ≤20, else 8,000 with +1 correction. Full frozen three-primary×confirmatory-stratum family including missing/unavailable members and across report splits; Holm .05, separate frozen exploratory BH. Signed/absolute effect tails, arm cost/wall/severity P95/P99/max, n/censoring and clustered uncertainty where valid. No finite-manifest credible claim or weak-mean/practical-margin significance.

Anchors seams 6–8/12/14. Size **24–28k**; high end, but common task clusters/family routing are one shared decision. Do not create a standalone tail-report store.

Acceptance:
1. **E positive:** independently calculable whole-cluster bootstrap preserves task pairing/weights and declared draws; 29/30 task boundary selects percentile/BCa; changed seed/draws changes actual computation and identity.
2. **E negative:** one task, absent cluster keys, unsupported dependence and genuinely degenerate bootstrap emit explicit unavailable rather than invented interval. Constant binary Beta/KL remains admissible under its own path.
3. **E positive:** small-T exact enumeration and >20 seeded +1 p-values; a constructed non-exchangeable/shared-shock design refuses the sharp-null test; report names sharp no-effect target.
4. **E positive/negative:** split reports/strata recompute the same Holm adjusted results from one frozen family; missing member is retained as unavailable/nonrejecting rather than removed, missing entire family identity refuses. Exploratory BH cannot replace primary Holm.
5. **E positive:** signed tails retain direction, absolute tails remain separate, P99 nearest-rank may equal max with n stated; censoring/unknown/unbounded quantities retain limits. No observed max becomes a population bound.
6. **E negative:** no unqualified omnibus verdict combines posterior, task-mixture and sharp-null results; no equivalence-IUT exception leaks into efficacy correction.

### M-E — Directional KL, bounded contrasts and inference labels

Primary **REQ-D-304, D-305, D-306, D-307**; **REQ-D-108 remains alias/superseded ONLY to D-304**, with D-309/310 dependencies. Consumers M-F authentic pairs and M-G admission review; AC-4-3/4. Depends M-A input contract + admitted KL/grouping/relative/IUT/S11 amendments. Can be tested with synthetic pairs before authentic runner execution.

Scope: implement exact selected Bernoulli KL inversion with continuous endpoints, directional disagreement bounds and planned-N missingness envelope; bounded absolute and relative-mean contrasts using physical ranges. Strict outward-rounded equivalence, .90 TOST vs .95 reporting, S11 negative claims (including two relative negative branches at .025 each or stricter registered selected-claim allocation). One frozen all-endpoint/all-stratum conjunction IUT versus selected-claim correction; required unknown/veto never removed. No CP engine, score/MOVER fallback, observed-reference division, clipping or new fitter.

Anchors seams 6–10/12/14. Size **24–28k**; retain scalar math + label/margin routing together because separating them recreates current raw-unit/relative and confidence mistakes. Frozen family identity comes from M-D/M-A, but parity correction is method-specific.

Acceptance:
1. **E positive:** endpoint x=0/x=1 limits, monotonicity/sign reversal, small-N/heterogeneous concentration tails, exact known a=b versus observed zero variance; zero-discordance width fixtures N10/30/150 ≈ .308497/.115703/.024293.
2. **E positive/negative:** equality/touching band => inconclusive after outward rounding; .90 inside => equivalent; .90 disjoint alone cannot infer not-equivalent; .95 wholly outside may do so only with applicable selected-claim allocation. Exercise strict boundary perturbations at encoded resolution.
3. **E positive:** uncertain pairs widen conservative completion envelopes at original planned N; raw/canonical/joint veto populations retain different known/unknown counts.
4. **E negative:** unequal required R/weights, reused subjects, missing pair/task/phase keys, empty pins, known shared shocks/carryover and invalid/nonfinite counts refuse on report path.
5. **E positive/negative:** unit-scale-invariant relative means, both negative branches correctly allocated; zero/near-zero reference without frozen absolute convention refuses; unknown/unbounded physical range and missing completion bounds refuse. No raw milliseconds-vs-ppm comparison or mean-of-task-ratios substitution.
6. **E positive/negative:** all-required conjunction with unknown member cannot pass; selected claims require recorded correction; IUT never suppresses D2 Holm. Metadata-only method selection fails integration tests.

### M-F — Authentic parity records, mapping and policy applicability

Primary **REQ-D-004, D-301, D-302, D-303, D-308**. Consumers D-201 correct/wrong/broken oracle controls, D-105 readiness, D-309 quote and D-310 admission; AC-4-3/4 and AC-5-1. Depends M-A/M-E, P1/P2/P3/P6/P7; exact runner/model/task/environment choices belong HUMAN/gates, not this generic admission code.

Scope: owner-routed authentic first-attempt HH/original pair manifest; signed pre-data dimension→metric/estimand/population/range/unit/margin/applicability/evidence map; actual identity/replay/floor/seed support checks; actual both-policy applicability or predeclared identical-bridge reuse proof. Preserve narrow grouping exception without granting honoured-seed/CRN/R2 claims. The map is not four automatic cap outcomes. This implementation validates supplied evidence; it does not manufacture authentic runs.

Anchors seams 3/7/9/10/13, `BenchmarkAdapter::parity_ref`, existing Lab ParityReport and suite validation. Size **21–25k**.

Acceptance:
1. **E positive:** pinned complete first-attempt synthetic admission fixture has traceable H/O pairs, equal T/R/weights, map and replay coverage, adapter ≥3 and S3 ≥5 floors; required metrics route to M-E unchanged.
2. **E negative:** old recorded-model-I/O corpus parity fixture, ArtifactBenefit wrapper alone, empty equal snapshot maps, oracle transport equality and seed presence cannot establish authentic equivalence; missing replay remains unmatched.
3. **E negative:** absent/late map, missing required endpoint/check or dropped difficult endpoint blocks overall equivalence; typed inapplicability records reason and never counts as passed.
4. **E negative:** baseline-only A parity cannot admit policy B absent prespecified identical-bridge reuse; b_A=-.10/b_B=.10 counterexample precludes treatment transfer. Mixed Bayesian/frequentist lower bounds cannot produce joint 95% certificate.
5. **D/R gate:** authentic evidence for actual participant/configuration must satisfy independent-across-pairs rationale and actual source/verifier/rights pins; synthetic E pass cannot close admission R. Nonlinear pass^k/conditional eligible-only endpoints lacking their own admitted method stay unavailable, not silently dropped.

### M-G — Numerical pipeline review and method-admission evidence

Primary **REQ-D-109, D-310**, suggested **REQ-E-209** if coordinator assigns the complete cross-method acceptance home here. M-B/C/D/E/F are consumers/contributors; do not double-own their REQs. Hooks AC-3-1/4-1/4-2/4-3, AC-9-1. Architecture ADR ownership can remain the common H-004 ticket; this ticket owns the composed methods evidence/admission packet.

Scope: frozen numerical validation protocol (parameters/seeds/error tolerances), independent derivation/routing review, exact owner pipeline simulations, all method metadata/refusal/version/draw/persistence evidence; no unapproved empirical study or provider execution. The known design fixes T={10,20,30}, R={5,10}, null/±.05/±.10, easy/hard/heterogeneous/zero-activation/informative-missingness/veto/shared-shock scenarios, ≥2,000 datasets/scenario and required outputs; implementation must not substitute the 200-dataset old multiplicity test.

Anchors seams 6–9/12/14; D2 F6/F8, D4 F8 and supplement §9.3. Size **18–23k**; planned offline execution under separate implementation authorization and ≤8 h, 4 cores/8 GiB, 32 core-hours, 2 GiB. This envelope covers the D2 numerical tranche only.

Acceptance:
1. **D positive:** exact before/after legacy clauses/ADR relations, affected IDs, method tags/payload compatibility, forbidden alternatives, reviewer/approver and revisits are recorded; old frozen source/history unchanged without protocol admission.
2. **E positive:** retain scenario generator/parameters/seeds/failures and ≥2,000 datasets/scenario results for availability, interval width/verdict probabilities, prior sensitivity, relevant type-I/coverage quantities and Monte Carlo error. Bayesian credible coverage must be labelled diagnostic, not automatic nominal frequentist guarantee.
3. **E positive/negative:** known answers, heterogeneous/unequal-n primitive checks, missingness/veto/shared-shock invalidity cases and actual eval→analysis→persisted report routing all pass or explicitly refuse as designed; instrumentation confirms requested method/draws actually executed.
4. **E negative:** exceeding numerical resource bounds stops retaining partial evidence; failed/unexecuted scenarios cannot become a method pass; unspecified tolerance or unreviewed numerical discrepancy prevents admission.
5. **D gate:** independent reviewer/operator records scope-specific admission; unknown precision => stop/requote or explicit limited-scope decision, never favorable-method fallback. Full method-dependent confirmation remains blocked until acceptance.

### M-H — Central protocol materialization and deterministic schedule

Primary **REQ-D-007, D-101, D-102, D-103**. Consumers D-105/110/309, M-A, actual study/report and E-011/B-103/B-106. Hooks AC-3-2, AC-4-1/4. Depends M-A plan contract + owning experiment scheduling; study inputs remain external immutable data.

Scope: materialize/validate a protocol with TB2 catalogue identity, outcome-blind eligible/excluded set, dev/held-out separation, fixed arm/control/trigger/model-role/cap factors, 30-task seeded selection without replacement under supplied quotas, task-level fair lane coin/five rounds, first slots/retries, phase IDs and frozen analysis/family/stops. This ticket owns the reusable protocol/schedule machinery and records that later actual R evidence remains owed. It does not choose actual tasks/seeds/quotas/margins/tariffs.

Anchors seam 13, `ExperimentView`, `PreRegistration/Design`, bench suite/task manifests; tests in hh-lab/unit and experiment engine. Size **18–22k**.

Acceptance:
1. **E positive:** same supplied manifest/metadata/seed yields same selection/schedule and protocol identity; no replacement, equal within-stratum weights, fair task-level arm/lane allocation, five interleaved rounds, ≤2 concurrency and one/arm are independently checked.
2. **E negative:** hard/untriggered tasks cannot be removed using outcomes, insufficient eligible held-out population refuses, dev/held-out overlap or late split refuses; no artificial pressure/third arm/policy search.
3. **E positive/negative:** first-subject/first-admissible-retry remains deterministic under completion-order/retry variations; ≤2 reattempts/plan and ≤10% plans, infrastructure classes/backoff supplied and frozen; wrong-answer retry refused.
4. **E negative:** unknown warmth, drift, altered required roles/fingerprints/route/seed support, changed protocol after subjects, material oracle correction and unapproved cap/margin edit require refusal/new cohort; no relabelled pilot or favorable top-up.
5. **D/R dependent:** actual protocol hash must predate all dependent subjects; later report demonstrates schedule and selections rather than merely storing a template. Pinned exact inputs are a gate, not implicit defaults.

## Execution packet, gates and skeletons

### M-I — Feasibility/quote and readiness packet (D; complete contract now)

Primary **REQ-D-105, D-110, D-309**. Hooks AC-4-4/9-1; consumes every implementation slice, actual P2/P6/P7 acceptance and rights/retention owners. Owner: study/execution owner and operator; trigger before each activation/pilot/parity/confirmation/replication. Size **12–17k**. No new data collection or paid execution is part of packet preparation.

1. Enumerate satisfied/unmet actual treatment/cap/created→finished/oracle/detector/parity/method prerequisites with native evidence; empty/missing prerequisite withholds dependent dispatch/headline.
2. Require exact participant/policies/manifest/split/strata/R/model roles/requested-served identity/oracle/image/runner/rights/isolation/cold-start/drift, signed endpoint/population/range/margin/applicability map and independence rationale; absent values remain unresolved, never fabricated.
3. Quote all runners/policies/endpoints/strata and costs: subjects `2TR≥10T`, original-runner pairs separately, pilot/activation/retries/regrades/instruments/cancellation/exposure, prices, CPU/RAM/time/full archive/backup/retention. All unset currency=$0, unknown finite exposure => zero dispatch.
4. Show plausible disagreement/precision scenarios, including zero and nonzero directional discordance; D4 widths are attainable diagnostics, not power. Do not confuse 300 A/B subjects with 300 H/O pairs or dev-task agreement with held-out agreement.
5. Keep stage envelopes/fixed final N and exact stop rules; infeasible parity/precision requires stop/requote/explicit scope ruling, leaving S3 unmet while independent S1/receipt work may advance. No automatic top-ups, clouds, task/method/margin shopping.

### Required HUMAN/GATE stages and post-gate skeleton scopes

A complete *gate/packet* contract can be authored now, but actual collection contracts must remain **Kind: skeleton**, no Run line, until exact execution-specific inputs and authority are signed. Gate pending means RETURN PASS, not invented technical blockage or pre-signed approval. Gates record words/UTC/channel/items/expiry/void conditions; Q-7/D4 acceptance grants neither run funding nor method ADR admission.

1. **Methods/contract gate:** independent numerical/routing/derivation and exact §9.3 ADR admissions. An early finite-efficacy scope may be separately admitted for offline EX-02; full study admission requires full pipeline. HUMAN owner reviewer/operator; trigger before dependent semantics/inference.
2. **Exact execution inputs/backend/rights gate:** study owner/operator supplies task/oracle/runner/model/image/roles/quota/seed, original-runner feasibility, backend confinement, cold/cache and required map/independence/margins; artifact owner supplies permissible retention/access/full archive. Zero dispatch if missing. These are binding later inputs, not broad missing-design tasks.
3. **Activation/pilot authorization gate → S-A skeleton:** separately quoted one-task activation, then ≤60-first-subject pilot only under exact stage approval. Fixed arms/zero search, ≤100 calls, 1M input/100k output, 30-min subject wall/≤15-min verifier, ≤2 concurrency, ≤4 cores/8GiB per subject, ≤200 core-hours/50GiB tranche; no pilot→confirmation relabelling. Retain failure/activation/variance/tail/cost/precision evidence; no favorable task selection. Expected working set **12–18k** once pins exist. Pilot can be null/unfavorable and still furnish honest feasibility evidence; task success requirement differs from research effect acceptance.
4. **Parity feasibility/frozen-N authorization gate → S-P skeleton:** no allocated parity tranche. Price and freeze actual required endpoints/policies, both runners, sample, margin and independent-across-pairs design, preserving replay/pin/class/veto coverage. Run one fixed final-N tranche only; equivalent admission must cover both actual policies or preregistered same-bridge reuse. Non-equivalence/inconclusive retains results and blocks dependent component confirmation; no CP fallback or favorable continuation. Expected **14–20k** once actual original-runner integration/pins are known.
5. **Confirmation pre-registration gate → S-C skeleton:** after full method/treatment/cap/detector/oracle/parity admission and complete quote, freeze actual 30-task held-out manifest, all input/margin/family/stopping identities and ≤300 first subjects (same individual caps, ≤900 core-hours/200GiB/14days). Execute only fixed final analysis; integrity/isolation/leakage/treatment/accounting/drift/ceiling stops preserve every partial row/cost; no retrospective smaller manifest. Expected **16–22k** for one collection/report-producing run, with dedicated partial/inconclusive disposition rather than falsely claiming completion.
6. **S-R report/recompute skeleton:** primary proposed **REQ-D-009**; consumers all D/E/R requirements above, F-004/F-203 receipt and publication owner. AC-3-2/4-4/9-3. Finish only on actual admitted cohort or retain unmet obligations explicitly. Report expected first-attempt finite-manifest efficacy with separate observed/posterior/task-mixture/sharp-null/parity claims, all populations/bounds/tails/unavailable members/failures/costs, prior/numerical/simulation limits and actual identity. Null/negative/inconclusive may satisfy research; missing admission/unexecuted cohort may not. Public defaults/adoption decision separate. Exact recomputation requires identity/count/order/verdict agreement and frozen portable scalar rule `1e−10×max(1,|reference|)`, with no threshold/verdict change allowed even within tolerance. Expected **14–20k**. Publication/recipient rights are separate gates; reporting internal admitted outcomes is not permission to publish.

R ownership must stay visible across skeletons. Do not mark D-001/002/003/101/102/103/104/106/107/109/301–310 fully satisfied from E-only tickets where their registry layer includes R; the single primary owner may reference execution/report consumer evidence, without allocating duplicate requirement owners. If the manifest convention requires one ticket to close the full predicate, make that owner the final relevant study skeleton and label earlier code tickets consumers; do not drop the R predicate.

## Complete D-family proposed primary-owner index

This table allocates each adopted D candidate exactly once locally; coordinator may adapt IDs/grouping while retaining source predicates. Non-method homes require coordination with other read-only maps.

| IDs | Proposed unique primary home | Required consumer proof |
|---|---|---|
| D-001/002/003/104 | M-A populations | actual R report/receipt retains complete selection/bounds |
| D-004/301/302/303/308 | M-F authentic parity | useful-task oracle controls; actual parity and policy scope |
| D-005/106 | M-C detector joins | treatment producer/native trace; actual R protocol |
| D-006 | P7 real-cap/accounting owner (outside this memo) | compare/quote use authoritative physical usage |
| D-007/101/102/103 | M-H central protocol | actual frozen selection/schedule/phase R evidence |
| D-008 | M-D task methods/families/tails | M-B/M-E known answers and M-G review contribute |
| D-009 | S-R admitted study report | null accepted; absent margin/opportunity/admission not pass |
| D-010/202 | recipient procedure owner | distinct verify/replay/R0/R1/recompute/fresh/independence |
| D-105/110/309 | M-I staged readiness/quote packet | S-A/S-P/S-C actual admission/cost/limits |
| D-107 | M-B finite-task efficacy | M-G numerical acceptance, actual report R |
| D-108 | superseded-by D-304 ONLY | retain dependencies D-309/D-310 and diagnostic CP width history |
| D-109/310 | M-G composed numerical/admission evidence | full family/stops M-D/M-H; formal ADR and actual dispatch |
| D-201 | useful-task/oracle owner | two fixed real subjects; correct/wrong/broken controls |
| D-203/204/205/206/207 | bounded operations owner | O1–O8×3 reached/observed/sentinel/counts/unknown/probe/permission/Account, O8 detected defect not invariant-held |
| D-208 | contribution/verifyfixed owner | baseline/repair identities and all six R0 controls; preserve A3 |
| D-209 | optional plugin/conformance owner | four actual package controls, skips, kernel hash and build costs |
| D-304/305/306/307 | M-E KL/relative/confidence/family semantics | M-F identity/mapping + M-G review + actual parity R |

The original D3 predicates cannot be folded into a generic statistics ticket. O1–O8 do not count as efficacy repetitions or establish a population failure rate; S1 requires a correct independent artifact, while a failed task may still have valid R0/recompute evidence. Optional plugin remains bounded and contingent, not a new study arm.

## Missing inputs and potential design decisions to surface explicitly

No wholesale method redesign is needed: the ratified source chooses finite-cell Beta, task bootstrap, sharp-null sign flips and directional KL, including strict confidence-label rules. Exact tasks/model/environment/backend, endpoint map/ranges/margins/independence, funding and rights are expressly deferred binding gates (H2 F8), not permission to guess.

The **numerical-validation protocol is not yet execute-grade data**: D2 specifies families of simulation scenarios and draw-doubling but does not pin numerical-error tolerance, full scenario probability/dependence/missingness parameters, generator seeds, precision-admission thresholds or exact error-doubling stopping criterion. M-G must freeze those in the reviewed method/validation admission packet before running the numerical battery; absence blocks that execution, not unrelated decomposition. If these choices change inference/acceptance rather than merely instantiate the stated battery, route a narrow explicit amendment/design decision. Do not silently invent a prior/fitter/method or call a broad scenario label a test vector.

The **new public metadata/encoding** (method/version/target/prior/draws/interval levels, planned population and pair IDs, physical range and separate reporting-vs-TOST intervals) is required semantically but not allocated stable wire field/tag names in the supplement. The owning ADR/schema ticket must pin additive/optional compatibility, unknown-version refusal and canonical representation before dependent implementation; the decomposition must not advertise proposed names as implemented public APIs. Existing integer-only canonical JSON (`crates/hh-wire/src/json.rs:15::Json`) and ppm scalar paths are a concrete encoding constraint: numerical outward rounding and full-precision recipient comparison must be explicitly consistent with the admitted representation. No independent float codec is allowed.

No independently admitted **original-runner effect-transfer/joint uncertainty design**, nonlinear pass^k inference or conditional eligible-only parity method exists in this selected scope. Required endpoints needing one must yield unavailable/stop until separately admitted; do not implement new methods to fill those holes implicitly. Baseline-only parity or a Bayesian lower endpoint plus frequentist calibration does not close them.

## Minimal verification plan and current limits

Future scoped checks: `cargo fmt --all --check`; hharness-eval unit/acceptance, hharness-lab schema round trips, hharness-analysis kernel tests, hharness-bench admission tests and narrow hharness-embed actual RPC/persistence batteries for each changed slice; `scripts/check-drift.sh` whenever schema/codegen changes, appropriate removability/tier checks, method-specific retained simulation evidence separately. Do not run the entire simulation suite in every ticket or routinely launch live collection. New behavior needs meaningful regression failures on this baseline; do not accept labels/assertions that mirror implementation.

**Executed during this analysis:** read-only row reader, file/symbol searches/reads, git HEAD/status and spec digest. **No tests were run, no live prerequisites were probed, no evidence class was upgraded.** Repository working-tree changes observed were existing/coordinator work; this agent wrote only this `/tmp` memo.

## Sequencing clarification for coordinator (avoid a circular gate)

Distinguish three events in the chain: **(1) method/design and architecture admission** (D4 independent proposal review is already accepted; formal §9.3 ADR/manifest admission and planned owner-routing review remain before affected new implementation), **(2) authorized implementation plus its independent numerical/actual-routing verification** (M-B/C/D/E/F and M-G produce new evidence), **(3) execution/inference authorization** for the exact frozen study after the new implementation evidence, actual pins/map, parity and resource gates. M-G is the implementation-evidence/readiness owner, not a reason to postpone the initial ADR until after product semantics have silently landed. An early, scoped efficacy admission/verification may unblock EX-02 independently of the later full central-study gates. Keep these gate records distinct; Q-7 does not stand in for event (1) or (3).
