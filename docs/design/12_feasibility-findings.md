# 12_feasibility-findings — Owning-layer changes for a restricted native runner

Stream: E — Execution and engineering\
Researched: 2026-10-29 UTC\
Author: /root/e2_feasibility\
Ledger rows covered: E2\
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; approved uncommitted planning package and accepted A1–A3/C1/E1/C2 inputs.\
Confidence overall: high for source boundaries; medium for design; execution unproved.

**Decision:** retain the restricted native in-process runner, with owning changes for subject/grade adoption, retained context→wire, Account metering and native closure. Pre-terminal first grading is provisionally preferred; frozen timing may require an ADR or the post-terminal alternative. Existing missing bindings justify a source-backed design stop for all three envelopes. **0 runtime probes/builds.** No task, verifier, provider or efficacy result is claimed.

## Scope and decision record

Read [brief](../brief.md), [assignment/Q-2](../research-ledger.md), [conventions](../research/CONVENTIONS.md), [trace](../OUTLINE_TRACE.md), root/docs instructions and `synthesize-spec` run mode (skills `e981ec57d487a2c41d416672483b5a4df15b1233`). [C2 F5](08_feasibility-and-scope.md#f5--offline-e2-can-resolve-architecture-without-funding-a-study) approval: “looks good to me. proceed”, recorded 2026-10-29T19:37:48Z. Accepted inputs: [E1](07_execution-seams.md), [C1](06_demo-and-study-portfolio.md), [A1](../research/01_capability-inventory.md), [A2](../research/02_evidence-audit.md), [A3](../research/03_newcomer-journey.md). Inspection checkpoint **2026-10-29T19:52:03Z** supplies retrieval stamps, not execution times. Offline only.

Interfaces below are **proposals**, not callable APIs. Preserve K-2/CC1, `idp/1`, fenced durable writes and owning projections. Dependency expansion requires ADR; schema/event changes use owner registration/generation and the amendment protocol.

| Envelope | Selected design / reusable owner | Remaining gate |
|---|---|---|
| A | Restricted embed adoption + one Dispatcher effect owner; provisional pre-terminal first grade, later overlays | New owning interfaces; reconcile frozen timing semantics (F1/F2/F4). |
| B | Native policy→durable retained view→gateway wire; real Account allocation/reservation/usage/deadline | Effective treatment, all four caps and detector denominators (F3–F5). |
| C | Immutable native attempt/grade closure; R0 lifecycle fix; common observable operational sink | Recipient closure and actual Kp4/control/deny/pause/exhaustion tests (F6/F7). |

## Material findings

### F1 — Adoption and effect dispatch belong inside the existing runtime owner
Claim: A restricted adoption entry must reuse embed's private runtime machinery and give each real effect exactly one lifecycle owner.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: [launch](../../crates/hh-experiment/src/engine.rs), 337–350/1393–1498, returns subject/writer; private [arm_driver](../../crates/hh-embed/src/open.rs), 2518–2685, and [drive](../../crates/hh-embed/src/service.rs), 1568–1715, own adapters. [EmbedGate](../../crates/hh-embed/src/runtime.rs), 313–421, waits for host reports; [Dispatcher](../../crates/hh-env/src/dispatch.rs), 940–956/1750–1788, owns actual effects. [embed dependencies](../../crates/hh-embed/Cargo.toml) admit control/env/budget/results, not bench.\
Retrieved: 2026-10-29T19:52:03Z.\
Implication for the spec: Add adoption and effect binding in their existing owners; do not create a second session/run or copy adapters into a standalone example.\
Outline delta: EXTENDS OL-05–OL-06, OL-08, OL-20–OL-21.\
Limitations: Compilation, resume and task-container usability remain untested.

Proposed `adopt_subject` consumes the launched binding; validates subject/plan/attempt, sealed configuration, registry/environment identity and writer generation; then arms the existing driver. Duplicate, finished, mismatched and stale adoption refuses. Resume reconstructs durable state under a valid fence. Failure after launch records an owned refusal/failure and releases/settles resources, without opening a replacement run. This versioned Rust recipe does not imply ordinary CLI/SDK/RPC support.

Select an injected **Dispatcher-backed effect bridge** with temporary access to the same store/writer seam: `hh-env` authorizes/prepares/commits/executes/captures once and returns the durable outcome. Extend the owning port/context; do not first commit through `EmbedGate` then dispatch the full chain again. Existing host-declared upcall/report mode is a separate option only with a real executor bound to its committed coordinate. Tool bytes must resolve to actual execution; a success report is insufficient. Failure/unknown/refusal remains typed.

The outer runner composes [facade owners](../../crates/hharness/Cargo.toml), including benchmark materialization/submission/grading, and injects its instrument adapter. No reversed bench→kernel ownership or undeclared embed→bench dependency. Original-runner environment/oracle reuse remains a later integration with unresolved task/registry/image/license/network pins and **unapproved execution**. E1's `cancel-async-tasks` is only a candidate; nothing was fetched.

### F2 — Pre-terminal grading is preferred pending timing reconciliation
Claim: Prefer pre-terminal first grading with explicit regrade supersession, conditional on reconciling frozen timing semantics.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: [Driver](../../crates/hh-control/src/driver.rs), 515–525/4179–4211/4804–5025, owns decision verification/finish; [terminal payload](../../crates/hh-control/src/events.rs), 255–273, uses stop reason alone. [append](../../crates/hh-ledger/src/store.rs), 1404–1436, refuses finished runs. [settle](../../crates/hh-experiment/src/engine.rs), 1555–1620/1652–1656/3032–3046, uses terminal facts and requests regrade, not rerun, on oracle failure. [overlays](../../crates/hh-results/src/projection.rs), 202–292, cannot clear original oracle failure with success. [grade](../../crates/hh-bench/src/grade.rs), 154–225, parses outputs; [ontology](../../crates/hh-ontology/src/eval.rs), 2203–2214, prioritizes oracle failure.\
Retrieved: 2026-10-29T19:52:03Z.\
Implication for the spec: Implement first-grade finalization and a revision-aware regrade fold; preserve original lifecycle, outcome/grade, veto and scheduler history separately.\
Outline delta: EXTENDS OL-05–OL-06, OL-08–OL-11, OL-15–OL-16, OL-20.\
Limitations: Unexecuted new hook; current VerifyPort alone guarantees neither finalization nor oracle-failure/isolation facts.

Ordering: stop participant actions → drain effects/record unknowns → freeze content-addressed submission → record participant-quiescence boundary → pinned executable oracle under **separate instrument budget and declared shared/separate environment semantics** → retain stdout/stderr/exit/reward/access evidence → durable grade/metric/veto → driver terminal with grade references and `oracle_failed` → settle → ResultsStore → persisted analysis. Durable finalization survives restart without rerunning the participant or exposing hidden feedback. Record a grade attempt before invocation; resume uses its result or adds an immutable capped instrument retry. Owners produce their own records.

The terminal instrument phase must not feed hidden oracle output into the completion gate's correction loop. The port returns typed evidence/failure references; the driver seals the terminal. Grader crash/timeout/empty or nonfinite output/refusal sets oracle failure through ontology; wrong applied output is a scored zero. Preserve stop reason even when oracle failure takes precedence. Budget-exhausted attempts remain represented and gradable; other failure/exclusion rules follow D1.

| Ordering | Consequence / decision |
|---|---|
| Pre-terminal initial grade | Provisionally preferred: new hook, existing settle sees oracle failure/regrade. Frozen created→finished timing still includes verification; E3 must resolve F4's contract branch. |
| Post-terminal initial overlay | Requires grade-pending, grade-aware settlement/retry authority. Compare this against ADR-backed timing clarification if verification must be outside the evaluation interval. |
| Later regrades | Reuse immutable overlays; add successful outcome supersession. Preserve old failure and settlement; no automatic subject retry. |

E3 must expose the current regrade disposition linked to the final plan, preserving the old settlement. Wrong-answer reattempts differ from grader retries. Required tests: correct/wrong/broken oracle; finalization crash/resume; duplicate settle; regrade failure→success→failure; budget-exhausted grading; veto preservation; one subject denominator despite multiple grades.

### F3 — Treatment must select retained content before the gateway plan is sealed
Claim: Bind native policies and consume a durable retained-view projection in the gateway, rather than preserving an unused compaction hash.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: [runtime](../../crates/hh-embed/src/runtime.rs), 1803–1945/470–527/615–680: empty ladder→EvictOldest, unused returned view, empty text/kind/pair/artefact metadata. [GatewayModel](../../crates/hh-embed/src/gateway_model.rs), 653–740/819–870/1343–1386, ignores `_request` for full transcript. [ClearToolResults](../../crates/hh-context/src/compact.rs), 1071–1145, needs capability/completion joins and group safety. [ModelPort](../../crates/hh-control/src/driver.rs), 47–90/2738–2855, already receives assembled requests.\
Retrieved: 2026-10-29T19:52:03Z.\
Implication for the spec: Correct the existing embed/context/gateway composition; keep one serializer and one canonical context/identity scheme.\
Outline delta: EXTENDS OL-09–OL-10, OL-15, OL-20.\
Limitations: No policy invocation/wire capture; treatment effectiveness unproved.

Resolve the sealed slot to the exact native policy/parameters at arm time. `clear_tool_results` gets admitted capabilities, age rule and real invocation→completion-sequence joins. Unknown/unbound policy refuses before dispatch. Declare any fallback ladder and record intended/served strategy; fallback is not assigned-treatment execution. Freeze common compaction-capable control, trigger and builder; retain untriggered outcomes.

Reuse candidate/plan structures and proposal validation. Populate source-event/item/artefact/tool links, `paired_with` and `batch_id`; preserve required items. Persist validated retained/omission facts, then reconstruct the view from the durable prefix before reassembly. The plan stash is only a cache. Transcript reconstruction must not resurrect removed items; reacquisition needs new admission evidence.

`GatewayModel::build_plan` consumes that retained representation; existing family renderers own wire messages/IDs. Preserve whole multi-call/result groups, required siblings and legal cuts or valid omissions; reject orphan pairs/duplicate IDs. Seal/hash the plan after selection/reservation and send exactly those serialized bytes. Capturing-transport acceptance must show intended changes under diagnostic pressure, explicit unchanged/fallback/impossible states, and valid groups across retry/restart/reassembly. This proves plumbing only.

### F4 — Canonical enforcement needs a real subject Account and a prepare/reserve/dispatch boundary
Claim: The matched-cap path needs actual Account allocation, reservation, usage reconciliation and real deadline enforcement before it can advertise full enforcement.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: [launch](../../crates/hh-experiment/src/engine.rs), 1197–1204/1304–1317/1393–1498, binds experiment slice without subject Account allocation. [Account](../../crates/hh-budget/src/account.rs), 140–145/192–232/294–407/441–477/581–615/975–1015, owns allocation/reservation/charge/exhaustion/deduplicated totals. [budget_dimensions](../../crates/hh-embed/src/open.rs), 3033–3062, leaves tokens fixed; [guards](../../crates/hh-control/src/driver.rs), 1467–1490, use logical time. Engine `consumed_vector` (3049–3070) and Results (118–124) sum propagation rows, unlike Account's leaf-most source/dimension fold.\
Retrieved: 2026-10-29T19:52:03Z.\
Implication for the spec: Make account binding and metering capability a launch prerequisite; reuse Account's deduplication with audited semantic filters.\
Outline delta: EXTENDS OL-09–OL-11, OL-15, OL-20–OL-21.\
Limitations: No measured overruns; provider bounds/observability/cancellation remain admission gates.

Keep funding slice and subject root distinct and durably linked. Extend owning launch/allocation to produce a real subject root with arm `eval_budget`, bind it before arming and return both refs. Never use an experiment-only slice ID against an empty subject Account or patch the manifest externally. One attributed root initially; verification has a separate instrument budget. Settlement transfers deduplicated actual use to the funding slice once. Partial launch recovery releases/settles each reservation once.

Sequence: retained request → gateway prepares bounds/role/model → driver metering port reserves via Account/store seam → gateway seals plan with reservation → requested event durable → deadline-bound dispatch → native I/O/usage/terminal evidence durable → actual charge/justified release → guard/exhaustion. Use typed prepared-call data, not arbitrary model-authored rows. Read remaining capacity from Account. Reuse its deduplication rules in experiment/results projections; hierarchical propagation must not inflate usage. Audit semantic filters: `Account::totals` excludes reverted-effect charges, so it is not automatically all-attempt physical cost; retain actual incurred/billed use separately.

Token totals are **derived**, never primary charges ([frozen §8.2](../../spec/sections/08-cross-cutting-contracts.md), line 167). Reserve/charge canonical input uncached/cache-read/cache-write and output visible/reasoning components; use existing [MeteringFormula/ResourceVector](../../crates/hh-budget/src/quantity.rs), 52–66/146–156, and [BudgetTree](../../crates/hh-budget/src/tree.rs), 550–593, for derived ceilings. Account reserve currently iterates primary keys: extend its owning pre-dispatch admission to check projected derived levels including outstanding holds; a post-charge check is insufficient. Never invent `tokens.*.total` consumed rows.

| Matched dimension | Required enforcement / failure behavior |
|---|---|
| `tokens.input.total` | Bound actual serialized system/tools/context/provider overhead through primary components; reconcile usage. Unsupported bounds/missing use remain unknown/partial. |
| `tokens.output.total` | Bound visible plus reasoning output via canonical components; visible-only limits cannot prove total enforcement. |
| `model_calls` | Reserve each actual role call/attempt/retry; specify rejected preparation/cache treatment. Retain call and attempt coordinates, including failures. |
| `time.wall_ms` | Real elapsed deadlines cover calls/effects/retries/approval waits under the admitted interval; recovery cannot reset the cap. Record quiescence/verifier boundaries separately and preserve frozen M1 (below). |

Record over-reservation charges honestly. Failed/ambiguous dispatch with missing usage retains conservative exposure and blocks further calls when safe capacity is unknown. Deadline expiry prevents new dispatch and requests bounded cancellation; committed effects drain or become unknown before exhaustion/closure. Socket timeout proves neither remote cancellation nor exact cost. Any dimension lacking a defensible enforced bound refuses comparable admission; no silent substitution.

Timing is an explicit acceptance gate: [frozen M1](../../spec/sections/05h-measurement-evolution.md), line 72, defines run `wall_ms` as `lifecycle.run.created`→`finished`. Pre-terminal grading therefore extends that span. E3/D1 must reconcile the matched `time.wall_ms` rule, retain M1 and separately admitted instrument/participant spans, or propose an ADR. E3 must compare ADR-backed timing clarification against grade-pending post-terminal settlement if verification must be outside the evaluation interval. Never backdate finish or silently subtract verification. This is an unresolved design branch, not another probe.

Record requested/served role/model/provider snapshots, routing, seed and cache; mismatches are drift. Subject summarization/validation/retries remain subject-charged; verification is instrument. Test every cap, usage≠reservation, unknown/failed usage, idempotence/hierarchy, role drift, in-flight exhaustion and delayed/restarted deadlines.

### F5 — Existing mechanism counts need protocol-owned denominator and coverage contracts
Claim: Reuse native count folds but add explicit rate/detector semantics before treating them as the canonical mechanism outcomes.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: [context_metrics](../../crates/hh-eval/src/context_metrics.rs), 145–210, defines reacquisition by artefact IDs and repeated exact action arguments separated by compaction; [catalogue](../../crates/hh-eval/src/catalogue.rs), 454–482, registers counts and a recall probe rate. Runtime candidates in F3 lack ordinary pairing/artefact IDs. The [evaluation contract](../../spec/sections/10-evaluation-reproducibility-plan.md), §§10.1–10.2/10.5.1, fixes the scorecard, matching and compaction comparison.\
Retrieved: 2026-10-29T19:52:03Z.\
Implication for the spec: D1/D2 own the observation unit, numerator, eligible opportunities, exposure window, missingness and task aggregation; E3 supplies those raw events and registry declarations.\
Outline delta: EXTENDS OL-09–OL-11, OL-15–OL-16, OL-20.\
Limitations: Repeats approximate forgetting; D1/D2 choose denominators/tasks.

Record trigger opportunities, intended/served policy, occupancy, changed/retained IDs, wire hash, latency, attribution and coverage. Distinguish `untriggered`, `ineffective`, `fallback`, `impossible`, `unavailable`, `no_detector`. No invented denominator or missing-as-zero. Known traces must include legitimate repeats and absent artefact IDs. E3 reconciles accepted D1 methods before implementation. Preserve TB2/native arms/fixed roles/four matched dimensions/`same_snapshot`/`cold_start`/zero search/fault-free comparison/replication floor unchanged.

### F6 — Native artifact closure and R0 status repair are concrete owner changes
Claim: Extend native closure for blobs/grades and compare R0 in its declared lifecycle vocabulary.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: [export](../../crates/hh-bundle/src/export.rs), 95–151, indexes `refs[]` blobs without adding bytes; [assemble](../../crates/hh-bundle/src/assemble.rs), 167–179/442–448, emits `open/finished` and empty results. [SubjectSection](../../crates/hh-bundle/src/manifest.rs), 570–585, declares that vocabulary; [R0](../../crates/hh-embed/src/bundle_ops.rs), 1566–1590, compares terminal status instead. [live-verify](../../scripts/live-verify.sh), 1513–1539/1622–1626, hashes a build note and overwrites date/leg paths. A3 F6/A2 F2–F3 record these execution/closure limits.\
Retrieved: 2026-10-29T19:52:03Z.\
Implication for the spec: F1/E3 must declare required native members, closure traversal, access and completeness; fix R0 in its owner before advertising reproduction.\
Outline delta: EXTENDS OL-07, OL-15–OL-18, OL-20.\
Limitations: No export or R0 rerun; producer `present` does not prove recipient access.

Use immutable launched run/attempt identity and distinct grade-attempt IDs; never overwrite same-day attempts. Index native heads/watermarks/blobs, configuration/task/image/oracle pins, submission/verifier/access evidence, budget/roles/grades/result versions and analysis. It catalogues existing authorities, using their codecs/content addresses and actual native head; a build-note snapshot is a separate reference.

Extend `hh-bundle` closure inputs/members for required native blobs, instrument/grade runs, submission and ResultsStore/scoring/declarations, with typed unavailable/redacted status. Traverse typed payload refs as well as `refs[]`; missing model-response bytes must not escape closure. F1 owns schema/profile and Q-5 access policy; no shadow format. Test two immutable attempts/multiple grades after producer cleanup: resolve required bytes, verify hashes/causes/watermarks, reject missing/altered members at promised levels. Redaction creates a superseding export with honest completeness.

R0 compares **lifecycle state** using assembly's owner rule: durable finish event→`finished`, otherwise `open`. Keep terminal status/stop/outcome separate and preserve chain/hash checks. Test real export→clean-store R0 for successful/failed/open runs plus tampered state/hash; report semantic verdict, exit and independence separately. R1–R3, recomputation, replay and fresh execution remain distinct.

### F7 — Kp4 requires an honest unknown/probe outcome, not optimistic replay
Claim: Operational cases require independently observed sink effects alongside native durable recovery evidence.
Status: UNVERIFIED\
Evidence kind: design proposal grounded in source inspection\
Evidence: [KillPoint](../../crates/hh-ledger/src/fault.rs), 1–49, defines Kp4 after durable commit before executor entry; [Dispatcher](../../crates/hh-env/src/dispatch.rs), 940–956, 1750–1788 and 1808–1885, supplies deny/kill/protocol-resume seams. [recovery table](../../crates/hh-ledger/src/recovery.rs), 1–25, maps committed/no-terminal to `unknown{worker_lost}` plus probe. [kill-point acceptance source](../../crates/hh-env/tests/kill_point_battery.rs), 1–29 and 880–910, asserts that mapping and explains its external counter is an `AtomicU64`. [LocalExecutor](../../crates/hh-env/src/local.rs), 1–65, advertises isolation `none`; Account exhaustion in F4 waits for committed effects.\
Retrieved: 2026-10-29T19:52:03Z.\
Implication for the spec: D3/E3 bind one observable confined sink and state exactly which recovery, permission and budget property each case tests.\
Outline delta: EXTENDS OL-12, OL-15–OL-16, OL-20.\
Limitations: Test source only; no containment certification or Kp5/Kp6 execution guarantee.

| Case | Actual observation / native expectation |
|---|---|
| Kp4/control | Fixed mutating effect; control changes confined sink once. Kp4 leaves durable commit and unchanged sink, then restores to unknown/probe without blind rerun. Observe sink independently; follow declared recovery policy. |
| Deny | Monitor denies before prepare; no prepared/committed effect or sink change. A scripted uncalled refusal is insufficient. |
| Permission pause/respond/resume | Pending permission blocks prepare and sink change; existing permission owner handles the answer on the same effect. Invalid/denied answers refuse. Protocol `input_required`/`resume_paused` is distinct: same effect, next attempt, opaque state; never substitute it for approval. |
| Exhaustion | Real Account bound blocks further dispatch; observe sink and charges. Already committed effects drain or become unknown before stop. |

Use a fixed confined capability and inspectable file/counter sink; declare actual isolation/probe/dedup. Scratch paths are no sandbox. Reuse one backend or escalate scope. Record `tier-c1`, tier-off refusals and stale-helper/host-timeout prerequisites; no installs are authorized.

## Answer and completion assessment

All three envelopes have owner/interface decisions and tests, with timing/ordering explicitly unresolved for E3. Execution/enforcement/scientific effects remain unproved; portfolio obligations are unchanged.

| Envelope | Authorized maximum | Actual investigation / stop |
|---|---|---|
| E2-A | 3 h wall; 6 core-h; 2 GiB new disk; 12 probes | Source/design only; **0 probes, 0 builds**. Private adoption and missing finalization/effect binding already settle the required owner changes. |
| E2-B | 4 h wall; 8 core-h; 4 GiB; 20 probes | Source/design only; **0 probes, 0 builds**. Empty policy binding, discarded retained view and absent real Account path cannot be repaired in scratch. |
| E2-C | 3 h wall; 6 core-h; 2 GiB; 12 probes | Source/design only; **0 probes, 0 builds**. Closure/R0 gaps are established; no new ambiguity justifies repeating A3's failure. |

Aggregate maxima: 10 h wall/20 core-h/8 GiB/44 probes; one active execution, ≤2 cores/8 GiB RAM, no allowance transfer. **Actual: zero runtime invocations/builds/artifacts; $0 additional task-provider/cloud-runtime spend.** Codex planning/local costs are excluded. Source/design resource use was not instrumented. Only this repository note is written; validation has temporary diagnostics. No network/install/dependency, source/central/build-state change, commit or external write. Toolchain/features/cached-build readiness are unverified. Runtime checker/wire/cap/operational/recipient cases: **skipped(missing owning bindings; implementation outside tranche)**. Later task/provider authorization remains zero.

## Open questions

| Question | Owner / affected units | Boundary |
|---|---|---|
| Exact adoption/finalization/metering type shapes, atomic budget-binding sequence and replay/resume acceptance | E3; D1–D3/F1/F2 | Routine design within scope; implementation later. New dependencies/schema/contract amendments need their normal ADR process. |
| Rate denominators, canonical wall interval versus finalization/instrument spans, reverted-charge versus incurred-cost accounting, grading/exclusions | D1/D2; E3 | Reconcile before implementation. |
| Original-runner/task/registry/image/oracle/model pins, unchanged lifecycle compatibility and licenses | D2/E3/operator; F1/F3 | Q-3/Q-5 and separately authorized diagnostics; E1 candidate remains unselected. |
| Actual precision/activation/variance, total-token enforcement capabilities, execution caps and distribution access | D2/F1/F3/operator | Q-4/Q-5; no current scientific result or publication approval. |

## Spec requirements emitted

Candidates refine E1; H1/O1 deduplicate without changing existing IDs. Proposed tests are unexecuted.

| ID | Requirement / rationale | Findings / outline | Proposed acceptance |
|---|---|---|---|
| REQ-E-101 | Adopt the launched subject in the runtime owner through one valid writer and one effect lifecycle. | F1; OL-05–OL-06, OL-20 | Duplicate/stale/mismatched adoption refuses; real dispatcher effect and output join to that subject; no second run/commit. |
| REQ-E-102 | Freeze submission; resolve grade/terminal/settlement ordering with frozen timing before implementation. | F2; OL-05, OL-08, OL-15 | Correct/wrong/oracle-failure controls, bounded verifier retry, crash/resume and hidden-feedback separation. |
| REQ-E-103 | Make regrade outcome revisions coherent with immutable original settlement and attempt denominators. | F2; OL-10–OL-11, OL-16 | Failure→success→failure overlays preserve history, one subject count and no automatic participant rerun. |
| REQ-E-104 | Bind declared native policy and durable retained selection to the exact serialized provider plan. | F3; OL-09–OL-10, OL-20 | Captured bytes prove intended changes; grouped/required items, restart, fallback and unknown-policy controls. |
| REQ-E-105 | Allocate a real subject Account, link its funding slice, and reserve/reconcile every matched dimension and role. | F4; OL-09–OL-10, OL-15 | Four caps via primary-token charges/derived ceilings, failed usage, role drift and no deadline reset. |
| REQ-E-106 | Reuse Account deduplication with audited cost semantics; preserve incurred costs, overruns and unknown exposure. | F4; OL-10, OL-15, OL-20 | Hierarchical charges count once; retry/recovery idempotence; no hidden over-reservation or optimistic release. |
| REQ-E-107 | Declare mechanism detectors, eligible denominators and missingness before translating native counts to rates. | F5; OL-09–OL-11, OL-15 | Known traces validate each numerator/exposure; no-trigger/no-detector/ineffective cases remain distinct. |
| REQ-E-108 | Retain immutable subject and grade attempts with explicit native/blob/result closure. | F6; OL-07, OL-15–OL-18 | Two attempts survive producer cleanup; missing/corrupt/restricted members fail or reduce the promised level. |
| REQ-E-109 | Compare R0 lifecycle state using the bundle's open/finished contract, independently of task outcome. | F6; OL-07, OL-17, OL-20 | Real clean-store export/reproduce plus successful/failed/open/tampered controls; inspect semantic verdict. |
| REQ-E-110 | Scope operational proof to the actual common effect bridge and observable sink under native recovery rules. | F7; OL-12, OL-15, OL-20 | Kp4/control, pre-prepare deny, permission response/resume and real exhaustion; honest unknown and tier-off refusal. |

## Handoff and verification

E3 can proceed. D1/D2 own methods/admission; D3 operations; F1 closure; F2 examples; F3 replication. No new row. Escalate public-API/backend/contract/resource expansion, not routine slicing. Later diagnostics need pins/enforceable ceilings; accepted notes remain unchanged.

Verification: deferrals-first 0/empty; planning pre/post checks: zero violations/warnings (locale fallback). Structural checks passed: 7×8 finding fields, 10 IDs, 49 links/anchors, four closing sections; added-file diff reviewed/check clean. Coordinator owns final docs checks. **0 Rust/runtime tests, builds, oracles or provider calls.**
