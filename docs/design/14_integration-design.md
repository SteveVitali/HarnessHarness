# 14_integration-design — Native execution, then instrument grading

Stream: E — Execution and integration\
Researched: 2026-10-29 UTC\
Author: /root/e3_integration\
Ledger rows covered: E3\
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved planning/accepted inputs.\
Confidence overall: medium.

**Recommendation:** retain the restricted native runner; finish the subject at its actual execution boundary, then grade its sealed submission in an instrument run and settle through an explicit grade-aware path. Preserve M1, legacy behavior and immutable history. Missing runtime bindings, provider bounds, a shared environment, recipient closure and **method/parity admission** remain material gates. No execution authorized here.

## Basis

Read [brief](../brief.md), [ledger/Q](../research-ledger.md), [conventions](../research/CONVENTIONS.md), [trace](../OUTLINE_TRACE.md), [C1](06_demo-and-study-portfolio.md), [E1](07_execution-seams.md), [C2](08_feasibility-and-scope.md), [D1](09_measurement-standard.md), [D2](10_compaction-protocol.md), [D3](11_operational-proof-protocols.md), [E2](12_feasibility-findings.md), [F1](13_receipt-contract.md), root/docs/crates instructions and `synthesize-spec` run mode (skills `e981ec57d487a2c41d416672483b5a4df15b1233`). **Source retrieval: 2026-10-29T20:53:30Z**, baseline above. Interfaces/events are proposals; inspection is not execution.

## Integration findings

### F1 — One restricted recipe composes the existing owners
Claim: Compose existing owners without a second runtime/store or upward dependency.
Status: UNVERIFIED\
Evidence kind: design proposal; source inspection\
Evidence: [facade](../../crates/hharness/Cargo.toml)/[embed](../../crates/hh-embed/Cargo.toml) manifests and inspected results→experiment/both→eval edges; E2 F1 private arming.\
Retrieved: 2026-10-29T20:53:30Z.\
Implication for the spec: Keep orchestration above narrow owner interfaces.\
Outline delta: EXTENDS OL-05–OL-08, OL-20–OL-21.\
Limitations: Composition/backend/pins untested.

One `hharness` Rust recipe; [BenchmarkAdapter](../../crates/hh-bench/src/adapter.rs), 426–481, keeps production stdio/in-process fixtures. No embed→bench, experiment→results, lower→runner edge or synthetic success/public RPC expansion.

| Source owner | Proposed interface | Durable addition/extension | Acceptance |
|---|---|---|---|
| experiment/budget | `launch_bound` → lease/root/funding | Launch intent; funding/grade binding | Crash-safe single subject/root. |
| embed arming/drive | `adopt_subject(binding,environment,ports)` | Execution profile | Stale/duplicate/mismatch refuses. |
| control/env Dispatcher | Intended coordinate→durable outcome | Existing lifecycle once | Real sink, one commit/terminal. |
| bench/verification | Sealed submission→executed oracle refs | Invoked/verdict: subject/submission/oracle/attempt/policy/cost | Correct/wrong/broken controls. |
| eval + experiment/results | Grade facts/stop/policy→selection; `plan_disposition_at` | Execution-settled/pending; settlement/scoring revisions | Pending not scored; no recharge/new n. |
| budget/gateway/control | Prepared bounds→reserve→dispatch→charge | Budget/model bindings | Four caps; dedup totals. |
| context/embed/gateway/eval | Retained view→wire; joins→metrics | Retained/omitted refs | Changed bytes; detector availability. |
| bundle + view owners | Closure roots/views→export | F1 native profile | Recipient closure; correct R0. |

Launch: persist intent→allocate/open→bind root/subject→publish→arm. Recover linked manifests or abort/release idempotently; no cross-run atomicity, orphan spend or replacement subject.

### F2 — Grade after terminal; preserve M1 and explicit pending state
Claim: Post-terminal grading excludes the independent oracle from subject M1 without redefining M1.
Status: UNVERIFIED\
Evidence kind: design proposal; source inspection\
Evidence: [M1 §2.5](../../spec/sections/05h-measurement-evolution.md) fixes created→finished; [driver](../../crates/hh-control/src/driver.rs), 5009–5017, uses turn start. [settle](../../crates/hh-experiment/src/engine.rs), 1508–1675, uses terminal facts/charges once; [projection](../../crates/hh-results/src/projection.rs), 202–292, latches overlay oracle failure. E2 F2/F4 and inspected Store::append/ScoringContext support closed-subject overlays.\
Retrieved: 2026-10-29T20:53:30Z.\
Implication for the spec: ADR-admit grade-aware settlement; resolve E2 timing.\
Outline delta: EXTENDS OL-05, OL-10, OL-15–OL-16, OL-20–OL-21.\
Limitations: Settlement/timing unimplemented.

Preterminal grading includes oracle delay despite a separate Account; exclusion requires a clock amendment. **Select post-terminal grading**: M1 unchanged, instrument/end-to-end elapsed separate. No backdating/subtraction; subject validation/repair stays charged, benchmark feedback never returns to participant.

Declare required-grade mode/pins/selection **before launch**. Absence preserves legacy behavior; unknown required features refuse. Pending is admission state, not outcome class. Results/query/export/scheduler/compare/analysis must gate default `Completed`→scored/success/completion; rows show unavailable metrics/reasons.

1. Stop actions, drain committed effects to observed/unknown, revoke participant access, finalize usage and seal immutable end-state/submission. Drain/seal remains inside M1; failed sealing cannot yield a mutable-workspace grade.
2. Driver records actual finish/stop/veto. Enforcement and report M1 use **persisted run-created origin**, including launch→turn delay, approval wait, restart downtime and drain/seal. Correct current turn-start payload; unknown legacy origin stays unavailable. Existing ledger/clock authority supplies time; untrustworthy resumed deadlines refuse, never reset/use a shadow clock store.
3. Experiment records proposed `execution_settled`: terminal ref, deduplicated cost, returned funding, once. Grade-pending blocks scored settlement/relaunch/non-partial close; partial close lists unresolved slots. Resource release does not imply correctness.
4. Bench opens instrument run/Account, persists invocation before unchanged pinned oracle execution on sealed submission, then output/exit/timeout/access/grade/finish. Preserve declared isolation/shared tamper `no_detector`. Crash/empty/nonfinite/refusal means oracle failure; wrong output means grade zero.
5. Pure eval selects validated facts/base stop/policy; callers validate target/submission/oracle/causes/watermarks via existing ledger access, no eval→ledger edge. `settle_graded` records first disposition through ontology rules without recharging. Oracle failure triggers regrade, never participant retry.
6. Results and `plan_disposition_at(scoring_ref)` use the same selection. Valid grading restores original stop-derived class, possibly budget-exhausted. Keep grade/stop/veto separate; pin analysis/export watermarks.

Regrades are immutable instrument attempts on the same subject/submission. First-valid within the frozen grade set forbids best-reward/last-write-wins; later failed retry cannot erase valid grade. Authorized correction/invalidation creates selection revision/reason/supersedes and may change effective outcome; revoked evidence remains unavailable/oracle-failed until replaced. Preserve failure→success→later failure history.

Original settle stays exactly-once. Revised plan/results views change outcome/pending without reopening history/relaunch/recharge/new n; old watermarks reproduce, new reports/bundles supersede.

**Crash tests:** pre-seal/finish resumes native subject; finish→grade restores pending without participant work; invocation→verdict retains ambiguity and only capped instrument retry; verdict→settle reuses verdict; transfer→disposition cannot recharge. Add delayed adoption/restart/long verifier, duplicate settlement, budget-exhausted grade, veto, invalid targeting and correction tests. Recovered selection/cost must match uninterrupted views.

### F3 — Prepare treatment and resource bounds before dispatch
Claim: A shared prepared-call seam can prove treatment and canonical enforcement without duplicate accounting or serializers.
Status: UNVERIFIED\
Evidence kind: design proposal; source inspection\
Evidence: [runtime](../../crates/hh-embed/src/runtime.rs), 1803–1945, discards retained view; [gateway](../../crates/hh-embed/src/gateway_model.rs), 819–870/1343–1386, rebuilds transcript/unreserved dispatch; [Account](../../crates/hh-budget/src/account.rs), 294–350/975–1025, checks primary keys/deduplicates totals. E2 F4 cites frozen derived-total rules.\
Retrieved: 2026-10-29T20:53:30Z.\
Implication for the spec: P6/P7 share admission; every matched dimension needs evidence.\
Outline delta: EXTENDS OL-09–OL-10, OL-15–OL-16, OL-20–OL-21.\
Limitations: Provider bounds/cancellation unproved.

Bind `evict_oldest`/`clear_tool_results` and exact parameters; unknown refuses. Persist retained/omitted IDs with real content/tool-pair/batch/required-group joins. Rebuild from durable selection; existing gateway renderer must not resurrect omitted transcript. Seal after selection/reservation and send those bytes. Capture intended/served strategy, activation/fallback/ineffective/impossible, next-dispatch joins and legal complete groups across restart/retry.

Matched caps remain `{tokens.input.total,tokens.output.total,model_calls,time.wall_ms}`. Order: retained view→serialized primary bounds→Account **whole-vector** reserve→sealed plan→durable request→deadline dispatch→durable usage/failure→charge/release→guard. Account must check projected **derived totals** on every ancestor including consumed/new/existing primary and slice holds. No primary `tokens.*.total` charges. Bound cache/provider input overhead, visible+reasoning output and actual role/call/transport attempts; unknown bounds/ambiguous use refuse enforced matching/unsafe further dispatch.

Subject root differs from funding slice. Reuse Account's scoped totals fold in experiment/results; propagated copies count once. Logical reverted totals differ from incurred/billed outlay; retain retry/failure/instrument cost. Expiry blocks new dispatch, drains/marks committed work unknown and records overruns. Socket timeout is not remote cancellation; over-reservation use remains charged. Test four caps, combined/concurrent ancestor holds, actual≠reserved, unknown use, role/cache drift, restart/expiry/dedup. Fixtures prove plumbing only.

### F4 — Operational proof uses the same native effect boundary
Claim: D3 requires actual permission transitions, confined sink observations and native probe outcomes through the task bridge.
Status: UNVERIFIED\
Evidence kind: design proposal; source inspection\
Evidence: [driver](../../crates/hh-control/src/driver.rs), 261–310/3610–3710, emits rows; [Dispatcher](../../crates/hh-env/src/dispatch.rs), 200–224/1750–1795, owns suspension/commit/Kp4; D3 F3/F4 and inspected recovery/permission/LocalExecutor sources bound claims.\
Retrieved: 2026-10-29T20:53:30Z; helper seam 2026-10-29T21:03:03Z.\
Implication for the spec: Port returns durable outcome, avoiding duplicate events.\
Outline delta: EXTENDS OL-05–OL-06, OL-12, OL-15, OL-20.\
Limitations: Shared backend suitability remains unresolved.

Dispatcher validates already-intended coordinate/scoped writer and owns authorization→settlement. `DurableEffectRef` suppresses duplicate driver rows; preserve fixture/host mode.

Candidate: [HelperExecutor](../../crates/hh-env/src/helper.rs), 370–410, via local_container/Podman. **P2 starts with feasibility/admission** of unchanged TB2 lifecycle and S4 confinement on this shared backend; availability/compatibility are unproved. New backend requires scope review. Fixed marker/surviving observer/denied sentinel/read-only probe and actual mounts/network are mandatory; scratch/LocalExecutor is insufficient.

Keep D3 eight cells/24 trials. Pending suspends before prepare; native offered-option/authority/idempotence and wakeup resume the same effect. Test restart/unoffered/conflicting/deny; `input_required` is distinct. Kp4 sink-zero cannot authorize settlement/retry: native unknown/probe persists; applied/not-applied/undeterminable requires admissible evidence. Retain stage counts/detector sensitivity. S4 stays outside D2 cohort.

### F5 — Extend native closure and correct R0's vocabulary
Claim: Owner-supplied closure can package grades/evidence without another authoritative receipt store.
Status: UNVERIFIED\
Evidence kind: design proposal; source inspection\
Evidence: [export](../../crates/hh-bundle/src/export.rs), 95–151, indexes references without blob bytes; [assembly](../../crates/hh-bundle/src/assemble.rs), 167–179, uses open/finished; [R0](../../crates/hh-embed/src/bundle_ops.rs), 1566–1590, instead compares terminal payload status. F1 supplies profile/procedure rules.\
Retrieved: 2026-10-29T20:53:30Z.\
Implication for the spec: Typed closure and lifecycle-correct R0.\
Outline delta: EXTENDS OL-07, OL-15–OL-18, OL-20–OL-21.\
Limitations: Recipient/rights checks unexecuted.

Owners supply typed payload/ordinary refs, causes/manifests/pages and submission/wire/grade/usage/selection/row/analysis roots. Bundle traverses with dedup/limits/unknown-schema refusal, no reverse dependency/hash regex. Reconcile all planned slots, including not-run/pending/failure/unknown. Verify recipient bytes after producer cleanup, without fallback; present/fetch is not access proof.

R0: finish event→`finished`, otherwise `open`; success/failure separate. Test successful/failed/open, changed lifecycle/hash/missing member, payload-only blobs, same-day attempts/regrades. Preserve separate replay/R1/recompute/R2/R3/independence; redaction supersedes with losses. Legacy receipts stay unchanged; build-note hash is not native head. Q-5/Q-6 govern access.

### F6 — Methods/parity require admission within existing owners
Claim: D1/D2 need a reviewed target/estimator decision before component claims, without a new statistical platform.
Status: UNVERIFIED\
Evidence kind: design proposal; source inspection\
Evidence: [scorecard](../../crates/hh-eval/src/scorecard.rs), 174–267, pools binary counts; [multiplicity](../../crates/hh-analysis/src/multiplicity.rs), 20–87, uses current reports. D1 F7/F8, D2 F6–F8 and inspected ADR-0277 D2/D6 bound numerical/schema changes.\
Retrieved: 2026-10-29T20:53:30Z.\
Implication for the spec: Methodology ADR/review gates claims.\
Outline delta: EXTENDS OL-09–OL-11, OL-16, OL-20–OL-21.\
Limitations: Numerical/parity validation unexecuted.

D2's **finite-manifest candidate remains conditional**: independent task/arm Beta(1,1) posteriors, paired weighted contrast, 8,000 draws/error checks require explicit ADR/review. No hierarchical population/correlated-replicate claim. Rate/cost task bootstrap targets task-mixture sensitivity, distinct from binary uncertainty; label each target, no joint verdict or new fitter/service/dependency.

Use existing eval numerics/compare/scorecard, analysis selection/persistence and lab/ontology schemas. Add D1 bounds/veto/first-subject rules, tails/unknowns/floors/actual draws/full frozen families. D2 rates require overlapping compaction→subject-stop joins, repeated deliveries/idempotency keys/denominators; retain old approximation names. Reacquisition intensity may exceed one, never probability-clamped.

Acceptance: numerical known answers/tolerances; heterogeneity/unequal n; one-task/missing-key/short-cell refusals; split-report families; D1 [−.4,.5] bounds counterexample; zero/absent detectors; retry/regrade counts; null/heterogeneous/missingness simulations with coverage/type-I/Monte Carlo uncertainty. Validation is not method approval.

**Q-3/Q-4:** D2 conservative CP/Bonferroni TOST cannot meet ±10pp at R=5 in perfect-agreement examples (T2/T6/T30 half-widths .584/.666/.758). This limits that construction/design, not parity inherently. Recommend bounded independent review of a more efficient established parity method before statistical tickets. Retain CP as fallback candidate, inspect admissible existing parity and quote additional work. Operator may explicitly limit claims to smoke/mechanisms while retaining the unmet central comparison. No automatic larger study, post-outcome margins or oracle-only parity. D2 60/300 subjects remain unapproved options; unset funding means zero calls.

### F7 — Eight packages, explicit contract and tier gates
Claim: C2's eight packages can carry the design without new scope or invented day estimates.
Status: UNVERIFIED\
Evidence kind: design proposal; source inspection\
Evidence: C2 F3 defines P1–P8. [Class table](../../crates/hh-ledger/src/classes.rs), 2202–2235, owns experiment metadata; [planes](../../crates/hh-ontology/src/planes.rs), `Kind/classify_home`, assigns one home; [codegen](../../crates/hh-codegen/src/main.rs), 1–17, owns outputs. Read facade/env/helper/ledger/embed manifests for C1/C4 feature boundaries.\
Retrieved: 2026-10-29T20:53:30Z.\
Implication for the spec: H1 routes amendments/decomposition.\
Outline delta: CONFIRMS OL-14; EXTENDS OL-17–OL-18, OL-20–OL-24.\
Limitations: No implementation authority/effort estimate.

ADRs: grade/settlement/launch; Dispatcher/prepared-call/derived admission; native closure/profile; methods/detectors. M1/R0 fixes follow existing rules. New event/payload/kind/domain requires owner codec/home/class-table durability/producer/audit/content-ref policy/tests; prefer invoked/verdict/bound extensions. Execution-settled needs admission.

No new/expanded dependency. Boundary changes use `hh-embed-schema`→`hh-codegen`/all outputs and negotiation/refusal tests; Rust adoption promises no RPC. Legacy semantics remain; unsupported features refuse. C0 adds no C4 dependency; C1 helper/suspend/recovery stays removable. Check resolved features/restore helper; no new tier.

| Package/order | Dependencies / bounded output | Exit acceptance |
|---|---|---|
| P1 subject/grade | Contracts; adoption/selection/pending/revisions | Legacy/F2 crash tests; compose with P2/P7. |
| P2 task/effect/oracle | P1 interfaces; one task/adapter/backend/seal | Correct/wrong/broken, sink/permissions/isolation. |
| P7 four caps | P1 interfaces; Account/gateway/control/clock | Four caps/unknown use/recovery; missing enforcement refuses. |
| P3 receipt/R0 | P1/P2/P7 facts; F1 profile | Six R0 controls; cleanup/loss/attempts/grades. |
| P4 recipe/lesson | P1–P3/P7; one recipe/fixture and needed request repairs | Scheduling→projection→analysis, trace/semantic drift checks. |
| P5 operations | P1–P3/P7; same backend | D3 eight cells/24 trials, observer/stage counts. |
| P6 treatment | P1/P2/P7 prepared call; two native policies | Wire/groups/fallback/restart/no resurrection. |
| P8 study | P3/P6/P7 + method/Q-3/Q-4 | Detectors/numerics/simulations/parity/frozen quote. |

Graph: decisions→P1 interfaces→{P2,P7}; {P1,P2,P7}→{P3,P6}; P3→P4; {P2,P3,P7}→P5; {P3,P6,P7,method/parity decision}→P8. Interfaces precede implementation; acceptance joins are not crate cycles. P7 serves S1/S4 too. Future checks: owner regressions/scoped tests/format, schema drift, feature removability, living docs; report counts.

Keep S1 graded task/receipt, S2 fixture lesson, S3 central gated comparison, S4 operations, S5 fixed report/verifyfixed plus optional one plugin, S6 optional one consumer/direction. **All 22 C1 family dispositions/nine later groups remain.** Optional invocation/tamper/no-helper/consumer-loss work follows core readiness; no third arm/platform. Blocked study remains unmet; fixtures cannot replace it. Null/negative/inconclusive research is valid; adoption separate.

## Answer and completion assessment

Design complete; ADR/ratification and named feasibility gates remain. No runtime, scientific or recipient success claimed.

## Open questions

- **Q-3, operator/method reviewer → D2/E3/F3/H2:** TB2/oracle/runner/backend/provider pins and method/parity disposition; new backend/public API expands scope.
- **Q-4, operator → D2/D3/E3/F3/H2:** margins/precision/parity route/full quote; no automatic larger study or spend.
- **Q-5/Q-6, operator → F1/F3/G2/G3:** rights/access/retention/replication/distribution; no outreach/publication inferred.
- **H1/O1 → P1–P8/H2/O2/O3:** route ADRs/compatibility; recommendation is not approval.

## Spec requirements emitted

Candidates; H1/O1 deduplicate preserving existing identities.

| ID | Candidate | Findings / outline | Acceptance |
|---|---|---|---|
| REQ-E-201 | One subject/recoverable launch/owner chain. | F1; OL-05/20–21 | Duplicate/stale/orphan tests. |
| REQ-E-202 | Pending admission everywhere; legacy preserved. | F2; OL-05/10/15 | No pending scored/compare/complete. |
| REQ-E-203 | Created→finished M1; seal/postterminal grade. | F2; OL-10/15–16 | Delayed adoption/restart/long oracle. |
| REQ-E-204 | Shared selection/immutable revisions. | F2; OL-10–11/16 | Crash/regrade; no recharge/new n. |
| REQ-E-205 | Native effect/permission/sink/probe. | F4; OL-05/12/20 | O1–O8/tier refusal. |
| REQ-E-206 | Retained treatment→actual wire. | F3; OL-09–10/15 | Policy/group/restart/fallback. |
| REQ-E-207 | Derived-cap admission/scoped actual totals. | F3; OL-10/20–21 | Four caps/holds/unknown/deadline/dedup. |
| REQ-E-208 | Typed closure/R0/all-planned reconciliation. | F5; OL-07/15–18 | Clean-recipient/loss/attempt controls. |
| REQ-E-209 | Reviewed methods/targets/detectors/parity. | F6; OL-09–11/16/21 | Numerics/simulations/admission refusal. |
| REQ-E-210 | Schema/tier/ADR/package discipline. | F7; OL-14/17–18/20–24 | Compatibility/drift/features/coverage. |

## Handoff and verification

E2 preterminal preference is resolved without rewriting it; D1/D2/F1 reconcile. F2/F3/G2 consume these boundaries; H1 owns amendments, H2 readiness. No row added; Q-3 may authorize the proposed methods follow-up. Portfolio/state remain coordinator-owned.

Deferrals-first empty; planning validation before/after: 0 violations/0 warnings (locale fallback). Scoped checks passed: 7 findings × 8 fields, 10 unique allocated requirements, 33 relative links, four closing sections and table widths. Full added-file diff reviewed; whitespace check had no diagnostics (no-index exit 1 denotes added content). **0 Rust builds/tests, runtime/task/oracle/provider executions, downloads, installs, commits or external mutations.** Only this note written; implementation acceptance is proposed.
