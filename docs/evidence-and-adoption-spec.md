# HarnessHarness — Evidence and adoption supplemental specification

- **Status:** canonical · v0.2 ratified by the operator at 2026-10-30T02:05:32Z; Q-7 resolved. Frozen supplemental specification; original behavioral authority and later admission/execution gates remain unchanged.
- **Ratification provenance:** User message in the current Codex chat: “Looks good, proceed”. Approved reviewed draft SHA-256 `914ea2fe1b9dbbd5b37ab3a6a35987c72f40f9f159ee52fd0520b982c09f648c` and the five decisions in the [ratification guide](spec-ratification-review.md), pre-approval SHA-256 `060966bd9d0590e3e089e00adea6c6f503677d58c447e6119659754d07ede982`. [Independent rereview](design/22_independent-spec-review.md#delta-rereview) SHA-256 `f68a284d4bc6216cb1de243d59dd9e8c553f3da69dfebf7459b46a0e49efc5b0` closed both findings. This ratification changes status/decision/coverage metadata only; the reviewed requirements and acceptance criteria are unchanged. The [decomposition handoff](decomposition-prompt.md) records the resulting frozen digest.
- **v0.1 → v0.2 delta:** Resolve the independent review’s one-direction interchange/round-trip conflict and participant-acceptance routing gaps. Clarify method-specific constant-data controls, the offline numerical-validation envelope and O8’s deliberately violated invariant. The 180 source requirements and 22 accepted inputs are unchanged; Appendix E records dispositions and the independent rereview route.
- **Synthesized from:** [founding brief](brief.md), [research ledger and operator decisions](research-ledger.md), [outline trace](OUTLINE_TRACE.md), and all 22 accepted notes and hashes in [Appendix C](#appendix-c). Baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; workflow revision: `e981ec57d487a2c41d416672483b5a4df15b1233`.
- **Supersedes:** Within this supplemental design, conflicting proposals in the accepted notes, only as ruled below. It does not supersede the frozen behavioral contract, historical evidence, operator decisions or governance. Proposed amendments remain unadmitted until the ADR/manifest protocol is completed.

<a id="s0"></a>
## 0. Authority, reading convention and decisions

### 0.1 Adopted contracts and scope of authority

The [canonical specification](../spec/CANONICAL_SPEC.md) remains the behavioral authority. Its identity/provenance, durable ownership, kernel/client boundary, tier/refusal and amendment constraints are normative here, particularly [runtime and recovery](../spec/sections/05a-runtime-durability.md), [measurement/evolution](../spec/sections/05h-measurement-evolution.md), [Lab](../spec/sections/06-laboratory.md), [cross-cutting contracts](../spec/sections/08-cross-cutting-contracts.md), and [evaluation/reproducibility plan](../spec/sections/10-evaluation-reproducibility-plan.md). The [governance](../GOVERNANCE.md), [contribution policy](../CONTRIBUTING.md), [maintenance/evidence rules](../MAINTAINING.md), root/docs AGENTS, ADR-0362 CI rules and ADR-0363 live-evidence rules also apply. The amendment register in [§9](#s9) identifies changes proposed here; until admission, affected new behavior or inference is unavailable. Ratifying a supplement cannot silently amend an imported contract.

“Must” and imperative acceptance clauses express future obligations, not claims of completed work. Source findings are explicitly labelled in §1. Design selection does not prove runtime feasibility, task success, independence, numerical coverage, affordability, rights clearance, usefulness or adoption. All program task/provider/cloud execution funding currently equals **$0**. No installation, container pull, recruitment, external write, hosting, publication, implementation, new build round or merge is authorized by this document.

The body supplies the governing semantics; Appendix A preserves every original candidate verbatim and assigns its disposition and acceptance hook. A source row's old alternatives, open-question wording or conflicting method are historical provenance, interpreted through the rulings here. Appendix A's retained source predicates and acceptance predicates supplement the body unless an explicit ruling replaces them. Partial overlap is a cross-reference, not deletion. Later-scoped requirements obligate a disposition/prerequisite, not execution of every later study.

### 0.2 Requirement identifiers and acceptance layers

```text
req_id_pattern: REQ-[A-H]-[0-9]+
```

IDs are append-only: retain literal spelling and allocated hundreds; never renumber/reuse, translate into `R-2.*`, or allocate duplicate owners. Families are A audit, B audience/ecosystem, C portfolio/feasibility, D methodology/protocols, E execution, F receipts/examples/replication, G reader/contribution/adoption, H handoff. Unused numbers remain reserved, including H2's unused H-family 101–199 allocation; H2 emits none. The 180-source-candidate set is closed for this ratified supplement. A newly discovered obligation requires explicit source/disposition review, not a silent new ID.

The registry distinguishes **adopted**, **merged-with(target)**, **superseded-by(target)** and **deferred(reason, owner, trigger, withheld claim)**. This supplement has 167 adopted, 12 merged and one superseded candidate; zero silently deferred/deleted. Every adopted target receives one eventual primary ticket at decomposition; merged aliases retain all predicates at that target. Consumers reference the owning requirement. Exact ticket numbering is unallocated.

Acceptance hooks use `AC-<section>-<number>` and these layers: **D** document/design/trace review; **E** engineering or executable procedure; **R** admitted research execution/reporting; **P** actual participant/publication/adoption evidence. An E pass is not an R or P pass. A sound null, negative or inconclusive study can meet R acceptance; an unexecuted or unadmitted study is unmet, not a null result. No favorable effect is an implementation acceptance criterion.

### 0.3 Decision register

The following preserves H2 R1–R9 in substance. These design rulings are ratified under Q-7; the stated admission gates remain.

| Ruling | Conflict resolved and binding choice |
|---|---|
| R1 | E3 F2 controls E2's preterminal preference and D1/D2/F1 timing alternatives: seal and finish the subject, settle execution cost once, then execute instrument grading. M1 is actual persisted created→finished. Required-grade pending gates every consumer; legacy mode stays unchanged. |
| R2 | First-valid immutable grade selection controls retries/overlays. Wrong-but-valid is zero, later failure cannot erase a valid grade, authorized invalidation supersedes without new n/recharge. Material oracle/task/environment/method correction requires a new confirmatory cohort; diagnostic regrading and clerical recomputation stay labelled. |
| R3 | E3's retained-view/prepared-call/whole-vector Account path controls treatment and charging. Primary charges derive totals across ancestor/outstanding holds. Physical incurred/billed usage survives logical reversal; exposure cannot become replenished physical capacity. Unknown bounds refuse matching. |
| R4 | D4 replaces D2's CP/Bonferroni parity implementation choice. REQ-D-108 has exactly one canonical supersession target, REQ-D-304; REQ-D-309/310 preserve feasibility/routing dependencies. Historical CP arithmetic remains diagnostic, with no parallel mandatory engine or post-outcome fallback. |
| R5 | A signed pre-data dimension→metric/target/population/range/margin/applicability map determines required parity outcomes. Four caps are not four automatic statistical endpoints. Required unavailable endpoints/vetoes block equivalence. D4's bounded relative-mean and single-conjunction IUT exceptions are narrow; D2 efficacy Holm remains separate. |
| R6 | First-wave efficacy is HH-native and finite-manifest. Admission covers the actual participant and both policies, or justified preregistered reuse of identical bridge semantics. No original-runner treatment-effect transfer or mixed Bayesian/frequentist joint certificate follows. Finite-task Bayesian efficacy, task-mixture sensitivity and sharp-null tests retain different targets. |
| R7 | F1 profile stays in native bundles; all planned slots and typed payload references close under recipient-specific procedures. R0 uses open/finished, independent of task outcome. Exact identity/verdict checks coexist with frozen numeric tolerance. Rights and package quotas do not fund or clear the full study archive. |
| R8 | Preserve D3 O1–O8 ×3, independent observation, Kp4 unknown/probe and actual permission transitions. Preserve four F2 example families, semantic controls, committed-HEAD quickstart and CI path rules. No scratch-directory containment claim or conflation with fault-free S3. |
| R9 | Preserve stable guides, run/inspect/learn branches and ordinary contributor simplicity. One outside verify/recompute reviewer precedes 4–6 formative evaluators; assistance, denominators and missing follow-up remain visible. GitHub first; readiness/consent/recruitment/publication authorization remains required. |
| Synthesis clarification S10 | F2's informative EX-02 analysis needs an admitted method on its exact path. P4 acceptance therefore depends on the necessary early P8 analysis/method slice, or verified already-admitted equivalent, without waiting for live parity or collection. P1–P8 are containers for decomposable slices, not indivisible tickets. Source: F2 F2–F3, E3 F6–F7, H2 F2, coordinator synthesis review. |
| Review ruling S12 | Adapt only REQ-B-102’s retained round-trip acceptance to the approved optional one-direction S6 exercise: native artifact→selected foreign format→pinned external reader, field/loss verification and unsupported direction/version controls. No return import is required; one-way acceptance earns no round-trip or reverse-direction claim. The literal source row remains historical provenance; AC-1-2 governs this adapted predicate. |

**Confidence-label clarification S11:** H2/D4's “disjoint=not-equivalent” must name its error level. The α=.05, 90%-coverage TOST construction decides equivalence. Reserve inferential **not-equivalent** for the separately computed 95% reporting interval lying wholly outside the equivalence band, with applicable selected-claim multiplicity; otherwise disjointness of the 90% interval is descriptive and the negative inference remains inconclusive. This changes no estimator and is an explicit confidence-label clarification, not a silent rewrite of the accepted notes.

REQ-H-010's independent method-review prerequisite is fulfilled by accepted D4; method implementation and formal admission remain owed. Later dates confer no general precedence. Q-3–Q-6 planning decisions supersede earlier notes' unanswered-policy language, while retaining their execution-specific gates.

### 0.4 Map

[§1 scope and researched limits](#s1) · [§2 execution](#s2) · [§3 measurement](#s3) · [§4 compaction and parity](#s4) · [§5 bounded operations](#s5) · [§6 receipts](#s6) · [§7 executable reader journey](#s7) · [§8 participation](#s8) · [§9 resources, amendments and handoff](#s9). Appendices: [A candidates](#appendix-a), [B Q decisions](#appendix-b), [C sources](#appendix-c), [D terms](#appendix-d), [E independent review](#appendix-e), [F outline coverage](#appendix-f). Tables are normative where stated; no illustrative sequence creates an event class, CLI command or public API.

<a id="s1"></a>
## 1. Audience, scope and researched limitations

**REQ-B-001, REQ-B-107, REQ-C-102** select the technically prepared harness evaluator: an engineer/researcher comparing useful task outcomes after changing one harness policy under fixed resources, able to inspect why outcomes differ. Integrators, replicators and contributors share the evidence. The audience need is a hypothesis chosen from operator intent and source analysis; no outside demand or usability result has been established. First value means a newly produced independently correct artifact plus correct interpretation of its outcome, not connectivity, terminal success or fixture completion (REQ-B-002).

The accepted audits describe baseline capabilities, not new verification by this draft. A1 found one shared embed boundary, native owners and typed refusals, but selected claim/launch/settle/compare wrappers omit required inputs. A3 recorded offline request/R0 failures: an exit-zero report can still be `not_reproduced`; R0 compared task-terminal vocabulary against bundle lifecycle vocabulary. E1/E2 found no composed proof on the inspected path of launched-subject adoption, task execution/sealed oracle grading, effective compaction-to-wire or all four matched caps. These bounded findings do not assert global nonexistence. Proposed fixes remain unimplemented.

A2's dated audit found 34 current receipts with matching redacted transcripts, 32 top-level passes and two failures, 90 spend entries with repeated paths, and six passing transcripts containing task `format_failure`. Those counts are not independent task success rates. Date/leg overwriting obscured earlier attempt receipts; missing native stores cannot be reconstructed from Markdown digests. Historical judge, plugin, replay and R0 evidence retains its exact scope. No controlled compaction efficacy study was identified in that audited set. A release tag or published repository does not alone earn `public`, `staging-verified` or usefulness claims.

### 1.1 Six selected scenarios

| Scenario | Required result and scope | Unmet/optional boundary |
|---|---|---|
| S1 useful task, inspect, replay, share | One unchanged upstream task, two fixed fresh subjects, independently checked submission, actual annotated native receipt and separately tested procedures (§5/§6). | No success-seeking replacement. A correct output is needed for useful-output acceptance; negative attempts remain. |
| S2 offline Lab | Two synthetic tasks × two arms × five repetitions, real scheduling/projection/persisted analysis, informative unequal contrasts and negative controls (§7). | Fixture teaching/plumbing only; admitted exact analysis path required. |
| S3 compaction | Central conditional HH-native two-arm finite-manifest study with oracle, policy-specific parity admission, four caps, detectors and frozen analysis (§4). | Missing admission retains an unmet objective requiring explicit scope disposition. Fixtures cannot fulfill it. |
| S4 operations | Eight cells ×three using the shared task/effect apparatus, independent sink, stage denominators and negative controls (§5). | Named host/fault/threat assumptions only; no population reliability or security certification. |
| S5 participation | Fixed no-run bug, verifyfixed, verify/recompute paths (§8). | At most one optional existing evict_oldest package exercise; no third study arm. |
| S6 interchange | At most one external consumer/direction, preferably existing Inspect JSON; ATIF only if simpler for this integration. | Optional after native readiness; pin format/consumer/version, preserve native authority and losses; no new viewer. |

The bounded edit exercise consumes/enforces one supported cap/permission field and retains new configurations/attempts; quote these runs separately from two fixed S1 subjects and confirmatory repetitions. Scope assumptions are one OS/environment family, one supported fixed-role lane, existing dependencies, unchanged task/oracle and existing owners. New backend, public interface, semantic amendment, tier/dependency reach or resource ceiling returns for explicit scope review. Routine ticket splitting does not.

### 1.2 Complete capability catalogue

The next matrix merges A1's source-backed entry/contract/coverage with C1/H2's disposition. “Inspected” identifies source/test or historical evidence, not a fresh pass. Negotiation, stable/experimental tier, host capability and compiled features are separate gates. C1 helper/env/ledger paths and C4 embed fleet/evolution/debt paths retain removability and typed refusals; no new tier is admitted.

| Family | Inspected entry / owner | Legacy contract | Baseline evidence / limit | Disposition / reason / revisit |
|---|---|---|---|---|
| Run lifecycle and durable history | `hh run start/status/events/inspect/resume/cancel`; `open_session`, `submit`, `read`, `project`, `verify`; `EmbedService` | R-2.2.1; R-2.11.4 | Scripted boundary lifecycle; selected historical live bindings. Read success is distinct from task outcome. (A1 F1, F12). | **First-wave:** S1/S4 Foundation for every observation; expand platforms/faults only after one environment and invariant set work. |
| Definition assembly, identity and registry | `hh definition plan/apply/diff/explain/validate/identity/compile`; `hh registry`; `lab.assembly.*`, `lab.registry.*` | R-2.1.2–4; R-2.10.1–2 | Assembly source differs from bare HIR. Typed diagnostics and fixture conformance. (A1 F1). | **First-wave:** S1/S3 pinned edit/validate/compile loop Make the changed policy identifiable; defer registry catalogue expansion until a second reusable definition needs it. |
| Providers, model profiles, routing and caches | Definition `model_port`; `GatewayModel`; gateway/budget/context crates; CLI `profile` family | R-2.3.1–4 | Local-compatible, OpenAI and Anthropic lane implementations; selected real-call receipts, fake-provider tests; provider capability differences remain typed. Routing/cache breadth is mapped, not exhaustively audited here. (A1 F3, F12). | **First-wave:** S1/S3 one lane/model level and declared cache policy Control confounding and setup; later routing/cache/model study after stable accounting and a demand-backed question. |
| Context building and compaction | Definition `context_policy`/compaction bindings; `KernelAssembler`, context policy and compaction families | R-2.4.1–2 | Real builder wiring with fixtures; summarizer/provider ports. A compaction policy's existence does not establish its quality effect. (A1 F3). | **First-wave:** S3 canonical two-arm contrast Central obligation; broader builders/provider-native compaction later after binding/detector parity and the first report. |
| Retrieval, memory lifecycle and consolidation | `MemoryStore`, indexed retrieval, definition consolidation rules, memory-store ABI | R-2.4.3–4 | Boundary tests exercise durable procedure/memory rows and rehydration; fake/recorded embedding inputs. (A1 F3). | **Later:** L1 Adds corpus validity, embedding and cross-episode state; revisit with isolated memory assertions and a pinned task/corpus. |
| Skills and Procedure IR | Procedure supplies; compiler targets; `hh/workflow`, `hh/program` | R-2.4.5 | Procedure selection/activation fixture coverage and plan-walker implementation; not proof of arbitrary imported skill success. (A1 F3, F4). | **Later:** L2 A procedure lesson could become a new executor; revisit once shared task ports can run one admitted procedure and check it. |
| Control and planning | Definition `control_strategy`; minimal/steerable/plan-execute/workflow/program variants; durable steering | R-2.6.1–2 | Source dispatch plus scripted driver/steering tests. Unknown variants refuse. (A1 F4). | **First-wave:** fixed baseline control in S1/S3; comparative study **later L3** Needed to execute, held constant to identify compaction; revisit after first contrast and executable alternate bindings. |
| Delegation, topology and coordination | `hharness_subagent::spawn`; `Orchestrator`; driver delegate seam | R-2.6.3; R-2.6.5 | T0/T1/T2/T3/T6 admitted by current topology code; T4/T5/T7 absent from implemented set. Requires bound ports. (A1 F4). | **Later:** L4 Additional ports, interference and team-budget accounting; revisit after a real bound topology and matched solo baseline exist. |
| Value of computation / budgeted choice | Definition `compute_policy`; `policy_for_configured` | R-2.6.4; R-2.1.6 | Static/uniform/rules/bandit/surface_prior/predictor selected in code; no measured efficiency improvement established here. (A1 F4). | **Later:** L5; fixed compute in S3 Adaptive selection changes estimand and cost; revisit with split discipline and complete search/inference accounting. |
| Resource budgets and telemetry | Budget definitions; `account`; ledger-derived cost/trace views | R-2.1.6; R-2.9.1 | Reserve-before-spend, attribution and matched-budget contracts; ceilings operate at decision points, not arbitrary effect interruption. (A1 F5). | **First-wave:** S1/S3 accounting, S4 bounded limit Essential to interpret comparisons; defer optimization/frontiers beyond required reporting until metering is validated. |
| Authorization, approvals, containment and secrets | `hh approval`; `respond_permission`; environment/helper and credential broker | R-2.8.1–7; R-2.5.5 | Typed permission/environment paths and focused attack/secret batteries; no general security certification. (A1 F5). | **First-wave:** S4 named deny/pause cases Demonstrate observable controls; broaden only with an explicit threat model and environment coverage, never general assurance. |
| Verification and independent critics | Completion gate, validators/reconcilers; lane-backed judge adapters | R-2.7.1–3 | Scripted completion-gate tests and fake-provider judge tests; model claims are not authoritative grades. (A1 F5). | **First-wave:** S1/S3 executable oracle Independent grades are essential; broader/calibrated model-critic study later when a task demands it, with non-headline labels. |
| Recovery and environment lifecycle | `suspend`, `compensate`, `heal`, `continue_goal`, `subscribe`; `env.*` | R-2.2.3–5 | Fixture lifecycle/restart cases; unsupported snapshot/hibernate capabilities remain explicit. (A1 F5). | **First-wave:** S4 one fault window Bounded proof reuses S1; additional snapshot/hibernate/platform cases wait for advertised host capability and fault controls. |
| Replay, branching and causal attribution | `fork`, `replay`, `counterfactual`; `lab.attribution.*` | R-2.2.4; R-2.9.7 | Reconstruct versus deterministic replay; attribution folds supplied outcomes and can open arms. Fresh causal evidence is a separate task. (A1 F8). | **First-wave:** S1 selected replay; causal intervention **later L6** Replay explains a receipt; attribution needs freshly executed matched counterfactuals, not supplied outcome rows. |
| Benchmark integration and grading | `BenchmarkAdapter`, `hh-bench-adapter`, `grade` | R-2.9.4 | A/C/D/E adapters are fixture adapters; grade consumes supplied verifier output. A real independent grader bridge must be identified. (A1 F6). | **First-wave:** S1/S3 selected external task, TB2/parity path Highest execution uncertainty; expand suites only after independently graded bridge, parity and resource review. |
| Experiment scheduling and matching | `lab.experiment.*`; CLI `experiment` wrappers | R-2.10.3 | Durable launch/settle infrastructure and exemplar documents; composed tests synthesize terminal/grade rows. Several CLI operands cannot reach required params. (A1 F6, F7). | **First-wave:** S2/S3 one supported runner Reuse existing semantics and repair selected requests; distributed scheduling waits for a measured execution bottleneck. |
| Evaluation, results, comparison and analysis | `lab.eval.*`, `lab.results.*`, `lab.analysis.*`, leaderboard methods; Rust analysis kernel | R-2.9.2; R-2.10.4–5 | Class-aware results and estimator plumbing; typed n/a/refusals; CLI `compare report` request mismatch. (A1 F6, F7). | **First-wave:** S2/S3 existing kernel and reports Required for defensible outputs; new public ranking surface **excluded** absent an approved comparison/admission job. |
| Portable bundles and import/export | `hh bundle create/validate/reproduce/show`, `hh run export/import`; `kernel.*` | R-2.9.3 | One inspected clean-store receipt is R0; no inference of fresh task execution or maximal-level reproducibility. (A1 F8, F12). | **First-wave:** S1/S5 native closure and corrected R0 Makes evidence usable; higher levels/foreign formats expand only after exact procedures and access are feasible. |
| Plugins and external harnesses | `plugin_abi/1`, package sealing/load, `VariantHost`; `lab.extension.*`, `lab.hosting.*` | R-2.12.2; R-2.8.5; R-2.10.6 | Concrete packaged binary flow and hosted fixture composition. Missing binary/helper can skip an ordinary test body. (A1 F9). | **First-wave contingent:** S5 one compaction package; external participants **later L7** Realistic extension path without another study; revisit hosted runners when task parity and class-scoped accounting work. |
| Fleet and external work sources | `fleet.*`; CLI view/items/item/reconcile/observe/capabilities/records/webhook | R-2.12.6 | C4 boundary, fixture source and signed-webhook lane selection. Not an autonomous organizational outcome claim. (A1 F10). | **Later:** L8 C4 governance and external-system effects exceed one evaluator job; revisit with an operator/work-source use case and human gates. |
| Evolution, debt retirement, model drift and co-evolution | `lab.evolution.*`, debt/model/coevolution/org-policy families | R-2.9.5–8 | Record-driven governed services, some C4-gated; HarnessHarness does not train weights. (A1 F10). | **Later:** L9 Search/transfer/default gates multiply scope; revisit after baseline reports, pinned old/new models and governed candidate evidence. |
| Web, MCP, ACP, SDK and foreign conformance | Loopback HTTP; generated clients; `hh-web`, `hh lab serve`, `hh acp`, `hh-xcheck` | R-2.5.4; R-2.11.2–4 | Artifact MCP catalog versus Lab MCP execution must be separated; foreign implementations cover a bounded corpus. (A1 F11). | **First-wave:** one supported SDK/RPC/CLI route; S6 optional exchange Reuse native receipt; web supplementary, MCP/ACP/foreign lessons later after exact negotiation/update/corpus scope checks and a reader need. |

### 1.3 Later questions and exclusions

| Later group | User question / scoped claim | Prerequisites, proof and cost driver / revisit |
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

These are adopted planning dispositions with cumulative prerequisites and revisit triggers, not newly opened DF rows or funded studies. First-wave exclusions are production/security certification, turnkey fleet/no-code service, universal runtime/client compatibility, an authored benchmark suite, a new leaderboard/viewer and uniqueness claims. Broader fault/security campaigns, cross-framework compaction and multi-model factorials require their own protocols. Existing contracts for later features remain binding.

### 1.4 Ecosystem and suggestion dispositions

**REQ-B-101–107, REQ-C-007–008** retain TB2 despite newer catalogue versions; every benchmark-dependent claim pins suite/task/runner/oracle/image, not a mutable tag. Harbor task/trajectory boundaries and Inspect evaluation/log tooling overlap the job; existing conversion fixtures are not proof of a real external-reader round trip. S6 must exercise the chosen supported direction with one real artifact, external validation, preserved-field/loss map and unsupported direction/version controls. Imported foreign data does not acquire native authority. Inspect strategy names do not establish treatment identity. LangGraph checkpoint/replay terminology is a comparison of documented semantics, not compatibility. Harness-Bench participation waits for resolution of its conflicting fetched population/manifest claims. TraceTrial informs mechanism-versus-outcome questions; GEPA-like adaptive search stays later with pre-search splits and full selection cost. Read current primary sources/rules again before integration or outreach; these are accepted source-review conclusions, not current service certification.

**S12 / REQ-B-102 acceptance adaptation:** If S6 is selected, one native artifact is exported in the chosen foreign format and accepted by the pinned external reader. Check preserved fields, declared losses and unsupported direction/version cases. A return import is not required and must not be claimed. This explicitly replaces the original source row’s round-trip acceptance predicate within the approved optional one-direction scope; all other identity, loss and authority predicates remain.

Retain the proposed anatomy/short preview, executable examples, independent receipt procedures, fixture Lab, bounded budgets/permissions, evidence index and later fleet disposition. Correct “same run again for free” to supported recorded-input replay with local cost and defined comparisons; correct “plugin swap without compilation” to a tested build/seal/load/invoke path and only evidenced no-kernel-rebuild claim. Web screenshots and foreign corpus replay are optional supporting evidence with exact scope; selected client requests **and notifications/callback failures** must be retained. Reject unsupported uniqueness, weekend/low-cost estimates and use of receipt counts/spend as efficacy. Real compaction remains central rather than a silently dropped stretch goal.

<a id="ac-1-1"></a>
**AC-1-1 (D):** Review all 22 families, nine later groups, six scenarios and every brief suggestion against the selected audience, available entry, contract, evidence limit, disposition and revisit condition. Missing families, changed scopes or fixture substitution fail coverage. No inferred external demand or effort estimate may pass as observation.

<a id="ac-1-2"></a>
**AC-1-2 (E, conditional S6):** For the selected single consumer/direction, export one real native artifact, run the pinned external reader/validator, compare each promised field and recorded loss, and exercise unsupported direction/version refusal. Preserve the native source/identity and exact conversion/reader versions. A successful native→foreign read with no return import passes this bounded exercise; it does not establish round-trip/reverse conversion, general interoperability or stronger native evidence. An unselected S6 remains optional and awards no compatibility claim.

<a id="s2"></a>
## 2. One native execution, grading and accounting chain

### 2.1 Ownership and recoverable launch

**REQ-E-201** unions REQ-E-001/101; **REQ-C-103, REQ-A-003/104, REQ-E-002/007/008/009/102/103** retain their separate task/oracle/client predicates. Use one versioned in-process `hharness` Rust recipe above existing owners. No copied runtime, shadow result/receipt store, forged producer rows, embed→bench or experiment→results dependency, lower-layer→runner edge, duplicate codec/serializer, or new statistical service. Proposed interface names below are design seams, not existing public RPC promises.

| Owner | Interface responsibility and durable fact |
|---|---|
| experiment/budget | Recoverable `launch_bound`: launch intent, lease, funding slice, actual subject root and grade binding. |
| embed arming/drive | `adopt_subject(binding, environment, ports)` reuses the existing runtime, one subject and current fenced writer; execution profile. |
| control/env Dispatcher | Validate already-intended coordinate/scoped writer; own authorization→settlement once; return a durable effect reference, suppress duplicate driver lifecycle rows. |
| bench/verification | Execute pinned unchanged oracle over sealed submission in instrument run/Account; invoked/verdict refs bind subject, submission, oracle, attempt, policy and cost. |
| eval, experiment, results | Pure grade selection plus owner-validated targets/causes/watermarks; execution-settled, pending, graded disposition and immutable selection revisions. No eval→ledger edge. |
| context/gateway/budget/control | Durable retained view, existing serialization, conservative bounds, reservation, sealed request, dispatch, usage and deadline. |
| bundle and producer owners | Typed closure inputs and native profile; all-planned population, exact source views, recipient closure. |

Persist launch intent→allocate/open→bind subject/root→publish→arm. Recovery must link existing manifests or idempotently abort/release; it cannot assume cross-run atomicity, spend from an orphan root or create a second subject. P2 begins by admitting HelperExecutor via local-container/Podman against unchanged TB2 lifecycle, confinement, surviving observer and read-only probe. Suitability is unproved; unsupported blocks dependent S1/S4. A new backend requires scope review, never an in-memory stand-in.

### 2.2 Seal, finish, then grade

**REQ-E-202/203/204** govern required-grade mode. Before launch durably freeze mode, task/submission/oracle pins, selection/regrade policy and instrument funding. Absence preserves legacy mode; unknown required features refuse.

1. Stop participant actions; drain committed work to observed or explicitly unknown; revoke participant access; finalize usage and seal immutable end-state/submission. Failed sealing cannot grade a mutable workspace.
2. Record actual finish, stop and veto. M1 uses the persisted run-created origin through actual finished time, including adoption delay, approvals, restart downtime and drain/seal. Correct turn-start measurement; no backdating, oracle subtraction or restart deadline reset. Unknown/untrustworthy origins cannot prove enforcement. Instrument and end-to-end spans are separate.
3. Settle deduplicated execution cost/returned funding once. `grade_pending` is admission state, not a seventh outcome class. Scheduler, results/query, export, compare and analysis must block scored success, completed-plan interpretation, participant relaunch and non-partial close while pending. Partial close lists every unresolved slot. Resource release conveys no correctness.
4. Persist instrument invocation before running the unchanged upstream oracle on the sealed submission; retain executable/environment/access, exit, raw stdout/stderr/reward, timeout/refusal, grade mapping and instrument finish/cost. Correct/wrong/broken-oracle controls must diverge. Participant validation/repair is subject work; oracle/probes never feed back. Preserve upstream shared-verification semantics and tamper `n/a{no_detector}` when appropriate; do not silently claim isolated grading from an enum.
5. Select the first valid grade within the frozen valid-grade set. Wrong-but-valid is valid zero; crash/timeout/empty/nonfinite/refusal is oracle failure, permitting capped instrument-only regrade of the same submission. No participant rerun, recharge or extra n. A later failed attempt cannot erase a valid grade.
6. Record graded disposition using shared pure selection. Valid grading restores the stop-derived outcome class, including `budget_exhausted`; raw grade, stop and veto remain separate. `plan_disposition_at(scoring_ref)`, scheduler, results and export must agree.

Authorized grade invalidation/correction appends reason, selection revision and supersedes links; invalidated evidence remains unavailable/oracle-failed until replaced. Reports pin eligible grade set, selection refs, prefixes and watermarks; old reports reproduce old selections. A material oracle/task/environment/method correction requires a new confirmatory cohort. Regrading old submissions may be a labelled diagnostic; clerical/recomputation corrections produce superseding reports with preserved original inputs/results. No historical WAL, cost or denominator rewrite.

<a id="ac-2-1"></a>
**AC-2-1 (E):** Reject stale/duplicate/mismatched adoption and orphan funding. Observe one actual Dispatcher effect and output joined to that launched subject, with one valid writer, commit and terminal sequence and no phantom second run. Test crash windows before seal/finish, finish→grade, invocation→verdict, verdict→settlement and transfer→disposition; restore the same subject/pending state and costs. Test failure→valid→failure, invalid targets, authorized invalidation, duplicate settlement, veto and budget-exhausted valid grade. Assert pending blocking independently in scheduler/query/export/compare/analysis/non-partial close and old-watermark reproducibility. Long verifier, delayed adoption, approval wait and restart must leave correct M1 and no recharge/new n.

### 2.3 Treatment reaches the exact wire

**REQ-E-206** unions REQ-E-003/104; **REQ-E-004/006/107, REQ-D-005** additionally bind metrics and model roles. Freeze actual native `evict_oldest` and `clear_tool_results` bindings/parameters, common control/builder/trigger, required-item protection and fallback. Unknown/unbound variants refuse. Persist retained/omitted item/content identities and legal tool/result, batch and required-group joins; rebuild from this durable selection. The existing gateway renderer must serialize that selected view and never resurrect omitted transcript.

The order is retained view→existing serialization and primary bounds→whole-vector Account reservation→sealed provider plan→durable request→deadline-bounded dispatch→durable actual/unknown usage→charge/release→guard. Capture actual dispatched bytes, inspectable content and call/transport-attempt joins, intended/served policy, next-dispatch, pressure opportunities, activation, fallback, ineffective and impossible outcomes. A hash without accessible content cannot independently demonstrate treatment. Instrument observations cannot feed subjects.

<a id="ac-2-2"></a>
**AC-2-2 (E):** A capturing transport verifies both policies change the expected payload and preserve required/legal groups. Cover unknown policies, no trigger, ineffective reduction, fallback, no next dispatch, retries and crash/restart. Check exact policy→selection→assembly→sealed request→actual wire joins and no omitted-item resurrection; labels, plugin conformance or fixed counters do not satisfy this test.

### 2.4 Four real caps and physical exposure

**REQ-E-207** unions REQ-E-005/105/106; **REQ-D-006** retains measurement/reporting. Match `{tokens.input.total,tokens.output.total,model_calls,time.wall_ms}` under the same snapshots/cold start. Subject root is distinct from its funding slice. Charge admitted primary token categories only; input/output totals remain derived. Account must reserve a **whole vector**, checking every ancestor's consumed usage, outstanding/new primary holds and slice holds against projected derived totals. Existing scoped Account deduplication is authoritative; propagated leaf/ancestor copies count once in experiment/results.

Bound provider/cache input overhead, visible plus reasoning output, all roles and actual call/transport attempts before dispatch; reconcile actual response, failure, retries, in-flight and ambiguous use. Unknown bounds/exposure refuse enforced matching and unsafe further dispatch. Real elapsed deadlines persist across restart/wait; socket timeout is not remote cancellation. Expiry blocks new dispatch, drains committed work to observed/unknown, charges incurred use and records overruns. No accounting erasure to meet a cap.

Logical reverted totals do **not** erase physical incurred/billed use or replenish an actual-use ceiling. P7 must enforce physical use/exposure through owning records or refuse matching; no runner-side ledger. Preserve raw reservations, charges, release, source/dimension attribution, price snapshot/confidence and actual/unknown billing, separately from logical scope totals. Subject task/summarization/repair/validation/retry work stays subject-charged; independent oracle/probes are instrument-funded. Account for preparation, environment, storage and cancellation costs without duplicate totals. Equal caps differ from equal consumption; median consumption deviation >0.10 is reported `imbalanced` with distributions/utilization, not automatic statistical inequivalence or proof that underuse is efficient.

<a id="ac-2-3"></a>
**AC-2-3 (E):** Test every cap, combined primary components, simultaneous child/ancestor/slice holds, actual≠reserved, duplicate/recovery settlement, failed/ambiguous usage, logical reversal after real spend, cache/role drift, restart/approval delay and expiry with committed work. Assert no replenished physical capacity from reversal, no double counting and zero dispatch when exposure is unknown/unfunded. Report fixture versus live enforcement separately.

<a id="s3"></a>
## 3. Common measurement and claim admission

### 3.1 Coordinates, populations and outcomes

**REQ-D-001/002/003/004/007/009/010, REQ-A-004/102** bind observations to `(protocol, phase, task, split, stratum, arm, replicate, plan, attempt, subject)` and full factors/class/granularity. Calls/verifiers are children. Retries/replay/regrades/events are never independent repetitions. For metric m, freeze the within-task reducer `x_at`, `d_t=x_test,t−x_control,t` and equal-task contrast `Δ=Σ_t w_t d_t`, `w_t=1/T` within each declared contamination stratum. Publish values/weights/units/direction and task/slot/first-subject/all-attempt/gradeable/selected-denominator counts. No weighting by retries or event opportunities.

| Outcome | Required interpretation |
|---|---|
| `scored` | Executable pass/fail is 1/0; lifecycle/process success is separate. |
| `budget_exhausted` | In capability denominator; retain valid grade. Missing grade is zero only under frozen `count_as_failure`; otherwise unknown. |
| `infrastructure_failure` | Environment/provider reason/detector; excluded from default eligible denominator, counted alongside and in all-planned bounds. |
| `oracle_failure` | Crash/timeout/empty/nonfinite/refusal is unavailable, never task zero; sealed submission retained. |
| `refused` | Attempted refusal; a prelaunch refusal remains planned/not-run with experiment fact, no fabricated subject. |
| `cancelled` | Launched cancellation with reason/time; unlaunched slots not-run. Distinguish deadline exhaustion from shutdown. |
| Veto/unknown | Raw pass and veto IDs survive. Unknown detector/check is neither zero nor clean. Outcome class, availability and veto are separate axes. |

Base N is scored ∪ budget-exhausted. Canonical headline excludes vetoes; show base N, clean N, vetoed passes and each tested/unknown veto. Also report **joint task-pass-without-observed-veto** over base N: observed veto=0, missing required checks=unknown. Compare and scorecard must use the same declared population. No class/veto filtering may silently improve a selected denominator.

First subject per slot is primary. Secondary recovery uses the first admissible completed retry, never best-of, preserving failure/cost. Freeze retry infrastructure classes/backoff: at most two reattempts per plan and at most 10% of plans, subject to separately funded allowance. Wrong answers do not retry. Regrades use §2.2. Short cells keep planned/not-run rows; no n inflation.

### 3.2 Missingness, tails and distinct uncertainty

For M planned first slots with s known successes, f failures, u unresolved, retain `[s/M,(s+u)/M]`. Equal-task contrast bounds are `[Σw(L_test−U_control),Σw(U_test−L_control)]`. Include unrun tasks; raw pass-with-veto counts as success for raw outcome and failure for joint outcome. A conditional eligible-only point/interval is not an all-planned estimate. Continuous completion bounds require frozen physical `[L,U]`; unbounded cost/intensity has no fabricated finite upper bound. Robust sign/practical conclusions require every permitted completion to preserve sign/margin.

**REQ-D-008/109, REQ-E-209** require actual owner-routed estimator/version/target/assumptions/prior, keys, seed, actual draws, floors, refusal and interval type/level in analysis. One task permits raw observations only; fewer than two tasks, missing keys, short/unequal required cells, unsupported dependence or degenerate estimator refuse inference. Existing pooled Beta, always-bootstrap and hardcoded 4,000 draws do not satisfy the proposed design by metadata alone. Whole-task bootstrap and sign-flip targets are specified in §4; no flattened event/replicate n. Full frozen primary/stratum families span split reports, retaining unavailable members; Holm α=.05 for efficacy primaries, BH for frozen exploratory families. Distinguish interval coverage from multiplicity-adjusted tests.

Retain `(c,n)`, `pass^k=C(c,k)/C(n,k)` and `pass@k=1−C(n−c,k)/C(n,k)` only for k≤n, with first subjects. Report signed effects, sign counts, P95/P99/max cost/wall/severity and P50/P95/max signed/absolute effects with n/censoring. Nearest-rank P99 can equal max in small samples; observed max is no population bound. Tail uncertainty uses declared clustered resampling where valid, otherwise undefined. Missing terminal usage/time is right-censored with lower bound, never a pretend cap-valued completion. Descriptive Wilson companions retain their binomial/clustering limits.

<a id="ac-3-1"></a>
**AC-3-1 (E):** Mixed classes/vetoes/short cells reconcile scheduler, projection, compare, scorecard and export. Reproduce the D1 counterexample: two tasks each with A=5 passes+45 unknown and B=25 passes+25 failures yields planned contrast bounds `[−0.4,0.5]`, not pooled excluded-fraction robustness. Test unknown/unrun/veto/censored/unbounded cases, retry/regrade denominators, one-task/missing-key refusals, signed tails, actual seed/draw routing and a multiplicity family split across reports.

<a id="ac-3-2"></a>
**AC-3-2 (D/R):** A protocol hash precedes dependent subjects and fixes estimand, manifest, classes, selection, assignment, methods/families/margins/stops and limits. Integrity/isolation/leakage/treatment/accounting/drift/ceiling failure stops with partial rows and costs. No favorable stopping, retrospective shrink, pilot relabelling or unregistered sequential/futility test. Reports distinguish engineered capability, task truth, bounded operation, scientific effect and adoption; missing margin/opportunity/admission cannot pass. New material protocols fork confirmation.

<a id="s4"></a>
## 4. Central compaction protocol and conditional methods

### 4.1 Frozen native contrast, phases and assignment

**REQ-D-101/102/103/104/105/110, REQ-E-011, REQ-B-103/106** retain TB **2.0**, suite A held-out selection and unchanged upstream oracle. A=`evict_oldest`; B=`clear_tool_results`. Freeze parameters, common compaction-capable control/builder/trigger, permission/required-item/fallback rules, fixed model roles including unused summarizer profile, `fault_profile=none`, `search_budget=0`, `same_snapshot`, `cold_start`, `matched_cap`, tolerance `0.10` and §2.4's four caps. No task-conditioned policy, artificial pressure/padding, adaptive search or third arm. Model capability/ports, requested/served fingerprints, route/cache/seed support must match or refuse.

Import the 89-task TB2 catalogue identity but do not infer a catalogue-wide estimand. Eligibility is outcome-blind: immutable task/image/verifier and original runner, allowed resource/isolation/network/task-surface protections, validity/contamination and rights. Publish exclusions before outcomes; hard/untriggered tasks remain. `cancel-async-tasks` is only an integration candidate. Assign development and held-out pools before pilot access. Seeded sampling without replacement selects 30 eligible held-out tasks under frozen metadata/contamination quotas and equal within-stratum weights; insufficient eligibility requires pre-confirmation revision or stop. Exact IDs/pins/quotas/seed are later binding inputs, not guessed here.

Separate diagnostic, activation, pilot, parity, confirmatory and replication identities. Pilot may estimate feasibility, activation, failure, variance, tails and cost; it cannot tune policies under zero search, select favorable tasks or become confirmation. Tuning requires a separate exploratory design. Post-freeze defects retain original rows and dated changes; material changes create a new cohort.

Randomize task order and use a fair task-level coin to assign arms to identical lanes, interleaved over five rounds. Balance host/calendar, at most two concurrent subjects and one per arm; preserve committed seed/schedule and actual order. Reset environment/session, bypass cache or enforce a cold namespace; unknown warmth fails matching. Distinct seeds by default, with requested/honoured status. Efficacy pairs tasks; replicate numbers do not create common-random-number coupling. Drift stops collection. Shared-shock/carryover independence assumptions need justification; resets/balance alone do not prove them. §4.4's narrow parity grouping exception does not alter efficacy assignment.

Primaries are `capability.task_success`, `grounding.reacquisition_rate`, `reliability.repeated_action_rate`. Hypotheses include zero contrast for each and mechanism differences despite negligible task-success effect. Target expected first-attempt outcomes on the frozen finite manifest, in each declared stratum; no population, original-runner effect-transfer or C4/default claim. Repetitions R≥5/task/arm and first-slot coverage are required; floor is not power. Missing/unequal cells keep descriptive values/bounds plus `InsufficientReplicates` or unavailable inference.

### 4.2 Mechanism definitions

**REQ-D-005/106, REQ-E-004/107/209** require versioned registry names, native units/bounds/direction, detector/extractor IDs, windows and reducers before collection. For completed compaction c, use `(completion_seq(c), subject_execution_stop_seq]`, including later compactions and retaining overlapping `(c,event)` joins. `forgotten(c)` contains committed omitted item/content addresses. No grading-time events enter the subject window.

| Metric | Exact numerator/denominator and handling |
|---|---|
| Reacquisition | Matching post-c deliveries, filesystem or memory reads / forgotten items. Retain `(c,event,item)` joins; repeated deliveries count, so intensity may exceed one. Never probability/ppm clamp. |
| Repeated action | Post-c effects matching pre-c idempotency keys / all post-c effects; retain `(c,effect)` joins and keys. Argument/capability similarity remains a distinctly named approximation. |
| Activation/fidelity | Starts / eligible pressure opportunities; completions / starts; changed actual requests / completions with next dispatch; triggered first subjects / all first subjects. Retain intended/served policy. |
| Ineffective/fallback | Counts plus ineffective/completed and fallback/started. Fallback is an assigned-arm deviation, never a silently relabelled arm. |
| Recall/labels/summary | Recovered/reachable extracted facts / extractable forgotten facts; pre/post labels, summary applicability and summary-followed detector, opacity delta. Absent extractor=`no_detector`; no summary=inapplicable. |
| Overhead/latency | Compaction resource / same-unit subject total, separately tokens/calls/spend; actual durations and total counts. No compaction gives count 0 and unavailable conditional latency/rates. |

Sum numerator/denominator within subject, divide, average available subject ratios within task, then equal-task contrast; disclose coverage and missingness. Do not pool events as trials or weight tasks by opportunities. Zero denominator=`estimator_undefined`; missing ID/key/detector=`no_detector`. Untriggered subjects remain in task success; activated-only analysis is exploratory post-treatment selection. Required unavailable detectors block their claims and remain in the frozen multiplicity family. Association with omission is not proof of causal forgetting.

<a id="ac-4-1"></a>
**AC-4-1 (E/R):** Golden traces cover overlapping compactions, repeated deliveries, matching/nonmatching idempotency keys, missing keys/extractors, zero opportunities, no event, fallback/ineffective/impossible outcomes and a next-dispatch join. Recompute subject/task reducers and every reported denominator from native evidence; retain old approximation names. Protocol review verifies all TB2/arm/split/model/match/search/assignment rules before any collection.

### 4.3 Finite-manifest efficacy and sensitivity

**REQ-D-107/109** are a conditional amendment, implemented only after independent numerical/routing review and formal admission. For fixed task t and arm a:

```text
c_at | p_at ~ Binomial(n_at, p_at)
p_at ~ Beta(1,1), independent across task/arm cells
p_at | data ~ Beta(c_at+1, n_at−c_at+1)
Δ^(b) = Σ_t w_t [p_Bt^(b)−p_At^(b)], w_t=1/T
```

This likelihood assumes independent Bernoulli runs within cells. Task pairing is stratification, not correlated-replicate modelling. It is not a pooled Beta, hierarchical/task-population model or proof of independence. Use integer-Beta inverse-CDF bisection, canonical-input-seeded xorshift64*, 8,000 posterior draws, labelled 95% equal-tail **credible** intervals and analytic posterior mean; separately report observed equal-task contrast. Freeze numerical-error checks/doubling through 64,000 draws, report numerical error and actual draws; no seed shopping. Beta(2,2) sensitivity is predeclared exploratory and never replaces the registered prior. Per-cell Wilson companions require valid binomial targets. Unsupported dependence, missing keys, one task or inadequate cells refuse.

Whole-task paired bootstrap for rates/cost retains entire clusters: BCa at ≥30 tasks, percentile below, declared seed/draws/degeneracy and ≥2,000 confirmatory draws. It describes **task-mixture sensitivity**, not the finite-manifest Bayesian target. Task sign-flips require the registered lane randomization/exchangeability argument and test a sharp no-effect null, not weak-mean or practical-margin nulls: exact enumeration at ≤20 tasks, otherwise 8,000 Monte Carlo draws with `(extreme+1)/(draws+1)`. Preserve D2 Holm α=.05 across all three primaries and confirmatory strata, including unavailable members; exploratory BH stays separate. No combined omnibus efficacy verdict is justified by combining these different targets. Credible intervals are not frequentist TOST or automatic practical-margin certificates.

Before dependent confirmation, simulate the exact pipeline at T={10,20,30}, R={5,10}, null/±.05/±.10 effects, easy/hard mixtures, heterogeneous effects, zero activation, informative missingness/vetoes and shared shocks; ≥2,000 datasets/scenario. Retain parameters/seeds/failures; report availability, coverage/prior sensitivity, relevant frequentist type-I error, widths/verdict probabilities and Monte Carlo error. Propagate pilot uncertainty; the existing paired-binary formula is a cross-check, not authority to override this target. Unknown precision triggers stop/requote or explicitly approved limited scope. Provisional useful gain δ=.05, harm h=.05 and desired 95% half-width .10 are **not ratified margins**.

<a id="ac-4-2"></a>
**AC-4-2 (E):** Validate analytic/known answers and numerical tolerance, heterogeneous/unequal n, prior placement, independent versus correlated counterexamples, floors/refusals, actual seed/draw/family routing and deterministic recomputation. Run and retain the registered simulation battery with Monte Carlo uncertainty. Verify exact eval→analysis→persisted-report dispatch; changing metadata without changing computation fails. This accepts an apparatus, never a favorable treatment effect.

### 4.4 Authentic parity, dimension map and directional KL

**REQ-D-301/302/303/304/305/306/307/308/309/310** govern the proposed replacement. Authentic admission compares the **same participant/configuration** under HH (H) and original runner (O), with pinned task/split/stratum/model/environment/oracle, first attempts, matched budgets/cache, original/replay evidence, registered method/margins and equivalent verdict. Empty maps, wrappers, supplied model-I/O fixtures or oracle transport equality alone fail. Missing required replay is unmatched. Until admission, only product smoke with `parity=none`, not a component headline.

Before data sign a dimension→required metric, estimand/population, range/unit, margin, applicability and evidence map for the actual participant:

| Contract dimension | Required mapping category |
|---|---|
| Capability | Executable task success and denominator population. |
| Reliability | Failures/stops and any required pass^k; nonlinear pass^k needs separately admitted inference. |
| Grounding | Named observable detectors and coverage. |
| Efficiency | Required named cost/latency outcomes and physical ranges; four enforced caps are not automatically these outcomes. |
| Autonomy | Approvals/interventions and their observed population. |
| Security/compliance | Integrity/veto predicates; required unknown checks block the conjunction. |
| Portability/evolvability | Fixed-configuration limits and explicit typed inapplicability with reasons. |

Do not demand every optional scorecard metric, omit a difficult required endpoint after data or treat inapplicability as a pass. Conditional eligible-only/nonlinear outcomes need their own admitted inference; unavailable required dimensions prevent overall equivalence.

For one participant/policy/stratum with T fixed tasks and equal R, set N=TR and `Δ=Σ_t(p_Ht−p_Ot)/T`. Preassign H/O **first attempts** within `(task,repeat,pair_id)`. Let `W+=1{Y_H=1,Y_O=0}`, `W−=1{Y_H=0,Y_O=1}`, counts c+,c−. Across actual pairs must be independent; within-pair dependence is allowed. This is a justified design assumption, never proved by reset logs or IDs. Known common shocks, cache/carryover or reused subjects require refusal or another admitted design. Preserve task pairing, requested/honoured seeds, adapter floor ≥3/task and S3 floor ≥5/task/arm. Equal weights/R identify the finite target; unequal design needs another bound. The proposed grouping exception grants no CRN, honoured-seed coupling, R2 or causal pairing claim.

For x=c/N define `K_η(x,N)=[L,U]` by continuous Bernoulli KL inversion:

```text
kl(x,p) = x log(x/p) + (1−x) log((1−x)/(1−p))
N kl(x,L) = N kl(x,U) = log(1/η),  L ≤ x ≤ U.
L=0 when x=0; U=1 when x=1, with continuous endpoint conventions.
At x=0: U=1−η^(1/N). At x=1: L=η^(1/N).
point_Δ = (c+−c−)/N
L_Δ = L+−U− ; U_Δ = U+−L−
```

For independent [0,1] variables (not necessarily identical), the concentration bound is `exp(−N kl(x,μ))`. Each K endpoint has one-sided noncoverage ≤η. TOST α=.05 sets η=α/2=.025 for each directional endpoint: union bounds give each Δ-tail error ≤α without W+/W− independence and joint interval coverage ≥1−2α=.90. Equivalence requires **strictly** `L_Δ>−ε` and `U_Δ<ε`. Under either null branch, acceptance must reject that true branch, so size≤α, not 2α. A separate 95% reporting interval uses α=.025, η=.0125. The 90% TOST interval strictly inside the band gives equivalent; overlapping/touching is inconclusive. By S11, inferential not-equivalent requires the separate 95% reporting interval wholly outside the band with applicable selected-claim multiplicity; otherwise outside-band wording is descriptive and negative inference remains inconclusive. Numerical comparisons round outward. This is concentration, not CP/binomial inversion.

With u uncertain pairs preserve planned N. Each directional interval is `[K_η(c/N,N).L, K_η((c+u)/N,N).U]`; combine endpoints as above. This completion envelope is conservative; it does not assume missing-at-random. Keep canonical, raw and joint-without-observed-veto populations and class counts. Unknown is neither zero nor clean. Informative exclusions cannot gain coverage by shrinking N; required conditional claims need another admitted method.

For required absolute bounded difference `D=X_H−X_O∈[a,b]`, apply K to the mean of `(D−a)/(b−a)` and transform back; known a=b is exact. Zero sample variance does not establish zero range. For relative margin ρ target the **ratio of finite-manifest means**, using two contrasts:

```text
lower bound for E_H−(1−ρ)E_O > 0
upper bound for E_H−(1+ρ)E_O < 0
```

Use known physical ranges, affine normalization and K at α for each equivalence direction. For S11's inferential negative verdict on a relative endpoint, let C−=E_H−(1−ρ)E_O and C+=E_H−(1+ρ)E_O: require U(C−)<0 or L(C+)>0 using one-sided error α/2=.025 per negative branch (or stricter registered selected-claim allocation). Their union controls this two-sided negative decision at .05; no ratio interval or observed-reference division is implied. Otherwise report inconclusive. No raw milliseconds-versus-ppm comparison, observed-reference division, mean-of-task-ratios substitution, clipping or invented bound. Zero/near-zero reference needs a frozen absolute convention or refusal. Unbounded/unknown billing, unbounded intensity, missing values without valid completion bounds or changed eligible-only populations make the required inference unavailable.

For one preregistered **all-endpoints/all-strata equivalent** conjunction, the narrowly admitted IUT requires no additional endpoint Bonferroni: at least one true null must be rejected. Selected individual or multiple claims and any-endpoint efficacy still need their respective family correction. Unknown members and compliance vetoes cannot be removed/averaged away. This exception does not change D2 efficacy Holm.

<a id="ac-4-3"></a>
**AC-4-3 (E):** Independent review validates derivation and existing-owner routing. Test endpoints/continuous limits/monotonicity, strict/outward-rounded boundaries, small-N/heterogeneous tails, sign reversal, .90 TOST versus .95 reporting, missingness/veto envelopes, relative scale invariance/zero reference, unknown bounds, unequal/missing identity, empty pins, reused-subject/shared-shock counterexamples and selected-versus-conjunction families. Unsupported assumptions must refuse through the actual report path. REQ-D-108's historical R5 width calculation remains an inspected diagnostic; this acceptance implements REQ-D-304 with REQ-D-309/310 dependencies, not the superseded engine.

### 4.5 Policy applicability, transfer and feasibility

Native first-wave efficacy still needs adapter admission applicable to both actual policies, or preregistered justified reuse of identical bridge semantics. Baseline-only parity cannot silently admit the changed policy. For policies A/B, `b_a=μ_H,a−μ_O,a`, so `Δ_H−Δ_O=b_B−b_A`. Baseline bias alone leaves treatment bias unknown; b_A=−.10 and b_B=.10 can create apparent +.20. Original-runner transfer is a separately admitted design requiring both-policy calibration or runner×policy analysis, registered practical gain δ, calibration budget κ and joint uncertainty allocation. Bounds |b_a|≤ε_a imply error≤ε_A+ε_B; any transfer certificate must require ε_A+ε_B≤κ and genuinely joint-valid `L_H−κ>δ` or `L_H−U_bB+L_bA>δ`. A Bayesian efficacy lower endpoint plus frequentist calibration intervals is **not automatically a joint 95% certificate**. No such mixed certificate is admitted here. Parity does not bound shared oracle bias.

Fixed ceilings are not power or spending authorization. At α=.05 with zero discordance, KL half-width is `1−.025^(1/N)`; N={10,30,150} gives approximately {.308497,.115703,.024293}. At N=150, equal directional counts {1,3,8,38} give half-widths {.042944,.065355,.100601,.190899}. These are attainable widths, not expected agreement. Ordinary independent p=.5 outcomes typically have q+=q−=.25, potentially making narrow parity unaffordable. D2's 300 A/B subjects are not 300 H/O pairs; both-policy/original-runner work, strata/endpoints/instruments/corrections need separate quotation. Six development tasks cannot establish held-out agreement.

Engineering/method admission precedes separately quoted feasibility and one frozen parity tranche, then confirmation. Fix final N before outcomes; no easier-task/method/margin shopping, peeking/top-ups, favorable stopping or pilot relabelling. Infeasibility preserves the unmet central study while independently gated useful-task/receipt work advances. No 89-task catalogue/population extrapolation follows from a selected manifest.

<a id="ac-4-4"></a>
**AC-4-4 (D/R):** The pre-data admission packet names actual participant/policies, dimension map, independence rationale, exact pins/strata/weights/R, δ/harm/precision/parity margins and method/claim scope. Quote plausible disagreement/precision scenarios for every required endpoint, both runners/policies and all costs. Reject baseline-only treatment transfer, mixed-confidence certificates, ceiling-as-power and unset funding. A complete admitted frozen study reports all outcomes and uncertainty; unfavorable results may pass research acceptance, while absent admission leaves the objective unmet.

<a id="s5"></a>
## 5. Useful-task and bounded operational protocols

### 5.1 Two fixed useful-task subjects

**REQ-D-201/202, REQ-A-003/104, REQ-B-002** require selecting one eligible unchanged upstream demo task/oracle before outcomes and executing **two fresh subjects**, fixed configuration/snapshot/order. Retain both; no success-seeking replacements. A genuinely independently correct artifact is required to mark useful-output acceptance, while negative attempts remain valid diagnostics. Any showcase selection is disclosed. Task/submission, native head/watermark, actual model/tool I/O, independent oracle invocation/output/grade, stop, veto coverage and all costs must join.

Sanctioned correct output→pass, plausible wrong contents with an existing file→fail, and broken oracle→oracle failure must differ. Diagnostic derivatives cannot replace benchmark tasks; no correct control means held acceptance. Mutated submission requires new digest/grade. Subject quiescence, oracle isolation/shared access, network and tamper availability must be evidenced. A single fresh rerun or these two subjects awards neither R2/R3 nor scientific efficacy.

<a id="ac-5-1"></a>
**AC-5-1 (E):** Inspect two fixed new subject→effect→sealed submission→executed oracle→grade→settlement chains and all three controls. Confirm no authored reward or fixture substitutes for real task work. Exercise independent procedure controls in §6, preserve incorrect/infrastructure outcomes, actual causal order and externally observed mutation. Distinguish task acceptance from apparatus acceptance.

### 5.2 Operational apparatus and O1–O8

**REQ-D-203/204/205/206/207, REQ-E-012/110/205** require S1's admitted Dispatcher/backend with one fixed **non-idempotent** append of a unique trial/effect marker, flush/digest, denied target/read-only sentinel and independently surviving supervisor. Freeze risk/execution requirements, actual isolation/mounts/network-off/observer access. Supervisor records executor entry, bytes, counts and termination before launch, at the boundary and after quiescence. Existence alone cannot pass. Scratch paths/LocalExecutor alone prove no confinement; unsupported shared backend means unavailable/scope review. No arbitrary commands, invented deduplication or in-memory replacement.

Use fresh store/workspace/marker/sink/approval state each repetition. Eight cells ×three repetitions=24 trials; frozen seeded permutation and balanced controls per block, identical inputs/backend except declared condition. Restart/response phases are not repetitions. Every boundary must be reached/assessable and every case-specific expected assertion must hold for complete operational acceptance. O1–O7 require their stated invariants; O8 deliberately violates the sink invariant and requires detection of all three seeded duplicates, never an invariant-pass label. Partial coverage remains partial.

| Cell | Trigger, observation and expected result |
|---|---|
| O1 allow | Same-path positive permission/reservation/call; one marker and matching observed digest. Supplies confined gate-off comparator for O3 and positive transport path for O6. |
| O2 Kp4 | Commit→injected termination before executor entry; sink delta 0. Restart/new fence/preserved prefix produces unknown/worker_lost+probe; repeated restoration adds no blind redispatch/duplicate obligation. |
| O3 deny | Monitor reached; deny/refused before prepare/commit/entry, no mutation. O1 demonstrates the ungated path was reachable. |
| O4 permission allow | Actual pending→suspend→host answer→wakeup/resume on the same effect; one mutation, full negative/restart controls below. |
| O5 permission deny | Actual pending→offered deny→refusal; no prepare/commit/entry/mutation; conflicting later answer cannot authorize. |
| O6 reservation refusal | Real Account has zero remaining model_calls; attempted reservation refuses before independently observed transport entry and dependent mutation. This is not terminal exhaustion. |
| O7 exhaustion/drain | Already committed effect waits behind bounded barrier; actual time.wall_ms reaches Account bound. Block new dispatch, drain to observed/unknown, then exhaustion/stop. No later marker; committed work was not prevented retroactively. |
| O8 seeded duplicate | Confined diagnostic executor appends twice despite a success report; observer detects duplicate. This is detector sensitivity, not a passing invariant. |

Kp4 proves classification, not completion. Read-only instrument probe records applied/not_applied/undeterminable with admissible evidence. External sink-zero cannot authorize native settlement or retry; without admissible probe, keep unknown/no blind retry. Exclude Kp5/Kp6, power/filesystem faults, concurrency and hostile observers from this claim.

O4 requires two observer intervals with no prepare/commit/mutation; reject unoffered answer; offered more_info remains pending if available; restart preserves pending; authority-checked allow-once wakes the original effect. Same answer/key is idempotent; conflicting new key refuses AlreadyDecided without another grant/mutation. Retain offers/authority/causal IDs. Protocol `input_required`/`resume_paused` is distinct and insufficient because its effect may already be committed. O5 checks deny.

O6 uses a deterministic diagnostic transport with independently counted entry and the identical O1 positive path; it proves local/fixture plumbing, not live usage/cancellation. O7 uses real root and separate instrument budget, elapsed origin across restart/approval, grace=0; proposed 5 s deadline/1 s observations/10 s fail-safe, bound before execution. Supervisor release needs no new effect. Expiry before commit is unreachable, not a pass. Rebuild Account root/child holds, source/deduplicated charges and stop; repeated reconciliation cannot double charge, and physical use survives reversion.

### 5.3 Counts, limits and verifyfixed

Per cell record P planned→A invoked→R boundary observed→T fully observable, V violations and expected verdicts. Publish A/P, R/A, T/R, V/T; zero denominator is undefined. With U=P−T, `[V/P,(V+U)/P]` is a planned-slot missingness bound, not a confidence interval. No pooling fault/phases/retries/events. O8 separately reports detected/3 seeded duplicates. Retain tier-off/refusal/unreachable/missing-detector/timeout/oracle-failure/unknown/cancellation. At most one setup diagnostic rerun per cell within cap, new identity and no replacement. Failed negative control blocks acceptance. Integrity/confinement/unexpected destination/resource breach stops immediately; otherwise fixed completion/cap, never until green. Zero violations means 0/T named opportunities on that host under those assumptions.

Prospective operations envelope is one subject, ≤2 cores/4 GiB, ≤2 wall-hours/4 core-hours/3 GiB new disk, $0 provider/cloud, ≤90 s/trial, with admitted prebuilt binaries/backend and small licensed inputs. Missing prerequisites are unavailable; no builds/downloads/installs are hidden in this envelope. Optional prebuilt plugin adds ≤30 min/1 core-hour/1 GiB (aggregate 2.5 h/5 core-hours/4 GiB). Clocks/disk/child cleanup must be enforced. These remain execution-quote ceilings, not present permission. Two S1 model runs and edits require separate subject/instrument funding.

**REQ-D-208/209** connect A3's R0 expected-finished/derived-succeeded drift to preserved baseline and repaired-revision reproductions plus six R0 controls. Missing historical bytes permit only a labelled diagnostic equivalent with differences; setup failure is not verifyfixed. Optional package acceptance is §8.2.

<a id="ac-5-2"></a>
**AC-5-2 (E):** Complete O1–O8 ×3 with independent entry/bytes/digest/sentinel and native prefix/fence/permission/Account evidence. Reconcile stage counts, invariants, unknown/probe and seeded detector sensitivity; preserve partial trials and caps. Test invalid/duplicate/conflicting answers, tier-off, repeated restoration, charge reconstruction and no later dispatch. No population reliability/exactly-once/containment claim may exceed this matrix.

<a id="s6"></a>
## 6. Native evidence, recipient procedures and rights

### 6.1 Profile and immutable coordinates

**REQ-F-001–008, REQ-E-208** (union of REQ-E-010/108/109), **REQ-A-101/103/105** require `evidence-package/1` as a typed member/role profile **inside `hh-bundle/1`**, owned by bundle codec/validator. Identity/wire retain `idp/1`/canonicalization, bundle version_id identity and archive transport separation. Existing ledger/experiment/bench/budget/results/analysis authorities produce facts; the profile is a derived index, not a second authoritative store. New domains/events/kinds need admission and one home/class/codec.

| Field/member group | Required content and identity |
|---|---|
| Profile/source/claims | Required even when partial: version, code commit and dirty-patch member or dirty unknown, runtime/toolchain/features; claim ID/text/scope/class/population and exact native/report refs. |
| Protocol/phase/plans | Study preregistration/task manifest, phase, task/split/stratum/arm/replicate/full factors; demo uses bounded case manifest. Frozen slot set/hash/count, close reason/watermark. |
| Attempts | Native plan/attempt/launch binding, subject if launched, ordinal/predecessor/reason/settlement. Prelaunch refusal has experiment fact, no fictitious subject. Date/leg filename/provider ID is not attempt identity. |
| Grade attempts/selections | Instrument event/run, sealed subject/submission digest, oracle/version, start/end/result, ordinal, frozen selection and supersession/revision refs. No extra subject n. |
| Native heads/watermarks | `{run_id,seq,event_id,hash}` for every included subject/experiment/instrument/overlay/lineage prefix and projection watermark; exported last event agrees. Later appends do not change snapshot. |
| Pins/raw task/oracle | Suite/task/source/split/validity/contamination and three task surfaces; image digests, runner/platform/dependencies, fault/network/isolation/cache, immutable submission; executable invocation, stdout/stderr/reward/error/exit/timeout, mapping and access/veto evidence. |
| Models/treatment/usage | Requested/served models/fingerprints, roles, seed support/material, routing/fallback/cache, config/registry/variant parameters; trigger→selection→actual request/response/call joins; subject/search/instrument budgets, MatchSpec, formula/prices/confidence, primary reservations/actual/failed/unknown usage and physical outlay. |
| Rows/analysis | Row versions, overlay watermarks, AnalysisSpec/record/report, executable/code/locks, registry/oracle/prices, selected/excluded slots/runs/grades, raw observations/numerators/denominators/reducers/weights, estimand/units/direction, estimator/version/assumptions/prior/seed/actual draws/floors/refusals/levels/bounds/tails/margins/full frozen families. |
| Operational evidence | Planned/reached/observable fault/attack opportunities, actual injection/permission/exhaustion and independent sink, duplicate/missing/unknown effects, assumptions. |
| Access/history | Required supersedes/losses/access policy (empty lists allowed), rights/recipient/retention/owner. Optional build-note bytes/revision/digest labelled as build-note, never native head; legacy receipts remain exact bytes. Optional illustrations/foreign exports link native evidence and losses. |

Declarations precede dispatch durably, with causal order and available trusted checkpoints. Self-reported time/hash is not authenticity. Create-if-absent identities reject conflicting bytes; latest pointers are derived. Regrade/correction/redaction/export retry creates a new snapshot, never rewrites old evidence or increments n.

All-planned closure reconciles **set equality** of frozen slots→plans→launches/first and all attempts→settlements/selected grades→rows→analysis. Retain not-run/refused/pending/orphans/deviations, no retrospective shrink. Per slot/attempt/subleg separately record prerequisite available/unavailable/unknown; invocation not_run/started/finished/unknown; exit/signal/timeout; semantic pass/fail/refused/unsupported/skipped/unknown with reason/detector/time/evidence; native lifecycle/stop/outcome, raw reward, veto/availability and grade revision. A process pass cannot erase a format failure or skipped oracle. Per metric keep membership/reasons, s/f/u/M, weights, bounds, censoring and undefined/no-detector states.

### 6.2 Typed recipient closure

Traverse claim/procedure roots through native heads/manifests/pages/causes/lineage, ordinary refs, IR refs and **owner-typed payload refs**, including response bytes, sealed submission/grade/usage/selection/rows/analysis. Owning codecs enumerate references; no hash regex or second class table. Use visited deduplication with parent/role/field/media/size and explicit traversal limits. Unknown relevant schema or exhausted limit fails closure.

Every required edge resolves to verified member bytes or explicit fetch/redacted/gc. `present` requires recipient bytes, not a producer assertion. Fetch requires exact identity, allowed locator/trust/expiry/rights and verified arrival under existing durable rules; it never executes. Missing, wrong, expired, unauthorized or unsupported fetch fails/refuses; no producer home/cache/path/credential fallback or secret-bearing URLs. Report bundle/profile/procedure/recipient/time, required/resolved/withheld/unresolved, verdict/failures. A sidecar cannot raise R levels. Static completeness differs from recipient procedure closure.

<a id="ac-6-1"></a>
**AC-6-1 (E):** Regenerate profile from authorities; verify after producer stores/caches/credentials are inaccessible. Exercise same-day attempts/regrades/export collision, removed slot/orphan/duplicate grade, pending/skip/format failure, stale watermark/build-note head substitution, missing ordinary and payload-only blobs, wrong/expired/unauthorized fetch, unknown profile, traversal limit, hierarchical/reverted/unknown costs, secret export and redaction/GC. Require exact all-planned membership and typed losses; never infer unseen executions or grade truth.

### 6.3 Procedure cards and comparison rules

**REQ-F-201–206, REQ-D-202, REQ-A-005/204, REQ-B-104** keep noncumulative R levels, evidence class, static maximum/requested/claimed/achieved level, completeness, native verdict, recipient closure, scientific admission and independence separate. Acquisition/inspection never executes bundled code. Until implemented/tested, printed recipes are `not_run{recipe_unimplemented}`; no new `hh recompute`, `hh replicate` or `hh bug-bundle` command is invented.

Every card states job, version/pins, inputs/rights/losses/expiry, prerequisites/setup/network, exact supported recipe/argument files, expected fields/tolerance, ceilings/cost, negative controls, exit **and semantic verdict**, independence and report route. Acquire a pinned tool release or separately build from locks and record binary/source/toolchain/platform/signer. Empty recipient store/workspace has no producer fallback. Verify/recompute/replay disable network; reject archive escaping paths/links and excessive expansion before execution. Identity mismatch stops; missing rights/inputs are unavailable/refused.

| Procedure | Required work and limit |
|---|---|
| Inspect | Locate accessible claim/event/adverse result and scope; does not establish integrity/truth. |
| Verify / optional R0 | Native staged identity/schema/chain/population/closure checks. R0 adds full native validation with lifecycle `open` absent finish, otherwise `finished`, separate from reward/stop/veto. Six cases: successful→finished, failed→finished, open→open; changed bytes, missing member and re-sealed wrong lifecycle fail. Task failure can validate. |
| Recompute | Verify inputs; reconstruct frozen grade selection; reconcile planned slots; rerun pinned analysis; compare counts/membership/weights/unknown bounds/raw and joint veto outcomes/estimates/intervals/refusals/full families and prices. Arithmetic agreement does not validate hidden history/oracle or confer R1. |
| Replay / optional R1 | Disposable source copy with inventory of prefix/head/derived objects. Disable provider/effect execution and independently count attempted dispatches/observe sink. Exact declared decision/assembly and original prefix agreement; replay markers may append. R1 additionally deletes required derived objects only in the disposable copy, rederives each required hash from authoritative prefix and replays where declared. Missing input, altered derivation/variant or any attempted live work fails/refuses, even if transport blocks it. |
| Fresh / optional R2 | New subject/calls/cost/grade; R2 additionally requires pinned configuration/models/environment, recorded seeds, validators, preregistered margins/minimum n and all native basis. Reproduction instrument charging retains subject usage attribution. |
| Fresh / optional R3 | Registered seed distribution/design, n≥k, MatchSpec, admitted equivalence/margins, observed fingerprints when unpinned; one fresh demo awards no level. R3 is not cumulative R2. |
| Independent replication/transfer | Distinct installation/signer plus actual outside relationship, assistance/authority/attestation. New phase/attempts, fixed comparison criterion and target; own D2 protocol or D3 eight×three apparatus. Own-task/new-model transfer is a new question. Same-author directory/key, matching signs or overlapping intervals proves neither independence nor generalization. |

Identities/native derived hashes, membership/order, counts/missingness and categorical verdicts agree **exactly**. Portable scalar tolerance is frozen before execution: `abs(new−reference) ≤ 1e−10 × max(1,abs(reference))`, using full precision before formatting. Same pinned implementation/seed is reference; preserve numerical-error records. Nonfinite values or changed threshold/verdict fail even inside numeric tolerance. This is not a scientific margin or permission to change seeds/draws. If a method cannot meet it, review a justified revision before recipient execution, never widen after mismatch.

Public rows may support arithmetic while inaccessible submission/oracle/wire prevents independent truth/fidelity checks. Withheld native→public mapping means that join is publicly unverifiable; no guessed sensitive hashes/prompts/solutions to fake closure. If row inputs cannot be cleared, real-data public recomputation is unavailable; synthetic teaching cannot fulfill it. Native replay is recorded behavior, not free fresh solving.

<a id="ac-6-2"></a>
**AC-6-2 (E/P):** Test each advertised procedure independently after producer cleanup, including six R0 cases, wrong/missing selectors/weights/prices/draws, numerical threshold change, changed derivation, attempted live dispatch, retired model/service and withheld/sanitized inputs. Exact verdicts survive expected refusals. One consenting outside reviewer must actually complete both promised verify/recompute procedures; a blocked/negative report is a valid attempt/contribution but portability remains unmet until repair/recheck. Neither author-only testing nor a declaration awards independence.

### 6.4 Rights, retention and recipient resource ceilings

**REQ-F-006/207/208** select permitted public index/code/analysis plus encrypted controlled staging, subject to item clearance. For each item record source/version, license/consent evidence, recipients/uses, owner/decision and expiry. Unknown rights=withheld; no secret-bearing bundle. Held-out solutions/oracle material/model I/O default restricted. Inspect public numerical rows and sensitive/low-entropy hashes too. Sanitization changes IDs with transform/version, permitted mapping and exact per-procedure loss; changed bytes cannot retain native hashes/R1. Expiry/GC/deletion emits permitted superseding status/tombstones and disables dependent procedures, even after earlier passes. Shorter rights override retention honestly.

| Scope | Selected policy; activation inputs still required |
|---|---|
| Public review package | Existing Git for permitted metadata/code/recipes; assets ≤512 MiB each, package ≤2 GiB compressed/8 GiB expanded, first-wave aggregate ≤10 GiB. Program caps, not hosting entitlements. Reports/metadata five years, assets two years from publication; review 90 days before expiry. |
| Controlled review | Encrypted local staging and separately encrypted backup within approved quote. Selected review originals ≤20 GiB, retained 12 months from collection closure, with actual dates before invitations. |
| Optional hosting | Only after operator approval: private versioned S3, public access blocked, SSE-S3/TLS, opaque metadata/recorded region, ≤32 GiB including versions/logs/derivatives. No mandatory account/runtime dependency. Quote storage/requests/egress/logs/retention; quotas are not billing hard stops. |
| Access | Named maintainer/private contact, target response five business days without service promise; record member/procedure/eligibility/rights/uses. Read-only grant ≤30 days renewable; object/version links expire after one hour, no write/delete/list rights. Bearer URLs stay out of bundles/logs/reports and may expire early; downloaded copies cannot be recalled. |
| Full archive | Review quotas do not cover pilot/confirmation originals, failures, versions, backups and operational artifacts. Quote full 50 GiB pilot/200 GiB confirmation envelope plus overhead separately before collection; reserve capacity or revise scope, never discard adverse attempts to fit. |
| Recipient prebuilt procedure | ≤2 wall-hours/4 core-hours, 2 cores/4 GiB RAM, 12 GiB scratch including ≤2 GiB download/8 GiB expansion, $0 provider fees. Setup separately ≤2 hours/8 core-hours, 4 cores/8 GiB, 20 GiB disk, tested recipe required. Larger controlled review separately quoted. |

Use authorized manual materialization plus native identity verification when native fetch cannot authenticate the selected store. No transparent-fetch claim is implied. Review renew/delete at expiry including noncurrent versions/backups; a delete marker is not deletion. Retain only permitted loss metadata. These are unmeasured ceilings, not performance or service guarantees; stop/requote on breach, no automatic install/pull/fallback. Local power/time/network/storage still cost resources. Fresh replication is separately scoped/funded; unused pilot allowance grants nothing.

<a id="ac-6-3"></a>
**AC-6-3 (D/E):** Before collection/distribution, bind rights/owner/contact/host/region/quotas/actual dates/full archive quote. Exercise expiry, denied/unauthenticated access, sanitized derivative and noncurrent-version deletion/loss cases. Unknown rights or lost required bytes must withhold the affected procedure/claim; controlled closure cannot become public closure. Preserve original/derivative linkage only when permitted.

### 6.5 Legacy evidence

Preserve every `hh-live-evidence/1` byte and ADR-0363 meaning. New views label legacy `run_ledger_head` as a build-note snapshot when that is its actual identity, not native WAL. Later native linking/collector changes need explicit amendment. No reconstruction of missing attempts, upgrading of old class or relabelling historical failures. Unknown mandatory profile/major refuses new claims while preserving legacy inspection; optional additions cannot strengthen old evidence. Native hashes support audit consistency, not external truth. Public availability, R level and live/fixture/foreign evidence remain distinct.

<a id="s7"></a>
## 7. Executable examples, documentation and maintenance

### 7.1 Reader routes and actual trace

**REQ-G-001–008, REQ-A-002/201/202/203/205, REQ-B-003** reuse README, getting-started, reference, architecture, status, docs map and CONTRIBUTING anchors. Propose three future pages named `docs/inspect-a-run.md`, `docs/experiments.md`, `docs/evidence.md`; they are not existing links or deliverables of this draft. Entry choices are **run a useful task**, **inspect a recorded result**, **learn offline**, with prerequisites/readiness visible first. No model/environment/budget means a labelled receipt/fixture path and unmet useful-task milestone. Experienced integrators may enter reference directly. Failure/refusal/wrong output leads to troubleshooting, trace or a sanitized report without full successful setup.

Every recipe card is job→version/prerequisites/cost/network→input schema→exact commands→semantic checks→failure route→next choice. Explain durable run versus session IDs, scratch store/workspace roots and AssemblySource versus bare HIR. Repair selected CLI forwarding with regression tests or use one supported recipe; keep semantics in owners. No manual RPC/source archaeology requirement on the primary route; no unsupported help/command shapes.

Six annotated excerpts must resolve to actual receipt/event IDs/sequence/payload/causes: (1) run/definition/task/submission, (2) real action/model relation, (3) effect intended and permission/rule, (4) full `action.effect.authorized/prepared/committed/observed` sequence and observed bytes, (5) lifecycle finish separate from oracle/grade/veto/failure, (6) context/replay with original prefix and allowed added markers. A3's fixture had tool proposal before model completion; do not invent universal ordering or nonexistent `action.effect.decided`. Real useful-task excerpts must come from that task, not relabel the internal `hh.submit` fixture. One editable supported cap/permission field must validate/compile, demonstrably be consumed/enforced, and rerun as a new attempt with both configs retained; this is adaptation, not causal efficacy.

Screenshots are optional illustrations of the same checked receipt/commands, at most terminal/context and comparison, with version/class/outcome caption, equivalent text and raw output. No new viewer. Evidence cards expose exact dated scope, admission/limitations/access and available/not-yet-available/restricted/unsupported with reason; current useful-task, teaching, R0 and real-study readiness remain independent.

### 7.2 Four example families and semantic controls

**REQ-F-101–105** specify:

| Family | Required execution and negative controls |
|---|---|
| EX-01 | Scripted quickstart, attach by returned run ID, six actual excerpts, replay, native export/verify/R0. Fixture label; connectivity ok and exact IDs/semantic reports. Binary/schema mismatch, unresolved excerpt, tamper/missing/lifecycle mismatch fail; exit zero is insufficient. |
| EX-02 | Two tasks×two arms×five repetitions=20 subjects. Actual register/expand/open/next/claim/launch/settle/close, projected rows and persisted/recomputed analysis. Pin binary sequences yielding task1 A=1/5 B=4/5, task2 A=3/5 B=2/5. Show unequal nondegenerate contrasts without requiring significance. Same inputs keep report identity; changing one observation changes result. Missing spec/row, bad lease/incompatible arm and duplicate settlement controls; one-task/missing-row variants are separate refusal/availability controls. Constant-data controls are method-specific: all-success/all-failure binomial cells retain valid Beta/KL uncertainty; a genuinely undefined bootstrap/other estimator reports that reason. Zero observed variance alone never forces a valid binary method to refuse or return a zero-width interval. |
| EX-03 | New useful task with sealed submission/unchanged executed oracle and consumed-field edit, plus EX-01 receipt. Wrong output→task fail; oracle crash/timeout→oracle failure/unavailable; unsupported edit refuses. Live adds authorized provider/resources/ADR-0363 records. Prerecorded inspection is not execution. |
| EX-04 | Shared confined sink/effect for allow/Kp4, deny, actual pause/respond/resume and exhaustion, with §5 full matrix and honest unknowns/tier refusals. Input-required is not approval; scratch is not sandbox. |

EX-02's exact informative comparison must use an already-admitted method verified on that path or the necessary early P8 method/analysis slice. P4 cannot publish using a known unsupported/unlabelled estimator. This prerequisite is independent of live parity, providers or study collection; package labels do not force an artificial P4-before-all-P8 order.

Metadata/recipes/tiny fixtures/checkers may later live under `scripts/examples/`; owning crates retain codecs/checks. Each case pins commit/dirty patch, Rust 1.94/rustfmt, lockfile, OS/features/binary/schema, fixtures/definition/registry/variant/checker/AnalysisSpec/seeds and task/model/price identities where relevant. Retain actual command/input, prerequisite/invocation/exit/signal/timeout, semantic verdict/reason, costs and native receipt. Expected refusal can pass an assertion while operation remains refused. Required positive skips block readiness; optional skips award no evidence class. Normalize only declared time/temp-path display variation, retaining raw evidence and exact grades/enforcement/losses.

<a id="ac-7-1"></a>
**AC-7-1 (E/D):** Run each advertised exact recipe from scratch on its supported pinned setup, with semantic and negative controls. EX-02 joins all 20 slots/launches/settlements/rows and an admitted persisted report; fixture edit changes results. The selected request-shape regressions fail on baseline mismatches. Every trace excerpt/field edit/claim/procedure link resolves. Missing positive prerequisites block the card; fixture success never satisfies S1/S3.

### 7.3 CI and refresh

**REQ-F-106/107/108** require executable doc drift checks and separate code, freshness and study revision policies.

| Lane | Bounded future execution |
|---|---|
| Static both modes | ≤60 s/OS, ≤20 MiB output, no build/network: links/anchors, displayed-versus-canonical command blocks, checker/pins/annotations/claim/procedure mappings. Missing/stale fails. |
| Full offline | One Linux leg ≤5 min after existing build, ≤1 GiB scratch/50 MiB diagnostics: EX-01/02 plus admitted deterministic EX-03/04 controls, no credentials/services. |
| Platform | Existing Linux/macOS owner/tier batteries; added cases ≤2 min/leg after build; support/refusal assertions. |
| Quickstart | Existing `bash scripts/check-quickstart.sh` reads **committed HEAD** in a clean clone; uncommitted README static review does not execute edited examples. Build time separate. |
| Live | No routine PR calls. Separately authorized local/protected leg/caps and ADR-0363 subleg records; unset allowance zero, skipped prerequisite not a pass. |

Only `docs/` and `research/` are ADR-0362 docs paths. Canonical executable inputs/assertions stay outside them. Both-mode checks compare displayed/canonical blocks and mappings; full execution reads canonical inputs without transitive committed docs/research reads. After `id: path`, any direct/indirect committed-content reader there carries `# ci-mode: both — <reason>`; all others use `if: steps.path.outputs.mode != 'fast'`. Do not evade the read classifier or silently run expensive docs execution. If necessary, design bounded both-mode execution or explicit CI revision. Validate with `bash scripts/ci/check-ci-gating.sh .github/workflows/ci.yml`; preserve anchor pair placement and maintainer/gate full-dispatch cadence.

Code/schema/runtime/checker/toolchain/feature change triggers scoped regressions and affected offline recipes. Artifact/access/expiry/redaction triggers new closure/loss/supersession and disabled dependent procedures. Model/provider/price/environment drift makes fresh-run claims stale and permits only an authorized diagnostic proposal; connectivity does not renew useful-task/study claims. Oracle/grade/method/cohort errors follow §2.2 corrections; material changes require new protocol/cohort, not documentation refresh. Preserve historical negative results. At release and 30-day launch metadata review, live-dependent “current” cards older than 30 days become **refresh due**, not false and not permission to spend. Historical findings do not expire. No automatic collection/schedule is created; launch cadence ends unless separately adopted.

<a id="ac-7-2"></a>
**AC-7-2 (E/D):** Docs-only changes with stale canonical examples fail static checks; recipe/code changes select full execution. CI gating lint passes exact path modes. Report committed-HEAD versus working-tree verification honestly, timeouts/diagnostic reruns/partial results retained. Changing access disables affected procedures; changing study input cannot silently recollect or upgrade evidence.

<a id="s8"></a>
## 8. Contributions, outside review and publication

### 8.1 Small contributions and verifyfixed

**REQ-G-101–107, REQ-B-004, REQ-A-205, REQ-D-208** reuse CONTRIBUTING, existing issue/PR forms and private security reporting. Add short report/reproduce, check-receipt and optional packaged-component routes when ready. Ordinary contributors can report failed installation without a binary/run/bundle/paid account or prior issue, make focused changes with applicable checks and open a PR. They need **no internal Q register, ledger or manifest ritual**. Maintainers own program traceability and human merges; consequential dependency/contract changes still follow ADR/amendment rules. Later reconcile CODEOWNERS' broad manifest wording with the ordinary-contributor policy without weakening governance. Existing MIT OR Apache-2.0, no DCO/CLA, conduct/security policy remains.

A no-run report includes revision/dirty state, OS/architecture/toolchain/features/backend/prerequisites, command, expected/actual output, exit/signal and sanitized stderr; doctor only if usable. Existing-run reports add attempt/config/input/event/grade/procedure IDs and expected/observed semantic fields, plus permitted receipt or access/loss reason. Inspect logs for secrets/signed URLs/private paths/model I/O/solutions/oracles/rights; attach only cleared data, with synthetic reproducer limits. Native export remains the receipt, report a cover note. Use actual returned bundle locator and a tested supported recipe; no fictitious bug-bundle interface.

Verifyfixed pins original revision/input/recipe/rights, copies allowed evidence into scratch, preserves the failure and obtains comparable candidate-revision evidence. A regression must fail without the fix. Record corrected predicate, old/new IDs, checks/counts and any input/backend changes. Unavailable prerequisite=unable to reproduce, not fixed; reporter/maintainer/outside verification are distinct. R0 uses all six controls and preserves A3's original drift. Grade/analysis correction follows §2.2; later pass cannot rewrite failure or create another replicate. Source tests cannot earn `live-executed` without ADR-0363 collection.

### 8.2 Optional component and starter ownership

At most one existing `hharness-compact-evict-oldest` source exercise; variant/shared fixture runtime are not crates.io packages. Require Rust 1.94/rustfmt, lockfile/Git/C toolchain, compatible Unix helper/backend, separate provisioning/build costs and network prerequisites. No Windows or no-compilation promise. Build→seal canonical package→load/recompute pins→helper spawn→bind compaction_strategy 1.0→assess/propose/execute four 100-token items at cap 200→check required/evicted/remaining≤200 and host-stamped external provenance→close. Tampered binary and wrong pin must yield `PinMismatch`, never implicit reseal. Changed binary requires intentional new sealing. Missing helper is unavailable/refused; isolate all resolution fallbacks, not just one environment variable.

The supported implementation recipe will exercise existing package APIs rather than invent CLI seal/load/invoke commands. Source-checked prospective package checks are `cargo build -p hharness-helper -p hharness-compact-evict-oldest`, owning package tests, varhost package battery and `cargo fmt --all --check`; they are not executed or published runnable by this draft. Existing early-return tests need actual availability and invocation/provenance phase evidence; required positives fail on missing prerequisites alongside deliberate missing-helper negatives. Compare kernel hashes before/after only to establish no-kernel-rebuild, while disclosing plugin build. Restore helper after tier-off tests. No third arm or provider-treatment/efficacy claim follows from ABI conformance. Defer if it competes with S1/receipt readiness.

Starter mapping: setup/failure prose→docs/examples owner plus living-doc checks/affected recipe; CLI/R0 mismatch→CLI/embed plus failing semantic regression; corrupt/missing member→bundle tests and no fallback; compaction boundary/prerequisite→compaction/varhost batteries; receipt mismatch→bundle/analysis with pins/tolerances/losses. Schema/tier changes add owner codegen/drift/removability checks; full CI dispatch/live/build-memory duties are not universal external PR prerequisites. Foreign corpus claims name exact version/arms/missing coverage and request/notification/callback outcomes; conversion/conformance does not imply a full alternate runtime.

<a id="ac-8-1"></a>
**AC-8-1 (E/D):** Walk no-binary, failed-export and restricted-input report paths; verify reproducible predicate and ownership without internal approval ritual. For a fix, regression fails on old code and comparable new evidence passes, originals retained. If optional component is offered, observe every build/seal/load/bind/invoke/output/provenance phase and all three adverse cases; early return is not a pass. Maintainer mapping/human merge and privacy/security routes remain intact.

### 8.3 Publication stages and report spine

**REQ-G-201/202/203/207/209** require this order for advertised claims: internal exact recipe/negative/closure/rights readiness→one outside clean verify/recompute→4–6 formative reviews and severe-defect repair/recheck→operator capability-publication decision→separately gated scientific report→manual day-30/day-90 value review. Capability publication may precede scientific results but leaves the central study unmet; sound null/negative/inconclusive admitted research can publish. No fixture, hash, tag or distribution creates a scientific/public evidence badge.

A release note states audience/job, change/revision, try/inspect prerequisites, earned/unavailable claims, evidence, report/contribute route and correction owner/date. A report states question/decision/scope/class/date and claim→receipt; protocol/deviations/population/arms/assignment/oracle/pins/caps/admission; planned→attempt→grade→analysis counts and adverse states; effects/uncertainty/margins/families/cost or operational verdicts; procedure cards/outside reports/assistance/access/losses; limitations/unsupported conclusions/corrections/rights/retention. Capability sections mark scientific fields inapplicable, never fill with synthetic efficacy.

GitHub docs/releases/issues/PRs first, then authorized direct review. At most one optional wider channel after current-rule review and operator approval. Accepted G3's Show HN option requires tryable work, human maintainer availability and human-authored submission/comments under the inspected no-generated/AI-edited/no-automated-posting rules; recheck before use. No report-only launch, scraped lists/bulk mail, vote solicitation, promotional issues or implied endorsement. Upstream engagement needs a concrete failing compatibility fixture, loss map or inspectable report plus current contribution rules and separate outreach authorization. No outreach or automation is authorized here.

### 8.4 Formative observation, rubric and denominators

**REQ-G-009/204/205/206/208, REQ-B-005/006/007** select one consenting outside verifier plus 4–6 technical evaluators. Overlap is allowed with familiarity disclosed and people counted once. Seek roughly half harness researchers/engineers and half integrators, at least two new to HarnessHarness, one receipt-first and one missing-live-prerequisite route; report coverage shortfalls. Convenience volunteers cannot estimate population adoption or causal improvement.

Outside verify/recompute uses §6 clean installation/signer, no producer fallback, network disabled and separate verdict/cost/assistance/relationship records. A negative/blocked attempt counts as contribution, not portability pass. Fresh replication requires a separate admitted funded protocol; ordinary unsolicited local use of published material needs no Q approval.

**Shared participant acceptance dependency:** REQ-B-107 and REQ-G-001/003/005 require the relevant AC-8-2 walkthrough, navigation, event explanation and procedure-choice records, in addition to their technical/document checks. REQ-A-201 and REQ-C-007 reuse the applicable first-use/report records. Freeze per-ID/branch observation coverage before sessions; identify the shared record, assistance, result and any unmet predicate. This adds no cohort or duplicate execution. A technical slice may complete while these participant predicates remain unmet; its pass cannot close the full requirement or claim comprehension/usefulness. Each adopted requirement retains one primary owner, with a dependency on the common participation evidence.

A formative session is ≤60 minutes total: approximately 5 consent/context, 40 participant-led tasks including setup, 15 interpretation/debrief; retain unfinished tasks at cap and separate prior install time. Optional consented recheck ≤15 minutes per affected participant retains original outcomes. Observe unaided first; record first assistance time/type (hint, corrected command, environment fix, author execution). Assisted completion/explanation stays distinct. Unsupported/unfunded live branches stay receipt/fixture-only with task milestone unmet.

Freeze rubric/version/scorer before sessions: (1) runtime executes/records while Lab plans/analyzes; (2) terminal success differs from independent grade; (3) verify/recompute neither solves a fresh task nor proves efficacy; (4) named rights/access/limitations constrain claims. Each asked item is correct/incorrect/unclear; not-asked is separate. Preserve answers/disagreements and original scoring when rescored; no post-outcome rubric improvement. Tasks also locate an adverse result/consequential event, explain or make the bounded edit, choose a procedure, find a no-run report route and prepare a precise sanitized example report (actual submission optional), and describe usefulness/rejection/own job. All remain within the same consented cohort/session ceiling; unfinished branches stay visible.

| Measure | Denominator and interpretation |
|---|---|
| Visibility | Dated/source-labelled aggregate views/downloads/stars, if available; interest events, not unique users/conversion. |
| Access | Route starters/consenting cohort; missing prerequisites/entrants, receipt/live branches separate. |
| Useful task | Independently correct artifact completions/all task entrants **and** /attempts; unaided/assisted/failure/timeouts visible. Fixture/setup excluded. |
| Understanding/procedure | Correct/all asked; verify and recompute verdicts/attempts separately, including unavailable/mismatch. |
| Independent own-task use | Voluntary adapted own-task use/eligible follow-up cohort and /respondents; self-report versus permitted inspected receipt distinguished. |
| Repeat use | Later purposeful session/eligible cohort and /respondents; exclude facilitator recheck/smoke, retain job/window/assistance. |
| Contribution | Distinct outsiders and submitted/actionable/reviewed/accepted/unresolved counts, type and reviewer effort; negative reports count, acceptance is not merge authority. |

Record invited only if authorized, opted in, started, prerequisites, stage attempted/completed unaided/assisted/failed/refused/timed out/not attempted/withdrawn where retained, version/branch/familiarity/platform. People and retries differ; show x/n and censored unfinished setup/active time, never successful-only averages or one conversion rate. Missing follow-up is unknown, not no use. Suppress identifying tiny strata publicly.

One exposure risk, false scientific interpretation or broken advertised command stops affected promotion until repair/recheck. Navigation friction seen twice prioritizes repair, not prevalence estimation; one reproducible defect may suffice. If all task entrants are blocked, repair readiness before more recruitment; receipt understanding cannot replace task execution. Manual review at day 30 and 90 after capability publication yields continue/repair/narrow/stop with evidence/unknowns/owner. At most one optional request per checkpoint to prior consenting participants, no chasing. Total maintainer ceiling ≤24 hours includes preparation/sessions/triage/follow-ups/reviews; record actual, exhaustion=incomplete/requote. No paid recruitment/provider/new tooling or schedule follows.

Before recruitment name purpose/tasks/time/cost, voluntary stop/withdrawal, fields/uses/access/storage and actual publication/deletion dates. Notes by default, no recordings/background telemetry. Recording, attributable quotes and follow-up need separate opt-in; refusal never affects support. Keep contact mapping separate from pseudonymous restricted notes; no private task/model contents or credentials for usability. Delete contacts/raw observations 30 days after day-90 review or earlier withdrawal where possible, actual dates disclosed before consent. Only consented non-identifying summaries publish; public copies cannot be recalled. Scientific receipt retention is separate. Correction owner preserves original failure, appends dated affected claims/revisions, reruns checks and updates pointers/downgrades/retracts unsupported claims; urgent repair does not wait for scheduled review or fund live refresh.

<a id="ac-8-2"></a>
**AC-8-2 (P/D):** Actual readiness/rights/consent/invitation/publication authority precedes participation. Retain both outside procedure reports, all 4–6 sessions or explicit shortfall, rubric/scoring/assistance, branch denominators, censored attempts, original/recheck and missing follow-up. Severe defects block affected promotion; no traffic/fixture/assisted-only inference of adoption. Verify minimal-data deletion and separate receipt schedule, manual review/cost limits and documented adverse-result correction.

<a id="s9"></a>
## 9. Dependencies, resources, amendments and build handoff

### 9.1 Eight packages and honest sizing

**REQ-C-104–108, REQ-E-210, REQ-H-005/008/009/011** preserve eight packages, not allocated tickets or effort promises.

| Package | Existing owner boundary / exit evidence |
|---|---|
| P1 subject/adoption/grade | Experiment/embed/eval/results; recoverable launch, sealing/pending/selection/revisions and legacy/crash acceptance AC-2-1. |
| P2 task/effect/oracle | Shared backend feasibility first, then environment/Dispatcher/bench; real unchanged task, correct/wrong/broken controls and native sink/probe AC-5-1/2. |
| P3 receipt/R0 | Bundle/producer owners; immutable profile, typed all-planned closure, loss/recipient/R0 controls AC-6-1/2. |
| P4 recipe/lesson | One recipe, selected CLI forwarding, fixtures/reader semantics/CI; AC-7-1/2, with early analysis prerequisite below. |
| P5 operations | Same P2 apparatus plus accounting/receipt; all O1–O8 ×3 AC-5-2. |
| P6 treatment-to-wire | Context/embed/gateway, two native policies, legal groups/restart/fallback; AC-2-2/4-1. |
| P7 four caps | Account/gateway/control; whole-vector derived admission, physical usage/deadline/unknown/dedup AC-2-3. Serves S1/S4 too. |
| P8 detectors/methods/study | Eval/lab/analysis/bench; early admitted informative analysis, then detectors/numerics/parity/frozen study/quote AC-3-1/4-1–4. |

Dependency graph: decisions→P1 interfaces→{P2,P7}; {P1,P2,P7}→{P3,P6}; P3 plus **admitted early P8 analysis slice or verified already-admitted method**→P4 informative EX-02 acceptance; {P2,P3,P7}→P5; {P3,P6,P7,method/parity admission}→P8 collection/report. This is interface/acceptance sequencing, not crate dependency cycles. Offline method acceptance does not require live collection/parity. Split packages by actual coupling, owning files/regression scope and fresh-context headroom; C2's 8–16 slices were illustrative, not a limit or calibrated days. New backend/public interface/dependency/contract/resource scope needs a ruling; routine splitting does not.

### 9.2 Execution gate inputs and resource envelopes

| Gate / owner | Trigger and required binding evidence | Absent/infeasible result |
|---|---|---|
| Method/contract — reviewer/operator | O2/Q-7 then ADR admission of §2/§4/§6 exact semantics; independent derivation/routing/numerics. | No new semantics or inference admitted by drafting. |
| Backend — P2/environment owner | Before backend commitment: unchanged TB2 lifecycle, real mediation, confinement/observer/probe feasibility. | S1/S4 blocked; new backend explicit scope decision. |
| Tasks/oracle/model/environment — study owner/operator | Before activation/pilot/parity/confirmation: exact eligible manifest/split/stratum, runner/image/oracle/model/roles, rights/isolation/network/cold-start/drift handling. | Zero dispatch, no substitute suite/provider/task. |
| Endpoint/independence/margins — method owner/sponsor | Before relevant data: signed dimension map, populations/ranges/applicability, justified independence/reuse, δ/harm/precision/parity/transfer margins where applicable. | Required unavailable dimension blocks equivalence; no guessed margin/transfer. |
| Numerical/routing — eval/analysis/bench reviewer | Before method-dependent collection: known-answer/refusal/heterogeneity/missingness/simulation and actual dispatch evidence. | Metadata-only method support fails. |
| Quote — execution owner/operator | Each activation/pilot/parity/confirmation/replication: numeric currency ceiling/prices, all roles/attempts/regrades/cancellation, CPU/RAM/time/full storage/backups/retention. | Unset funds=$0; finite exposure missing→zero dispatch; no automatic top-up/cloud fallback. |
| Artifact rights/access — named owner/operator | Before retention commitments, collection/distribution: per-item rights/recipients/losses, host/region/contact/quotas/actual dates/full archive quote. | Withhold bytes/dependent claims; no hosting/public closure promise. |
| Participation — maintainer/operator | Before invitations/publication: ready cleared procedures, actual consent/date/retention/effort plan and explicit authority. | No outreach/telemetry/automatic review. |
| Build tooling/readiness — decomposition/tooling reviewer | After Q-7 and separately authorized artifacts/tooling: actual base/chain, coverage/ADRs/contracts/tail, retained tested guard and launch-path evidence. | No product dispatch or reused gate/signature. |

Resource worksheet must compute `N_subject=2TR≥10T` for confirmation, then separately add pilots, parity original/HH runner attempts, retries, reproduction and all instrument/regrades. Price every requested/served role and failure, transport attempt, cache behavior, provider billed/unknown exposure, environment/image acquisition, storage/egress/versions/backups and retention. Do not mechanically divide summed runtime by concurrency or reuse historical spend authorization.

| Prospective stage | Conditional hard envelope, subject to exact quote/authority |
|---|---|
| Offline methods/numerical validation (D2) | No provider/cloud/external benchmark tasks/containers; ≤8 h, 4 cores/8 GiB, 32 core-hours, 2 GiB for this numerical/simulation tranche. Requires implementation authorization. |
| Activation | Separate one-task quote; unset bounds/approval means zero task/provider/container dispatch. |
| Pilot | ≤60 first subjects; each ≤100 calls, 1M input/100k output total tokens, 30 min admitted wall, verifier ≤15 min; ≤2 concurrent, ≤4 cores/8 GiB each; ≤200 core-hours/50 GiB. |
| Parity | No allocated tranche. Admit method, exact required endpoint/policy precision/disagreement design and quote both runners first. |
| Confirmation | After all gates, ≤300 subjects under same individual caps; ≤900 core-hours/200 GiB/14 days. Larger T/R needs new quote. |
| S1/S4/recipient/participation | Separate §5/§6/§8 envelopes; unused pilot capacity cannot authorize these or replication. |

The D2 numerical tranche is not a global P1–P8 build/test budget or an estimate of implementation effort. Builds/provisioning, EX-02’s synthetic workflow and other package tests use their separately scoped authorization, prerequisites and envelopes (including §7.3 where applicable); this clarification grants no extra resources or execution permission.

Fixed sample/final analysis; integrity/treatment/cap/isolation/leakage/drift/ceiling failure stops and retains partial results/costs. No favorable peeking/top-ups or retrospective smaller manifest. Feasibility/precision failure leaves objective unmet with owner/trigger, not waived or null. A useful-task release can proceed through its own gates; omitting S3 still requires explicit scope disposition.

### 9.3 Proposed contract amendment register

**REQ-H-004** requires ADR/manifest admission by operator/maintainer authority, with revisit triggers and generated index updates. No ADR number is allocated here. The landing owner below implements only after that admission; O2/Q-7 review of proposed scope is not an already-landed amendment.

| Scope / exact source sections | Before → proposed after | Owning requirements / landing owner | Compatibility, alternative and admission evidence |
|---|---|---|---|
| Launch/required grade/settlement: §06.3.2.2–2.4 (R-2.10.3), §06.5 overlays (R-2.10.5), §05h.2.3–2.4; ADR-0155/0161/0162 | Terminal-derived final settle alone→opted-in execution-cost settlement once, pending admission, immutable graded disposition/revision. | REQ-E-201–204; experiment/embed/bench/eval/results. | Keep legacy settle/six outcomes and old records; unsupported required mode refuses. Preterminal oracle alternative rejected because it extends M1; preserve M1. AC-2-1 across every consumer/crash window. |
| Effects/prepared calls/derived admission: §§05a/05b/05c/05e and §8.2 | Narrow adopted subject/Dispatcher durable result, retained-view→wire and whole-vector projected derived-cap checks; actual-use/exposure enforcement. | REQ-E-205–207; env/context/gateway/budget/control. | No duplicate lifecycle/serializer/totals; new backend/dependency/public boundary separately admitted. AC-2-2/3, AC-5-2; side-ledger/fixed-counter alternatives rejected. |
| Native closure/profile: §05h.3 R-2.9.3/§10.4 and ADR-0363 collector linking | Existing bundle/legacy envelope→versioned evidence profile, immutable attempt/grade/export links, owner-typed closure and recipient procedure verdicts. | REQ-F-001–008, REQ-E-208; bundle/producer/ontology. | Preserve old bytes/fields/levels, unknown required versions refuse. AC-6-1/2; stand-alone receipt authority rejected. |
| Finite-task efficacy: §10.1 reporting, §05h.2/R-2.10.4, ADR-0158 D1–D3/D8–D9 | Explicitly admit independent task/arm Beta(1,1) likelihood/contrast as labelled finite-manifest Bayesian inference. | REQ-D-107/109, REQ-E-209; eval math/lab metadata/analysis. | No pooled/population/correlated claim, global floor reduction or new fitter/dependency; retain other defaults/refusals. AC-4-2 and method review. |
| Statistical parity grouping: §10.1 replicate factor, §05h.2.1 compare, ADR-0046 CF-095 | Honour-seed-only replicate pairing→narrow preassigned H/O first-attempt grouping under independent-across-pairs finite design. | REQ-D-302/310; bench/lab/eval. | No CRN/same-seed/R2 claim; task pairing and other rules unchanged. Reject unequal/missing IDs/dependence, AC-4-3. |
| Finite-manifest equivalence: §10.1 estimator/multiplicity, §10.2 equivalence, §05h.2.1, ADR-0158 D2/D7/D9 | Explicit directional KL, bounded relative-mean/completion-envelope small-n exception; one all-endpoint conjunction IUT exception. | REQ-D-303–307/310; eval/lab/analysis/bench. | Declared α/η/target/range/unit/margin, retained floors/classes/veto/selected-claim corrections. No credible-as-TOST, raw-unit/ppm or post-outcome CP fallback. AC-4-3/4, including confidence-label clarification. |
| Detectors/analysis metadata: ADR-0077 D4, §§05c/05h/06.4/10.5.1 | Exact overlapping completion→execution-stop joins, repeated deliveries/idempotency keys, denominators/coverage/unbounded intensity; complete frozen family/actual draws/refusals. | REQ-D-003/005/008/106/109; eval/context/lab/analysis. | Old approximations retain names; no count-to-rate relabelling. AC-3-1/4-1/2 and missing-key tests. |

Existing-contract fixes, not automatic amendments: M1 created→finished origin (R-2.9.1), R0 open/finished mapping (R-2.9.3), selected CLI request fields (K-2). If implementation discovers a genuine semantic change, route it separately. Every new event/kind/domain retains one codec/home/class table and durable-before-visible producer/fenced writer. Public schema changes originate in hh-embed-schema→hh-codegen and every generated output, with negotiation/compatibility/tier-off/removability. No new dependency, expanded admission, reverse edge or new tier is approved.

<a id="ac-9-1"></a>
**AC-9-1 (D):** Each changed contract has exact before/after scope, source section/ADR relation, owner, affected IDs, compatibility/refusal, rejected alternative, approver and acceptance evidence. Frozen source/history remains unchanged until formal admission. Missing method/pin/quote/rights/backend inputs stop only their dependent action while retaining unmet obligations, owner and trigger.

### 9.4 Compatible decomposition and retained executable guard

**REQ-H-001–008/011** preserve `canonicalSpec=spec/CANONICAL_SPEC.md`, the legacy `R-2\.[0-9]+\.[0-9]+[a-z]?` matcher, stock external validator and completed build/history. Do not invent a machine-ledger key, assume links expand canonical coverage, or use `req_index_literal_from` on mixed namespaces. Supplemental canonical status after Q-7 means ratified within this scope only.

Later separately authorized `decompose-spec` extension consumes the ratified supplement and hashes, accepted notes/trace/Q/O2 dispositions, amendment register and actual base/dispatch settings. Re-read default-branch SHA/ancestry, stack CI, backlog/digest/decisions. The observed boundary was DONE Round 4, last R4.18, chainTip main, ending sequence 167b; **only if unchanged** may later work extend Round 5/sequence 168 onward. No number allocated now. Preserve old rows/files/signatures; archive replaced non-state head text byte-for-byte, append transition, retain machine-state keyset and parser-valid `blockedOn: (none)` when clear. New branches default codex/; stacked tickets fork from/target preceding ticket, operator alone merges. No old budgets/provider/harness/model/autonomy permissions are inherited.

Trace OL→note/finding/candidate→supplement section/ID→legacy relation→one primary ticket or explicit owned deferral→numbered AC/layer→run/PR/native evidence. Append supplemental source/pattern/literal owner tables to new-round planning/manifest records. Preserve imported invariants as references. Contracts carry current anchors, ownership/invariants, scope/exclusions, symbols/wire compatibility, AC/negative checks, caps/stops, size/dispatch budget, Live stage, Production mutations, operating clauses and Run line; skeletons have none and cannot dispatch. HUMAN rows name owner/trigger/verification/withheld claims. Use fresh partition review; size by coupling/context, not package count.

A **separately authorized tooling prerequisite** must implement and retain a versioned supplemental coverage/routing guard plus rejection fixtures under build tools. It reads existing records, never owns/writes state. Check candidate/alias closure, literal in-scope coverage, exactly one owner per adopted target, resolvable AC/layer/evidence hooks, unique grammar-valid chain IDs/files, backward dependencies, skeleton refusal, full-tail roles and fresh acceptance. Check `nextTicket` equals the active round's first unlanded row and forks-from/base agrees with verified chain tip and evidenced legacy completion. Record tool version/input digests/counts/diagnostics.

Reject missing/duplicate IDs, wrong owner, old-gate routing, unexplained old residual, pending/failed gate, skeleton and old signature falsely satisfying new acceptance. Native skill coordination must invoke the guard before/after relevant dispatch/boundaries. If headless dispatch is chosen, a tested bounded wrapper may call external driver `--max-iters 1`, preserve CI gates and distinguish iteration-cap/no-progress from success. The wrapper is conditional, not a replacement orchestrator. Stub-worker tests prove old GATE-G1 pointer dispatches zero, correct first-new row dispatches one and pending/failed state stops; test CI failure/cap/resume. Documentation clauses or stock validation alone are insufficient; no unguarded accumulating loop is ready.

Full tail retains independent gap hunt (CAP.1), composed verification (CAP.2), closure (CAP.3), operator acceptance, backlog/readiness (REC.1), spec reconciliation (REC.2), integration plan (REC.3), reader docs (DOC.1), agent docs (DOC.2). Later assign unused grammar-valid IDs and fresh gate with role-equivalence mapping; never rebind old files. New readout/signature with exact operator UTC/channel/words/items/expiry/void conditions and agent-draft hash confirmation is required. Old GATE-ACCEPT signatures cannot satisfy new scope. Pending decision means RETURN PASS; failed technical prerequisite is blockedOn. Agents never sign. Full-tail read obligations include actual access/service claims, landed code/ADRs/coverage and stack CI/release conditions, without granting collection/production authority.

### 9.5 Historical warnings and acceptance packet

H1 recorded zero violations/eight warnings, not a general waiver. Rebaseline against the actual extension. Scope each compatibility disposition by check/path/historical rows, never “any eight warnings”:

| Historical diagnostic | New-round obligation |
|---|---|
| 108 unbannered chain rows | Preserve old rows; number new chain banner. |
| Frozen spec ADR appendix/file-set mismatch | Preserve appendix; new ADR/index/trigger consistency; correction only by admission. |
| DONE versus lowest unlanded GATE-G1 | Verify second 2026-09-15 PASSED—accept ADR-0226 disposition and completed-chain evidence; never let old residual select work or invent closure. |
| 142 oversized PHASE LOG entries | Preserve old region; new region ≤2 KiB fixed-shape entries linking evidence. |
| Two BUILD_INDEX column mismatches | New headers/rows correctly escaped. |
| Two old verification vocabularies | New index template enum plus truthful native class. |
| Four old readouts without guard | Every new readout exact guard/operator provenance. |
| 82 run ledgers without Harness | New Harness/model/tier, Skills, Started/Closed/CI headers. |

Unknown/new violations stay fatal; no rewritten history or relaxed/global guard solely for a strict label. Actual checker/routing incompatibility needs narrow reviewed remedy. Build readiness packet includes ratified digest/frozen source index/trace/Q, O2 and partition dispositions, ADR owners, retained guard/fixtures/actual dispatch checks, manifest/contracts/tail/new acceptance map, warning baseline, resource/access decisions and actual skill/toolchain/harness/base/dispatch settings. One state writer, fresh context per ticket, no overlapping central mutations.

Product acceptance uses `cargo fmt --all --check`, scoped tests and failing-without-change regressions, plus relevant schema drift/removability/CI gating; restore default helper after tier-off and report actual failure/rerun. Examples require semantic/negative/recipient tests and committed-HEAD caveat. Research requires admitted frozen execution and all adverse/unexecuted slots, not a favorable result. Ticket close reviews working/staged/range history, guard, current-head/ancestor CI and branch state, run/PR/index/PHASE LOG agreement. External build-memory/backlog checks and repo row-tool checks are separate; full CI remains maintainer/gate action. Round closure includes independent/composed verdicts, unique owed-work homes, fresh operator acceptance and reconciliation/docs. Append historical corrections rather than overwrite.

<a id="ac-9-2"></a>
**AC-9-2 (D/E):** Before product dispatch, the retained guard and actual chosen launch path pass positive/rejection fixtures, including zero work for old GATE-G1 and old-signature refusal. Reconcile all 180 candidate dispositions/24 OL mappings and one primary owner per adopted target, unique chain/full-tail/backward dependencies, fresh signature and scoped warning baseline. Missing tooling or independent partition/O2/operator review blocks readiness; passing stock validation cannot substitute.

<a id="ac-9-3"></a>
**AC-9-3 (D/R/P):** Closure reports engineering, research, portability, public availability and adoption separately with executed commands/counts/evidence and true skipped/pending limits. Null/negative/inconclusive admitted study can complete R; unmet/unadmitted study needs explicit owned scope disposition. No favorable effect, absent outside participation, old approval or documentation validator may satisfy another layer. Q-7 remains open until human ratification after O2 disposition.


<a id="appendix-a"></a>
## Appendix A. Literal source requirements and disposition index

All 180 original requirement rows below are copied verbatim from their accepted notes, including the original finding/OL and acceptance text. They are provenance, interpreted by §0 rulings and body refinements, not a second conflicting contract. The immediately following per-ID registry supplies the governing destination/acceptance layer and legacy relation. Adopted rows are active within their conditional scope. Merged targets union every source predicate; partial overlaps remain separately adopted. No later study is funded merely by its adopted planning requirement. Every adopted target gets one primary ticket later; exact tickets remain unallocated.

Counts: **180 source IDs = 167 adopted + 12 merged-with + 1 superseded-by + 0 deferred**. REQ-D-108 has only REQ-D-304 as canonical target; REQ-D-309/310 are feasibility/routing dependencies.

### A.A1 — [A1 source](research/01_capability-inventory.md)

| ID | Candidate requirement and rationale | Source findings / outline | Proposed acceptance |
|---|---|---|---|
| REQ-A-001 | Maintain a capability coverage table recording the concrete entry point, contract, build/negotiation gates, evidence class and first-wave/later/excluded disposition for every matrix family. | F1–F12; OL-02, OL-14 | Reviewer maps every family to a reasoned portfolio disposition and a source-backed supported path or explicit absence. |
| REQ-A-002 | A published runnable path must carry its exact input schema and work against the actual boundary; selected CLI wrappers must supply all required fields or be replaced by a documented supported API path. | F1, F7; OL-08, OL-17, OL-20 | Execute tutorial from a clean scratch store; regression tests for claim/launch/settle and comparison must fail against the baseline mismatch. |
| REQ-A-003 | Distinguish task execution, submission capture, independent verifier execution, result production and analysis, with an identified producer and evidence handoff for each. | F6; OL-05, OL-09, OL-20 | One real pilot task traverses the chosen bridge; inspect terminal/grade/accounting artifacts and verify failure paths without authored success rows substituting for execution. |
| REQ-A-004 | Every displayed outcome must distinguish task success, infrastructure success, oracle failure, veto, unsupported and skipped states; a test pass must not conceal an early-return skipped subleg. | F5, F9, F12; OL-12, OL-15 | Fixtures include format_failure under successful transport and missing-helper skips; rendered/exported receipts preserve both statuses and reasons. |
| REQ-A-005 | Verification, reconstruction, deterministic replay, analysis recomputation and fresh execution must have separate claim/command/prerequisite/comparison definitions. | F8; OL-07, OL-16 | Run each promised procedure independently from the distributed artifact; verify its stated scope, provider-call behavior and comparison criteria. |
| REQ-A-006 | An extension walkthrough must name build/package/seal/load/invoke steps, pinned ABI/contracts, grants and isolation prerequisites; foreign conformance must state exact corpus scope. | F9, F11; OL-13, OL-18 | Clean-environment package invocation and tamper rejection; corpus comparator with declared missing/unsupported cases; no inferred general runtime compatibility. |
| REQ-A-007 | Selected context, control, compute and delegation treatments must pin actual variants, ports and provider capability gates; topology and model restrictions must be explicit. | F2–F4; OL-09, OL-14 | Treatment manifests resolve to inspected executable bindings; missing port, disallowed topology and unsupported provider cases fail typed before scientific collection. |
| REQ-A-008 | Offline exemplars must be labeled as teaching/plumbing evidence; their packaged defaults must not silently become confirmatory sample-size, effect or budget decisions. | F6; OL-08–OL-11 | Tutorial labels fixture scope; live protocol separately justifies budgets, repetitions and practical effect criteria before confirmation. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-a-001"></a>REQ-A-001 | merged-with(REQ-C-102) | [§1](#s1); [AC-1-1](#ac-1-1) | extends capability catalogue; preserves A1 per-family R-2.* contracts | D |
| <a id="req-a-002"></a>REQ-A-002 | adopted | [§7](#s7); [AC-7-1](#ac-7-1) | fix K-2 / R-2.10.3–5 forwarding, no CLI semantics | E |
| <a id="req-a-003"></a>REQ-A-003 | adopted | [§2](#s2); [AC-2-1](#ac-2-1), [AC-5-1](#ac-5-1) | extends R-2.9.4 / R-2.10.3 task-to-grade chain | E/R |
| <a id="req-a-004"></a>REQ-A-004 | adopted | [§3](#s3); [AC-3-1](#ac-3-1), [AC-9-3](#ac-9-3) | preserves §10.1 classes / ADR-0363 evidence distinctions | E/D |
| <a id="req-a-005"></a>REQ-A-005 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves R-2.9.3 / §10.4 procedure distinctions; R0 fix | E |
| <a id="req-a-006"></a>REQ-A-006 | adopted | [§8](#s8); [AC-8-1](#ac-8-1) | preserves R-2.12.2 / R-2.11.* ABI/client/corpus limits | E/D |
| <a id="req-a-007"></a>REQ-A-007 | adopted | [§1](#s1); [AC-1-1](#ac-1-1), [AC-2-2](#ac-2-2) | preserves R-2.3–4 / R-2.6 ports/topology gates | D/E |
| <a id="req-a-008"></a>REQ-A-008 | adopted | [§7](#s7); [AC-7-1](#ac-7-1), [AC-4-4](#ac-4-4) | preserves §10.1–.3; fixture versus study admission | D/E |

### A.A2 — [A2 source](research/02_evidence-audit.md)

| ID | Candidate requirement / rationale | Findings / outline | Proposed acceptance |
|---|---|---|---|
| REQ-A-101 | Each collection attempt has an immutable ID, timestamp, code/config revision, outcome, exclusion and supersession; retries never replace earlier observations. Preserve existing evidence and use the ADR/schema workflow for any receipt-contract change. | F2, F6; OL-10, OL-15, OL-16 | Two same-day retries produce distinct retained receipts; totals reconcile; all failures/skips remain queryable. |
| REQ-A-102 | A claim index separates infrastructure assertion, task terminal, independent grade, analysis conclusion, evidence class and checked denominator. | F1, F4–F6, F9; OL-05, OL-12, OL-15 | Format-failure/pass and early-return-skip fixtures render honestly; published claims resolve to exact observations. |
| REQ-A-103 | Distributed receipts enumerate artifact roles and hashes, redaction/access treatment, native event/blob closure and retrievable evidence snapshot independently of code HEAD. | F1, F3, F7; OL-07, OL-16, OL-18 | A clean recipient verifies selected artifacts and completes every advertised procedure; missing/private artifacts produce explicit limits. |
| REQ-A-104 | Useful-task and study evidence preserves actual submission, independently executed verifier result and task-level accounting; judge calibration discloses cases and limitations. | F4, F5, F9; OL-05, OL-09, OL-10, OL-20 | One pilot traverses executor→submission→grader→result→analysis with inspected artifacts and deliberate incorrect-output cases. |
| REQ-A-105 | Each replay/recompute/rerun procedure declares its level, inputs, comparisons, provider-call behavior and resource assumptions; repaired scope is linked without relabelling old failures. | F6, F7; OL-07, OL-15, OL-16 | Independently exercise each promised procedure, including effect dispatch and negative/tampered cases. |
| REQ-A-106 | Client and extension receipts record actual versions, negotiation, invocation/subleg outcomes and callback/notification failures; foreign claims declare corpus and missing coverage. | F8, F9; OL-13, OL-15, OL-17 | Chosen client completes required requests and updates; plugin markers prove invocation; comparator reports omitted cases explicitly. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-a-101"></a>REQ-A-101 | adopted | [§6](#s6); [AC-6-1](#ac-6-1), [AC-6-3](#ac-6-3) | extends R-2.9.3 / §10.4; preserves ADR-0363 legacy | E |
| <a id="req-a-102"></a>REQ-A-102 | adopted | [§3](#s3); [AC-3-1](#ac-3-1), [AC-9-3](#ac-9-3) | preserves §10.1 classes / ADR-0363 evidence distinctions | E/D |
| <a id="req-a-103"></a>REQ-A-103 | adopted | [§6](#s6); [AC-6-1](#ac-6-1), [AC-6-3](#ac-6-3) | extends R-2.9.3 / §10.4; preserves ADR-0363 legacy | E |
| <a id="req-a-104"></a>REQ-A-104 | adopted | [§2](#s2); [AC-2-1](#ac-2-1), [AC-5-1](#ac-5-1) | extends R-2.9.4 / R-2.10.3 task-to-grade chain | E/R |
| <a id="req-a-105"></a>REQ-A-105 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves R-2.9.3 / §10.4 procedure distinctions; R0 fix | E |
| <a id="req-a-106"></a>REQ-A-106 | adopted | [§8](#s8); [AC-8-1](#ac-8-1) | preserves R-2.12.2 / R-2.11.* ABI/client/corpus limits | E/D |

### A.A3 — [A3 source](research/03_newcomer-journey.md)

| ID | Candidate requirement / rationale | Findings; outline | Proposed acceptance |
|---|---|---|---|
| REQ-A-201 | Selected onboarding path distinguishes setup from useful task success and names its input schema, IDs, store and independent artifact check. | F1–F2; OL-01, OL-05, OL-17 | Execute chosen task from scratch; inspect artifact/check and one failed outcome without source consultation. |
| REQ-A-202 | Selected commands are discoverable and forward all required boundary fields; tutorial steps never rely on unsupported wrapper shapes. | F2–F3; OL-08, OL-17, OL-20 | Help/recipe review plus real CLI-to-kernel regression failing on baseline comparison/selected experiment mismatch. |
| REQ-A-203 | Annotated trace uses actual classes/order and causal coordinates, with full receipt access and explicit fixture/internal-effect scope. | F4; OL-06, OL-15 | Every annotation resolves to retained events; no invented phase or inferred real-tool/security claim. |
| REQ-A-204 | Replay and sharing instructions assert semantic report verdicts, limits and permitted comparisons; R0 export/reproduce reconciles lifecycle state with terminal outcome. | F5–F6; OL-07, OL-15–OL-17 | Fresh fixture export→clean-store R0 report succeeds after repair; altered status fails; replay markers do not invalidate the intended comparison. |
| REQ-A-205 | Contribution walkthrough separates bug/docs participation, component packaging and scientific replication, using actual prerequisites and privacy handling. | F7; OL-13, OL-18 | Reproduce one bug report and selected package/replication exercise under their distinct acceptance rules. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-a-201"></a>REQ-A-201 | adopted | [§7](#s7); [AC-7-1](#ac-7-1), [AC-5-1](#ac-5-1); [§8 participant evidence](#s8), [AC-8-2](#ac-8-2) | extends reader job; preserves K-2/task-grade distinction | E/P |
| <a id="req-a-202"></a>REQ-A-202 | adopted | [§7](#s7); [AC-7-1](#ac-7-1) | fix K-2 / R-2.10.3–5 forwarding, no CLI semantics | E |
| <a id="req-a-203"></a>REQ-A-203 | adopted | [§7](#s7); [AC-7-1](#ac-7-1) | preserves native event classes / R-2.2.1 / R-2.9.1 | E/D |
| <a id="req-a-204"></a>REQ-A-204 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves R-2.9.3 / §10.4 procedure distinctions; R0 fix | E |
| <a id="req-a-205"></a>REQ-A-205 | adopted | [§8](#s8); [AC-8-1](#ac-8-1) | preserves R-2.12.2 / R-2.11.* ABI/client/corpus limits | E/D |

### A.B1 — [B1 source](research/04_audiences-and-jobs.md)

| ID | Candidate requirement and rationale | Findings / outline | Proposed acceptance method |
|---|---|---|---|
| REQ-B-001 | State one selected primary audience, concrete decision/job, prerequisites, secondary roles and deliberate first-wave exclusions; label inferred demand and the evidence used to choose it. | F1–F3, F6; OL-01, OL-19 | Portfolio review records Q-2 choice and maps each selected demonstration to a job; no external-user claim without a source |
| REQ-B-002 | Define first value using an independently checked useful task output plus correct interpretation of its outcome; separate setup/lifecycle success and fixture teaching from that milestone. | F2–F4, F6; OL-05, OL-08, OL-15 | G1/F2 task rubric includes expected output/check, run outcome, evidence interpretation and explicit fixture/live labels; later executable acceptance belongs to the selected task |
| REQ-B-003 | Give the chosen first-use route exact prerequisites and recovery guidance for its relevant command, definition, identifier, store and provider failure cases. | F2, F4; OL-01, OL-06, OL-17 | A3/G1 trace each selected-route step and failure to the explanation or a scoped missing-behavior obligation; no requirement to repair every interface |
| REQ-B-004 | Provide a small contribution lane with reproducible evidence, applicable checks, privacy guidance and review expectations, separate from assigned manifest-ticket responsibilities. | F5–F6; OL-18, OL-21 | G2 walks one bug/documentation submission and one selected extension through the actual policy and packaging steps; maintainer authority and ADR rules remain explicit |
| REQ-B-005 | Define adoption stages with task-based success events, denominators, observation windows, retry/assistance handling, missing observations and branching participation paths. | F7; OL-15, OL-19 | G3 measurement dictionary can classify example success, failure, skipped prerequisite, duplicate/retry and missing-follow-up records without silently dropping them |
| REQ-B-006 | Keep adoption, understanding, scientific efficacy and operational evidence claims separate; publish the scope and uncertainty of each measured result. | F1, F3, F7; OL-10, OL-11, OL-15, OL-19 | Review rejects conversion/utility claims inferred solely from traffic, stars, provider completion, fixture passes or release availability; any causal adoption claim has a stated comparison design |
| REQ-B-007 | Propose outside validation around the chosen job and disconfirming outcomes; require an explicit scope/collection decision before recruitment or telemetry and retain unmet-user-need findings. | F6–F8; OL-19, OL-20, OL-24 | Q-6/G3 package names cohort, tasks, observations, costs, consent/privacy and stopping rule; records no outreach or collection as completed without evidence |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-b-001"></a>REQ-B-001 | adopted | [§1](#s1); [AC-1-1](#ac-1-1) | supplements reader/positioning scope; no legacy efficacy upgrade | D |
| <a id="req-b-002"></a>REQ-B-002 | adopted | [§5](#s5); [AC-5-1](#ac-5-1), [AC-8-2](#ac-8-2) | supplements first-value acceptance; R-2.9.4 independent oracle | E/P |
| <a id="req-b-003"></a>REQ-B-003 | adopted | [§7](#s7); [AC-7-1](#ac-7-1) | supplements guides; K-2/identity/refusal invariants | D/E |
| <a id="req-b-004"></a>REQ-B-004 | adopted | [§8](#s8); [AC-8-1](#ac-8-1) | preserves CONTRIBUTING/GOVERNANCE ordinary PR and operator merge | D/E |
| <a id="req-b-005"></a>REQ-B-005 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements adoption protocol; preserves scoped evidence classes | P/D |
| <a id="req-b-006"></a>REQ-B-006 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements adoption protocol; preserves scoped evidence classes | P/D |
| <a id="req-b-007"></a>REQ-B-007 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements adoption protocol; preserves scoped evidence classes | P/D |

### A.B2 — [B2 source](research/05_ecosystem-and-benchmarks.md)

| ID | Candidate requirement and rationale | Findings / obligations | Proposed acceptance method |
|---|---|---|---|
| REQ-B-101 | Every benchmark-dependent claim names exact suite/task, runner, grader and environment revisions; contract-version changes require an explicit amendment disposition. Prevent silent substitution. | F1–F2; OL-04, OL-21 | Check a proposed study manifest against canonical §10.3 and reject missing pins or undocumented version changes |
| REQ-B-102 | Interchange claims name format/version, direction, external consumer, preserved fields and losses; foreign imported claims cannot acquire native authority from conversion. | F2–F3; OL-04, OL-16 | Round-trip one real artifact through a pinned external reader, inspect losses and exercise unsupported direction/version cases |
| REQ-B-103 | Public comparisons distinguish native component interventions from whole-system comparisons and document semantic/budget mismatches. | F4, F6–F8; OL-09, OL-10, OL-15 | Protocol review enumerates treatment differences, task/grader parity, model identities, repeats, failures and accounting before execution |
| REQ-B-104 | Every replay/reproducibility example names its procedure, external-call behavior, evidence level and comparison criterion. | F5; OL-07, OL-16 | Follow the example from a clean store and verify that its observed procedure matches its advertised guarantee |
| REQ-B-105 | Reliability reports distinguish task outcome, mechanism activation, evidence sufficiency, invariant violation and causal hypothesis; replacement attempts remain visible. | F8; OL-12, OL-15 | Review a success-with-violation, missing-evidence case and failed/retried run without coercing them into one success flag |
| REQ-B-106 | Adaptive optimization is a separate, budgeted design with pre-search splits and selection accounting; it cannot silently enter the zero-search compaction contrast. | F6, F9; OL-10, OL-14 | Validate design fields and accounting; reject unregistered candidate search or held-out feedback into optimization |
| REQ-B-107 | Ecosystem positioning links each claimed benefit to demonstrated behavior and includes overlap, current limitations and a scoped participation path. | F2–F10; OL-04, OL-19 | Editorial/source audit plus one newcomer or researcher walkthrough; do not mark usefulness verified solely from documentation |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-b-101"></a>REQ-B-101 | adopted | [§4](#s4); [AC-4-1](#ac-4-1), [AC-4-4](#ac-4-4) | preserves §10.3 TB2/adapter/version admission | D/R |
| <a id="req-b-102"></a>REQ-B-102 | adopted | [§1](#s1); [AC-1-1](#ac-1-1), [AC-6-2](#ac-6-2), [AC-1-2](#ac-1-2) | preserves conversion losses/native authority; S12 explicitly adapts round-trip predicate to one direction | E/D |
| <a id="req-b-103"></a>REQ-B-103 | adopted | [§4](#s4); [AC-4-1](#ac-4-1), [AC-4-4](#ac-4-4) | preserves §10.2–.5 native component and zero-search scope | D/R |
| <a id="req-b-104"></a>REQ-B-104 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves §10.4 levels/noncumulative procedure basis | E |
| <a id="req-b-105"></a>REQ-B-105 | adopted | [§5](#s5); [AC-5-2](#ac-5-2), [AC-3-1](#ac-3-1) | preserves R-2.2 recovery / §10.1 outcome classes | E/R |
| <a id="req-b-106"></a>REQ-B-106 | adopted | [§4](#s4); [AC-4-1](#ac-4-1), [AC-4-4](#ac-4-4) | preserves §10.2–.5 native component and zero-search scope | D/R |
| <a id="req-b-107"></a>REQ-B-107 | adopted | [§1](#s1); [AC-1-1](#ac-1-1); [§8 participant evidence](#s8), [AC-8-2](#ac-8-2) | supplements reader/positioning scope; no legacy efficacy upgrade | D/P |

### A.C1 — [C1 source](design/06_demo-and-study-portfolio.md)

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

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-c-001"></a>REQ-C-001 | merged-with(REQ-C-102) | [§1](#s1); [AC-1-1](#ac-1-1) | supplements full catalogue; preserves all A1 legacy family contracts | D |
| <a id="req-c-002"></a>REQ-C-002 | adopted | [§2](#s2); [AC-2-1](#ac-2-1), [AC-5-1](#ac-5-1), [AC-6-1](#ac-6-1) | extends R-2.9.4/R-2.10.3; CC1/K-2 ownership | E/D |
| <a id="req-c-003"></a>REQ-C-003 | adopted | [§6](#s6); [AC-6-1](#ac-6-1), [AC-6-2](#ac-6-2), [AC-7-1](#ac-7-1) | fix K-2/R0; extends §10.4 immutable closure | E |
| <a id="req-c-004"></a>REQ-C-004 | adopted | [§4](#s4); [AC-4-1](#ac-4-1), [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | preserves §10.2–.5; conditional methods in §9.3 | D/E/R |
| <a id="req-c-005"></a>REQ-C-005 | adopted | [§9](#s9); [AC-9-3](#ac-9-3) | preserves scientific versus default/C4 acceptance | D/R |
| <a id="req-c-006"></a>REQ-C-006 | adopted | [§5](#s5); [AC-5-2](#ac-5-2) | preserves R-2.2/R-2.8 bounded operational claims | E |
| <a id="req-c-007"></a>REQ-C-007 | adopted | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2); [§8 participant evidence](#s8), [AC-8-2](#ac-8-2), [AC-1-2](#ac-1-2) | preserves §10.4/R-2.12.2 participation and ABI scope | D/E/P |
| <a id="req-c-008"></a>REQ-C-008 | adopted | [§1](#s1); [AC-1-1](#ac-1-1), [AC-9-3](#ac-9-3) | supplements explicit rescope; preserves central §10.5.1 objective | D |

### A.C2 — [C2 source](design/08_feasibility-and-scope.md)

| ID | Candidate / rationale | Findings / outline | Proposed acceptance |
|---|---|---|---|
| REQ-C-101 | Separate task, teaching and study gates; retain actual checked-task minimum in a reduced release. | F1/F2; OL-05, OL-08–OL-11, OL-15 | Readiness labels fixture-only fallback as unmet task/study objectives requiring rescope. |
| REQ-C-102 | Preserve 22 families/nine later groups, optional ceilings and explicit disposition changes. | F2; OL-03, OL-14, OL-24 | H2 reconciles coverage with recorded decisions; no silent deletion. |
| REQ-C-103 | Select smallest supported native composition, preserving owners and upstream task semantics. | F1/F3; OL-04–OL-06, OL-20–OL-21 | E3 compares routes and identifies writer/effect/oracle/budget/results authority; no forged rows. |
| REQ-C-104 | Retain package assumptions/exclusions/material scope triggers; re-estimate after E2. | F3; OL-20, OL-24 | Decomposition links scope to acceptance; material scope expansion returns for decision, not routine slicing. |
| REQ-C-105 | Budget confirmation, parity, pilots, retries, grading, replication and retention separately under frozen rules. | F4; OL-09–OL-11, OL-15–OL-16, OL-20–OL-21 | Q-3/Q-4 worksheet uses selected T/R/rates/enforced caps; no inherited spend or silent reduction. |
| REQ-C-106 | Gate compaction collection on serialized-input treatment evidence, four enforced dimensions and detector/rate coverage. | F1/F4; OL-09–OL-10, OL-15 | Acceptance rejects labels, fixed counters and count/rate substitution. |
| REQ-C-107 | Bound every spike's writes/prerequisites/resources/operations/stop/result evidence; allow source-backed design results. | F5; OL-20, OL-22, OL-24 | E2 reconciles actual use to authorization; no hidden repairs or provider/task execution. |
| REQ-C-108 | Keep portfolio, resources/artifacts, execution, ratification and build readiness separate. | F2/F4/F5; OL-11, OL-21, OL-23–OL-24 | Exact decisions precede dependent actions; scientific success/default adoption stay distinct. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-c-101"></a>REQ-C-101 | adopted | [§1](#s1); [AC-1-1](#ac-1-1), [AC-9-3](#ac-9-3) | supplements explicit rescope; preserves central §10.5.1 objective | D |
| <a id="req-c-102"></a>REQ-C-102 | adopted | [§1](#s1); [AC-1-1](#ac-1-1) | supplements full catalogue; preserves all A1 legacy family contracts | D |
| <a id="req-c-103"></a>REQ-C-103 | adopted | [§2](#s2); [AC-2-1](#ac-2-1), [AC-5-1](#ac-5-1), [AC-6-1](#ac-6-1) | extends R-2.9.4/R-2.10.3; CC1/K-2 ownership | E/D |
| <a id="req-c-104"></a>REQ-C-104 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves amendment/manifest/operator workflow | D |
| <a id="req-c-105"></a>REQ-C-105 | adopted | [§9](#s9); [AC-4-4](#ac-4-4), [AC-9-1](#ac-9-1) | preserves §10.1–.4 budget/phase separation | D/R |
| <a id="req-c-106"></a>REQ-C-106 | adopted | [§4](#s4); [AC-4-1](#ac-4-1), [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | preserves §10.2–.5; conditional methods in §9.3 | D/E/R |
| <a id="req-c-107"></a>REQ-C-107 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves amendment/manifest/operator workflow | D |
| <a id="req-c-108"></a>REQ-C-108 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves amendment/manifest/operator workflow | D |

### A.D1 — [D1 source](design/09_measurement-standard.md)

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

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-d-001"></a>REQ-D-001 | adopted | [§3](#s3); [AC-3-1](#ac-3-1) | extends §10.1 / ADR-0158 populations/bounds | E/R |
| <a id="req-d-002"></a>REQ-D-002 | adopted | [§3](#s3); [AC-3-1](#ac-3-1) | extends §10.1 / ADR-0158 populations/bounds | E/R |
| <a id="req-d-003"></a>REQ-D-003 | adopted | [§3](#s3); [AC-3-1](#ac-3-1) | extends §10.1 / ADR-0158 populations/bounds | E/R |
| <a id="req-d-004"></a>REQ-D-004 | adopted | [§4](#s4); [AC-5-1](#ac-5-1), [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | preserves §10.3 executable oracle/adapter admission | E/R |
| <a id="req-d-005"></a>REQ-D-005 | adopted | [§4](#s4); [AC-2-2](#ac-2-2), [AC-4-1](#ac-4-1) | extends ADR-0077 D4 / §10.5.1 exact detector joins | E |
| <a id="req-d-006"></a>REQ-D-006 | adopted | [§2](#s2); [AC-2-3](#ac-2-3) | preserves §8.2 / §10.2 Account/derived accounting | E |
| <a id="req-d-007"></a>REQ-D-007 | adopted | [§3](#s3); [AC-3-2](#ac-3-2), [AC-4-4](#ac-4-4), [AC-9-3](#ac-9-3) | preserves §10.1–.3 freezes/decisions; narrow parity grouping only | D/R |
| <a id="req-d-008"></a>REQ-D-008 | adopted | [§3](#s3); [AC-3-1](#ac-3-1), [AC-4-2](#ac-4-2), [AC-4-3](#ac-4-3) | amends §10.1 / ADR-0158 methods within existing owners | E |
| <a id="req-d-009"></a>REQ-D-009 | adopted | [§3](#s3); [AC-3-2](#ac-3-2), [AC-4-4](#ac-4-4), [AC-9-3](#ac-9-3) | preserves §10.1–.3 freezes/decisions; narrow parity grouping only | D/R |
| <a id="req-d-010"></a>REQ-D-010 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves §10.4 procedure-specific claims | E |

### A.D2 — [D2 source](design/10_compaction-protocol.md)

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

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-d-101"></a>REQ-D-101 | adopted | [§4](#s4); [AC-4-1](#ac-4-1), [AC-3-1](#ac-3-1), [AC-4-4](#ac-4-4) | preserves §10.5.1 protocol; §9.3 narrow lifecycle/method amendments | D/E/R |
| <a id="req-d-102"></a>REQ-D-102 | adopted | [§4](#s4); [AC-4-1](#ac-4-1), [AC-3-1](#ac-3-1), [AC-4-4](#ac-4-4) | preserves §10.5.1 protocol; §9.3 narrow lifecycle/method amendments | D/E/R |
| <a id="req-d-103"></a>REQ-D-103 | adopted | [§4](#s4); [AC-4-1](#ac-4-1), [AC-3-1](#ac-3-1), [AC-4-4](#ac-4-4) | preserves §10.5.1 protocol; §9.3 narrow lifecycle/method amendments | D/E/R |
| <a id="req-d-104"></a>REQ-D-104 | adopted | [§4](#s4); [AC-4-1](#ac-4-1), [AC-3-1](#ac-3-1), [AC-4-4](#ac-4-4) | preserves §10.5.1 protocol; §9.3 narrow lifecycle/method amendments | D/E/R |
| <a id="req-d-105"></a>REQ-D-105 | adopted | [§4](#s4); [AC-4-1](#ac-4-1), [AC-3-1](#ac-3-1), [AC-4-4](#ac-4-4) | preserves §10.5.1 protocol; §9.3 narrow lifecycle/method amendments | D/E/R |
| <a id="req-d-106"></a>REQ-D-106 | adopted | [§4](#s4); [AC-4-1](#ac-4-1) | amends ADR-0077 D4 / §10.5.1 detector metadata | E/R |
| <a id="req-d-107"></a>REQ-D-107 | adopted | [§4](#s4); [AC-4-2](#ac-4-2), [AC-3-1](#ac-3-1) | conditional §10.1/§05h.2/ADR-0158 finite-task efficacy admission | E/R |
| <a id="req-d-108"></a>REQ-D-108 | superseded-by(REQ-D-304); dependencies REQ-D-309/310 | [§4](#s4); [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | supersedes D2 CP choice; §10.2 admission retained via D-304/309/310 | D/E |
| <a id="req-d-109"></a>REQ-D-109 | adopted | [§4](#s4); [AC-4-2](#ac-4-2), [AC-3-1](#ac-3-1) | conditional §10.1/§05h.2/ADR-0158 finite-task efficacy admission | E/R |
| <a id="req-d-110"></a>REQ-D-110 | adopted | [§9](#s9); [AC-4-4](#ac-4-4), [AC-9-1](#ac-9-1) | preserves §10.1–.3 resource/fixed-design admission | D/R |

### A.D3 — [D3 source](design/11_operational-proof-protocols.md)

| ID | Requirement | Source; outline | Proposed acceptance |
|---|---|---|---|
| REQ-D-201 | Useful output requires oracle and complete attempt history. | F1/F2; OL-05–OL-06, OL-15 | Two fixed subjects; correct/wrong/broken-oracle controls. |
| REQ-D-202 | Separate chain/replay/R0/R1/recompute/fresh claims. | F2; OL-07, OL-16 | Per-procedure controls/verdicts, no replay dispatch, actual independence. |
| REQ-D-203 | Bind operational claims to confined real sink/observer. | F3; OL-12, OL-15 | Entry/bytes/counts/sentinel and O8 duplicate detection. |
| REQ-D-204 | Preserve Kp4 unknown/probe without optimistic retry. | F1/F3; OL-12, OL-16 | O1/O2 ×3; reached fault, prefix/fence/repeated restore and probe limits. |
| REQ-D-205 | Exercise actual permission sequence and negative transitions. | F3/F4; OL-12, OL-15 | O3–O5 ×3; pending restart, denied/invalid/duplicate responses. |
| REQ-D-206 | Use real Account refusal/exhaustion with committed-effect accounting. | F4; OL-12, OL-20–OL-21 | O6/O7 ×3; elapsed deadline, drain/unknown, no later dispatch, charge reconstruction. |
| REQ-D-207 | Preserve opportunity counts/failures with fixed stops. | F5; OL-10–OL-12, OL-15 | Reconciled stage counts, unknowns, seeded detector denominator, partial cohort. |
| REQ-D-208 | Link mismatch to scoped verifyfixed. | F2/F6; OL-16–OL-18 | Baseline/repair identities and six R0 controls; preserve A3. |
| REQ-D-209 | Gate extension claims on invocation/refusal controls. | F6; OL-13, OL-18 | Four package checks, explicit skips, kernel hash/build-cost disclosure. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-d-201"></a>REQ-D-201 | adopted | [§5](#s5); [AC-5-1](#ac-5-1) | preserves §10.3 upstream oracle and independent task truth | E |
| <a id="req-d-202"></a>REQ-D-202 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves §10.4 R0–R3/replay limits | E |
| <a id="req-d-203"></a>REQ-D-203 | adopted | [§5](#s5); [AC-5-2](#ac-5-2), [AC-2-3](#ac-2-3) | preserves R-2.2/R-2.8/Account native transitions | E |
| <a id="req-d-204"></a>REQ-D-204 | adopted | [§5](#s5); [AC-5-2](#ac-5-2), [AC-2-3](#ac-2-3) | preserves R-2.2/R-2.8/Account native transitions | E |
| <a id="req-d-205"></a>REQ-D-205 | adopted | [§5](#s5); [AC-5-2](#ac-5-2), [AC-2-3](#ac-2-3) | preserves R-2.2/R-2.8/Account native transitions | E |
| <a id="req-d-206"></a>REQ-D-206 | adopted | [§5](#s5); [AC-5-2](#ac-5-2), [AC-2-3](#ac-2-3) | preserves R-2.2/R-2.8/Account native transitions | E |
| <a id="req-d-207"></a>REQ-D-207 | adopted | [§5](#s5); [AC-5-2](#ac-5-2), [AC-2-3](#ac-2-3) | preserves R-2.2/R-2.8/Account native transitions | E |
| <a id="req-d-208"></a>REQ-D-208 | adopted | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2) | preserves R-2.9.3/R-2.12.2; verifyfixed/package scope | E |
| <a id="req-d-209"></a>REQ-D-209 | adopted | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2) | preserves R-2.9.3/R-2.12.2; verifyfixed/package scope | E |

### A.D4 — [D4 source](design/23_parity-method-review.md)

| ID | Candidate | Findings / outline | Acceptance |
|---|---|---|---|
| REQ-D-301 | Authentic original-runner admission. | F1; OL-09/15–16 | Fixture/wrapper alone rejected. |
| REQ-D-302 | Fixed target, equal weights/R, independent groups. | F2/F4; OL-09–10/21 | Manifest/count/dependence checks. |
| REQ-D-303 | Dimension map separate from caps. | F2/F5; OL-10/15/21 | Required unavailable outcome blocks. |
| REQ-D-304 | Declared KL bounds/TOST levels. | F4; OL-10–11/16 | Tail/endpoint/outward-rounding checks. |
| REQ-D-305 | Planned missingness/veto populations. | F5; OL-10–11/15 | Unknown/retry completion envelopes. |
| REQ-D-306 | Correct bounded relative-mean tests. | F5/F8; OL-10/16/20 | Scale/zero/unknown-bound refusals. |
| REQ-D-307 | All-endpoint versus selected-claim multiplicity. | F5; OL-10–11 | Frozen family/unknown-member checks. |
| REQ-D-308 | Scope calibration to actual policies/claim. | F6; OL-09–11/15 | Bias-addition/joint-error counterexamples. |
| REQ-D-309 | Feasible quotes/fixed final sample. | F7; OL-09–11/20 | Discordance scenarios; zero unset funding. |
| REQ-D-310 | Existing-owner admission/routing. | F3/F8; OL-16/20–24 | ADR, numerical and dispatch acceptance. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-d-301"></a>REQ-D-301 | adopted | [§4](#s4); [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | narrow §10.1–.3/§05h.2.1/ADR-0158 amendments; §9.3 | D/E/R |
| <a id="req-d-302"></a>REQ-D-302 | adopted | [§4](#s4); [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | narrow §10.1–.3/§05h.2.1/ADR-0158 amendments; §9.3 | D/E/R |
| <a id="req-d-303"></a>REQ-D-303 | adopted | [§4](#s4); [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | narrow §10.1–.3/§05h.2.1/ADR-0158 amendments; §9.3 | D/E/R |
| <a id="req-d-304"></a>REQ-D-304 | adopted | [§4](#s4); [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | narrow §10.1–.3/§05h.2.1/ADR-0158 amendments; §9.3 | D/E/R |
| <a id="req-d-305"></a>REQ-D-305 | adopted | [§4](#s4); [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | narrow §10.1–.3/§05h.2.1/ADR-0158 amendments; §9.3 | D/E/R |
| <a id="req-d-306"></a>REQ-D-306 | adopted | [§4](#s4); [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | narrow §10.1–.3/§05h.2.1/ADR-0158 amendments; §9.3 | D/E/R |
| <a id="req-d-307"></a>REQ-D-307 | adopted | [§4](#s4); [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | narrow §10.1–.3/§05h.2.1/ADR-0158 amendments; §9.3 | D/E/R |
| <a id="req-d-308"></a>REQ-D-308 | adopted | [§4](#s4); [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | narrow §10.1–.3/§05h.2.1/ADR-0158 amendments; §9.3 | D/E/R |
| <a id="req-d-309"></a>REQ-D-309 | adopted | [§9](#s9); [AC-4-4](#ac-4-4), [AC-9-1](#ac-9-1) | preserves §10.1–.3 resource/fixed-design admission | D/R |
| <a id="req-d-310"></a>REQ-D-310 | adopted | [§4](#s4); [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | narrow §10.1–.3/§05h.2.1/ADR-0158 amendments; §9.3 | D/E/R |

### A.E1 — [E1 source](design/07_execution-seams.md)

| ID | Candidate requirement and rationale | Findings / outline | Proposed acceptance |
|---|---|---|---|
| REQ-E-001 | Bind a launched subject and fenced writer to actual native execution, with explicit adoption/resume ownership. | F1; OL-05, OL-08, OL-20 | One subject identity across launch, execution and settlement; stale/duplicate writer rejected; no phantom second run. |
| REQ-E-002 | Materialize external tasks and mediate real effects through existing environment authority; capture immutable submission. | F2/F10; OL-05–OL-06, OL-20 | Actual workspace effect/observation and submission digest; denied effect leaves observable sink unchanged. |
| REQ-E-003 | Prove configured native compaction strategy invocation and resulting provider-request change. | F3; OL-09–OL-10, OL-20 | Capturing transport verifies each intended retention policy, required pairs and typed unknown/unbound refusal. |
| REQ-E-004 | Declare control/trigger semantics and exact detector/denominator mapping for all canonical metrics. | F4; OL-09–OL-11, OL-15 | Known traces validate counts/rates; no-trigger, no-detector, fallback and ineffective outcomes remain visible. |
| REQ-E-005 | Enforce every matched dimension through existing durable budget authority with real time and actual role charges. | F5; OL-10, OL-15, OL-20 | Reserve/charge/exhaustion checks per dimension; failed calls and committed effects accounted; unenforced/unequal comparisons refused. |
| REQ-E-006 | Fix and record requested/served model identities, role assignments and cache/seed/routing behavior. | F5; OL-09–OL-10, OL-15 | Manifest plus call evidence validates role equality and subject/instrument charge separation. |
| REQ-E-007 | Execute pinned upstream oracles with evidenced declared isolation and preserve their outputs/failures independently of supplied grade values. | F6/F10; OL-05–OL-06, OL-15–OL-16 | Correct/wrong output and broken oracle diverge as intended; held-out access, tamper status and instrument costs checked. |
| REQ-E-008 | Define coherent execution-terminal, grade/overlay, settlement and retry/outcome semantics. | F7; OL-06, OL-08, OL-11, OL-20 | Grader crash cannot become zero or terminal success; supersessions and denominators agree across scheduler/results/report. |
| REQ-E-009 | Orchestrate native projections and persisted analysis through one supported client path; repair selected CLI contracts without CLI semantics. | F8; OL-08, OL-11, OL-17 | Scratch workflow consumes real producer facts; request regressions and missing-input refusals; fixture labels remain explicit. |
| REQ-E-010 | Retain every immutable attempt with native member closure and corrected, separately scoped R0 reproduction. | F9; OL-07, OL-15–OL-18 | Same-day retries coexist; required native artifacts survive cleanup; clean-store R0 passes and tamper/status controls fail. |
| REQ-E-011 | Preserve TB2/version/parity, native arms, match tuple, zero search and replication floor while separately pinning selected tasks and statistical design. | Basis/F10; OL-09–OL-11, OL-21 | Admission rejects silent substitutions, fixture efficacy and no-parity component claims; `2TR` planning distinguishes total resources. |
| REQ-E-012 | Keep operational cases confined to one declared effect boundary and observable controls, with resolved tiers and honest unknown/refusal outcomes. | F11; OL-12, OL-15, OL-20 | Kp4/control, denial, pause and exhaustion expose both ledger and sink outcomes under stated assumptions. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-e-001"></a>REQ-E-001 | merged-with(REQ-E-201) | [§2](#s2); [AC-2-1](#ac-2-1) | extends R-2.10.3 / embed arming; one subject/fenced writer | E |
| <a id="req-e-002"></a>REQ-E-002 | adopted | [§2](#s2); [AC-5-1](#ac-5-1), [AC-5-2](#ac-5-2) | extends R-2.9.4 / environment owner; unchanged upstream oracle | E |
| <a id="req-e-003"></a>REQ-E-003 | merged-with(REQ-E-206) | [§2](#s2); [AC-2-2](#ac-2-2) | extends §05c/§05b treatment binding; CC1 serializer | E |
| <a id="req-e-004"></a>REQ-E-004 | adopted | [§4](#s4); [AC-4-1](#ac-4-1) | extends ADR-0077 D4 / §10.5.1 detector contract | E |
| <a id="req-e-005"></a>REQ-E-005 | merged-with(REQ-E-207) | [§2](#s2); [AC-2-3](#ac-2-3) | extends §8.2 admission; preserves Account/physical-use semantics | E |
| <a id="req-e-006"></a>REQ-E-006 | adopted | [§2](#s2); [AC-2-2](#ac-2-2), [AC-2-3](#ac-2-3), [AC-4-4](#ac-4-4) | preserves R-2.3.* / §10.2 snapshot/cache/role matching | D/E |
| <a id="req-e-007"></a>REQ-E-007 | adopted | [§2](#s2); [AC-5-1](#ac-5-1), [AC-5-2](#ac-5-2) | extends R-2.9.4 / environment owner; unchanged upstream oracle | E |
| <a id="req-e-008"></a>REQ-E-008 | adopted | [§2](#s2); [AC-2-1](#ac-2-1) | amends R-2.10.3–5 required-grade mode; preserves M1/R-2.9.1 | E |
| <a id="req-e-009"></a>REQ-E-009 | adopted | [§7](#s7); [AC-7-1](#ac-7-1) | fix K-2 forwarding; preserves R-2.10.* projections/analysis | E |
| <a id="req-e-010"></a>REQ-E-010 | merged-with(REQ-E-208) | [§6](#s6); [AC-6-1](#ac-6-1), [AC-6-2](#ac-6-2) | extends R-2.9.3 / §10.4 closure; existing R0 lifecycle fix | E |
| <a id="req-e-011"></a>REQ-E-011 | adopted | [§4](#s4); [AC-4-1](#ac-4-1), [AC-4-4](#ac-4-4) | preserves §10.3/.5.1 TB2/native match/floors | D/R |
| <a id="req-e-012"></a>REQ-E-012 | adopted | [§5](#s5); [AC-5-2](#ac-5-2) | extends shared Dispatcher seam; native recovery/permission semantics | E |

### A.E2 — [E2 source](design/12_feasibility-findings.md)

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

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-e-101"></a>REQ-E-101 | merged-with(REQ-E-201) | [§2](#s2); [AC-2-1](#ac-2-1) | extends R-2.10.3 / embed arming; one subject/fenced writer | E |
| <a id="req-e-102"></a>REQ-E-102 | adopted | [§2](#s2); [AC-2-1](#ac-2-1) | amends R-2.10.3–5 required-grade mode; preserves M1/R-2.9.1 | E |
| <a id="req-e-103"></a>REQ-E-103 | adopted | [§2](#s2); [AC-2-1](#ac-2-1) | amends R-2.10.3–5 required-grade mode; preserves M1/R-2.9.1 | E |
| <a id="req-e-104"></a>REQ-E-104 | merged-with(REQ-E-206) | [§2](#s2); [AC-2-2](#ac-2-2) | extends §05c/§05b treatment binding; CC1 serializer | E |
| <a id="req-e-105"></a>REQ-E-105 | merged-with(REQ-E-207) | [§2](#s2); [AC-2-3](#ac-2-3) | extends §8.2 admission; preserves Account/physical-use semantics | E |
| <a id="req-e-106"></a>REQ-E-106 | merged-with(REQ-E-207) | [§2](#s2); [AC-2-3](#ac-2-3) | extends §8.2 admission; preserves Account/physical-use semantics | E |
| <a id="req-e-107"></a>REQ-E-107 | adopted | [§4](#s4); [AC-4-1](#ac-4-1) | extends ADR-0077 D4 / §10.5.1 detector contract | E |
| <a id="req-e-108"></a>REQ-E-108 | merged-with(REQ-E-208) | [§6](#s6); [AC-6-1](#ac-6-1), [AC-6-2](#ac-6-2) | extends R-2.9.3 / §10.4 closure; existing R0 lifecycle fix | E |
| <a id="req-e-109"></a>REQ-E-109 | merged-with(REQ-E-208) | [§6](#s6); [AC-6-1](#ac-6-1), [AC-6-2](#ac-6-2) | extends R-2.9.3 / §10.4 closure; existing R0 lifecycle fix | E |
| <a id="req-e-110"></a>REQ-E-110 | adopted | [§5](#s5); [AC-5-2](#ac-5-2) | extends shared Dispatcher seam; native recovery/permission semantics | E |

### A.E3 — [E3 source](design/14_integration-design.md)

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

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-e-201"></a>REQ-E-201 | adopted | [§2](#s2); [AC-2-1](#ac-2-1) | extends R-2.10.3 / embed arming; one subject/fenced writer | E |
| <a id="req-e-202"></a>REQ-E-202 | adopted | [§2](#s2); [AC-2-1](#ac-2-1) | amends R-2.10.3–5 required-grade mode; preserves M1/R-2.9.1 | E |
| <a id="req-e-203"></a>REQ-E-203 | adopted | [§2](#s2); [AC-2-1](#ac-2-1) | amends R-2.10.3–5 required-grade mode; preserves M1/R-2.9.1 | E |
| <a id="req-e-204"></a>REQ-E-204 | adopted | [§2](#s2); [AC-2-1](#ac-2-1) | amends R-2.10.3–5 required-grade mode; preserves M1/R-2.9.1 | E |
| <a id="req-e-205"></a>REQ-E-205 | adopted | [§5](#s5); [AC-5-2](#ac-5-2) | extends shared Dispatcher seam; native recovery/permission semantics | E |
| <a id="req-e-206"></a>REQ-E-206 | adopted | [§2](#s2); [AC-2-2](#ac-2-2) | extends §05c/§05b treatment binding; CC1 serializer | E |
| <a id="req-e-207"></a>REQ-E-207 | adopted | [§2](#s2); [AC-2-3](#ac-2-3) | extends §8.2 admission; preserves Account/physical-use semantics | E |
| <a id="req-e-208"></a>REQ-E-208 | adopted | [§6](#s6); [AC-6-1](#ac-6-1), [AC-6-2](#ac-6-2) | extends R-2.9.3 / §10.4 closure; existing R0 lifecycle fix | E |
| <a id="req-e-209"></a>REQ-E-209 | adopted | [§4](#s4); [AC-3-1](#ac-3-1), [AC-4-1](#ac-4-1), [AC-4-2](#ac-4-2), [AC-4-3](#ac-4-3) | narrow §10/ADR-0158/0077 admission; existing-owner routing | D/E |
| <a id="req-e-210"></a>REQ-E-210 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves CC1/CC7/CC10/tier/dependency/amendment constraints | D/E |

### A.F1 — [F1 source](design/13_receipt-contract.md)

| ID | Requirement / rationale | Findings / outline | Acceptance |
|---|---|---|---|
| REQ-F-001 | Minimal native profile; derived index, existing authorities. | F1; OL-16, OL-20–OL-21 | Regenerate from sources; unknown profile refusal. |
| REQ-F-002 | Immutable slot/attempt/subject/grade/export identities and native heads. | F2; OL-10, OL-15–OL-16 | Collision/retry/regrade/head-substitution cases. |
| REQ-F-003 | All-planned reconciliation; separate prerequisite/process/task/veto verdicts. | F3; OL-10–OL-11, OL-15 | Partial/orphan/format-failure/skip cases. |
| REQ-F-004 | Pins/raw grades/errors/usage/treatment and versioned analysis. | F4; OL-05, OL-09–OL-10, OL-16 | Recompute weights/selectors/families; reject altered inputs. |
| REQ-F-005 | Native/typed reference traversal and recipient procedure closure. | F5; OL-07, OL-16, OL-18 | Missing blob/payload/fetch/tamper/cleanup cases. |
| REQ-F-006 | Rights/access/retention and per-claim loss; sanitization supersedes. | F6; OL-15–OL-16, OL-20–OL-21 | Public/restricted/GC/redaction losses. |
| REQ-F-007 | Legacy compatibility; levels distinct from actual procedure verdicts. | F7; OL-07, OL-15–OL-17, OL-21 | Legacy/R0/unknown-profile regression cases. |
| REQ-F-008 | Claims bind tested prerequisites/population/evidence/limits. | F3–F7; OL-10–OL-11, OL-15–OL-16 | Unrun/unavailable/negative cannot become verified. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-f-001"></a>REQ-F-001 | adopted | [§6](#s6); [AC-6-1](#ac-6-1) | extends R-2.9.3/§10.4 native profile/closure; existing authorities | E |
| <a id="req-f-002"></a>REQ-F-002 | adopted | [§6](#s6); [AC-6-1](#ac-6-1) | extends R-2.9.3/§10.4 native profile/closure; existing authorities | E |
| <a id="req-f-003"></a>REQ-F-003 | adopted | [§6](#s6); [AC-6-1](#ac-6-1) | extends R-2.9.3/§10.4 native profile/closure; existing authorities | E |
| <a id="req-f-004"></a>REQ-F-004 | adopted | [§6](#s6); [AC-6-1](#ac-6-1) | extends R-2.9.3/§10.4 native profile/closure; existing authorities | E |
| <a id="req-f-005"></a>REQ-F-005 | adopted | [§6](#s6); [AC-6-1](#ac-6-1) | extends R-2.9.3/§10.4 native profile/closure; existing authorities | E |
| <a id="req-f-006"></a>REQ-F-006 | adopted | [§6](#s6); [AC-6-3](#ac-6-3) | preserves §10.4 rights/loss/supersession; Q-5 retention | D/E |
| <a id="req-f-007"></a>REQ-F-007 | adopted | [§6](#s6); [AC-6-1](#ac-6-1), [AC-6-2](#ac-6-2), [AC-9-3](#ac-9-3) | preserves ADR-0363 legacy/§10.4 scopes; no evidence upgrade | D/E |
| <a id="req-f-008"></a>REQ-F-008 | adopted | [§6](#s6); [AC-6-1](#ac-6-1), [AC-6-2](#ac-6-2), [AC-9-3](#ac-9-3) | preserves ADR-0363 legacy/§10.4 scopes; no evidence upgrade | D/E |

### A.F2 — [F2 source](design/15_examples-and-maintenance.md)

| ID | Requirement / rationale | Finding / outline | Acceptance |
|---|---|---|---|
| REQ-F-101 | Four families; offline/live/readiness separation. | F1/F2; OL-05–OL-08, OL-12, OL-17 | Owner/recipe/unavailable mapping; no fixture substitution. |
| REQ-F-102 | Semantic/negative checks; invocation distinct. | F3; OL-07–OL-08, OL-15–OL-17 | CLI/R0/tamper fail; expected refusal keeps refused outcome. |
| REQ-F-103 | Synthetic Lab lifecycle/projection/persisted analysis. | F1–F3; OL-08, OL-17 | Joins, repeat identity, changed-fixture response, unavailable controls. |
| REQ-F-104 | Exact pins and separate costs. | F3; OL-16–OL-17, OL-20 | Unresolved caps block live; unknown stays unknown; offline no provider. |
| REQ-F-105 | F1 native receipts/recipient closure/event links. | F3; OL-06–OL-07, OL-16 | No home fallback; missing/corrupt/restricted inputs tested. |
| REQ-F-106 | Both-mode equivalence/full execution under ADR-0362. | F4; OL-17, OL-20–OL-21 | Lint; docs drift fails fast path; recipe change selects full. |
| REQ-F-107 | Separate working-tree/HEAD/live checks. | F1/F4; OL-15, OL-17, OL-21 | Old HEAD cannot verify edits; skips never become live passes. |
| REQ-F-108 | Code/freshness/study maintenance with supersession. | F4; OL-11, OL-16–OL-17, OL-24 | Lost inputs disable procedure; drift never silently recollects. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-f-101"></a>REQ-F-101 | adopted | [§7](#s7); [AC-7-1](#ac-7-1) | supplements executable docs; K-2/§10.4 identities/fixture limits | D/E |
| <a id="req-f-102"></a>REQ-F-102 | adopted | [§7](#s7); [AC-7-1](#ac-7-1) | supplements executable docs; K-2/§10.4 identities/fixture limits | D/E |
| <a id="req-f-103"></a>REQ-F-103 | adopted | [§7](#s7); [AC-7-1](#ac-7-1) | supplements executable docs; K-2/§10.4 identities/fixture limits | D/E |
| <a id="req-f-104"></a>REQ-F-104 | adopted | [§7](#s7); [AC-7-1](#ac-7-1) | supplements executable docs; K-2/§10.4 identities/fixture limits | D/E |
| <a id="req-f-105"></a>REQ-F-105 | adopted | [§7](#s7); [AC-7-1](#ac-7-1) | supplements executable docs; K-2/§10.4 identities/fixture limits | D/E |
| <a id="req-f-106"></a>REQ-F-106 | adopted | [§7](#s7); [AC-7-2](#ac-7-2) | preserves ADR-0362/0363/docs edit modes; executable maintenance | D/E |
| <a id="req-f-107"></a>REQ-F-107 | adopted | [§7](#s7); [AC-7-2](#ac-7-2) | preserves ADR-0362/0363/docs edit modes; executable maintenance | D/E |
| <a id="req-f-108"></a>REQ-F-108 | adopted | [§7](#s7); [AC-7-2](#ac-7-2) | preserves ADR-0362/0363/docs edit modes; executable maintenance | D/E |

### A.F3 — [F3 source](design/16_replication-and-artifacts.md)

| ID | Candidate | Source / outline | Acceptance |
|---|---|---|---|
| REQ-F-201 | Separate procedure/level/verdict/claim. | F1; OL-07/15–16 | R0 failure/replay/fresh counterexamples. |
| REQ-F-202 | Pinned handoff and closure without producer fallback. | F2; OL-16–18/20 | Clean installation after cleanup. |
| REQ-F-203 | Recompute frozen fields with exact/numeric rules. | F3; OL-10–11/15–16 | Missing selectors/weights/threshold cases. |
| REQ-F-204 | Isolate replay/R1 and detect live attempts. | F4; OL-07/12/16 | Hash/prefix/tamper/dispatch controls. |
| REQ-F-205 | Gate fresh R2/R3/transfer and scientific scope. | F5; OL-09–12/16/21 | Missing pins/parity/methods refuse. |
| REQ-F-206 | Require outside execution/report for participation. | F5; OL-16/18 | Outside review; same-author negative. |
| REQ-F-207 | Bound rights/storage/access/retention and losses. | F6; OL-15–16/20–21 | Expiry/redaction/version-deletion cases. |
| REQ-F-208 | Report cost/stops/adverse results; no automatic spend. | F7; OL-16–18/20 | Caps/unknown costs/zero-budget refusal. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-f-201"></a>REQ-F-201 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves §10.4 noncumulative levels; tested recipient procedures | E/P |
| <a id="req-f-202"></a>REQ-F-202 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves §10.4 noncumulative levels; tested recipient procedures | E/P |
| <a id="req-f-203"></a>REQ-F-203 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves §10.4 noncumulative levels; tested recipient procedures | E/P |
| <a id="req-f-204"></a>REQ-F-204 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves §10.4 noncumulative levels; tested recipient procedures | E/P |
| <a id="req-f-205"></a>REQ-F-205 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves §10.4 noncumulative levels; tested recipient procedures | E/P |
| <a id="req-f-206"></a>REQ-F-206 | adopted | [§6](#s6); [AC-6-2](#ac-6-2) | preserves §10.4 noncumulative levels; tested recipient procedures | E/P |
| <a id="req-f-207"></a>REQ-F-207 | adopted | [§6](#s6); [AC-6-3](#ac-6-3) | preserves §10.4 rights/loss/supersession; Q-5 retention | D/E |
| <a id="req-f-208"></a>REQ-F-208 | adopted | [§6](#s6); [AC-6-3](#ac-6-3), [AC-9-1](#ac-9-1) | preserves quote/exposure/access limits, no inherited spending | D/E |

### A.G1 — [G1 source](design/17_reader-journey.md)

| ID | Candidate requirement / rationale | Finding / outline | Proposed acceptance |
|---|---|---|---|
| REQ-G-001 | Provide the concise evaluator spine, three entry choices and direct failure/report branches; reuse stable guides. | F1; OL-01, OL-18–OL-19 | Navigation review and G3 observations reach each branch without inventory/history prerequisite. |
| REQ-G-002 | Give every example a versioned prerequisite/command/output/failure/next-step card and distinct semantic completion. | F1/F2; OL-05, OL-08, OL-17, OL-20 | F2 clean-environment checks verify exact recipes and negative cases; first value requires new independently checked artifact. |
| REQ-G-003 | Bind the annotated walkthrough to actual task/receipt/event identities and separate lifecycle, grade, veto and infrastructure result. | F3; OL-06, OL-15 | Event excerpt/link checks, correct/wrong/oracle-failure examples and reader explanation; no invented ordering/classes. |
| REQ-G-004 | Include one proved editable harness field and explicit validate/compile/rerun loop without causal-efficacy inference. | F2/F3; OL-05–OL-06, OL-17 | Consumption/enforcement evidence plus independently retained before/after configurations/attempts. |
| REQ-G-005 | Expose canonical procedure cards for verification, recomputation, replay and fresh rerun, including distinct R0 scope. | F4; OL-07, OL-16 | F3 supplies single-source recipes/tolerances; reader selects correct procedure and reports unavailable/mismatch honestly. |
| REQ-G-006 | Gate public content on scoped evidence; separate useful-task release, offline teaching and central real-study readiness. | F4; OL-08–OL-11, OL-15, OL-20 | Publisher matrix rejects baseline CLI/R0 defects, missing integration/treatment/caps and fixture substitutes; accepts sound null report. |
| REQ-G-007 | Make screenshots optional evidence-linked illustrations with equivalent text and tested commands, without a new viewer. | F3; OL-06, OL-17 | Caption/version/receipt/semantic-output match and accessible text review; screenshot cannot stand in for execution evidence. |
| REQ-G-008 | Permit low-cost bug/setup/verification participation and state real component build prerequisites. | F2/F5; OL-13, OL-18 | G2 report path works before successful setup; selected component builds/seals/loads/invokes and rejects tampering. |
| REQ-G-009 | Validate comprehension/usefulness with explicitly authorized branched observation, retaining failures and assistance. | F5; OL-19, OL-24 | G3/Q-6 plan and scoped observations; no time/adoption improvement claim inferred from small cohort or traffic. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-g-001"></a>REQ-G-001 | adopted | [§7](#s7); [AC-7-1](#ac-7-1), [AC-7-2](#ac-7-2), [AC-6-2](#ac-6-2); [§8 participant evidence](#s8), [AC-8-2](#ac-8-2) | supplements reader route; K-2/§10.4/evidence scope unchanged | D/E/P |
| <a id="req-g-002"></a>REQ-G-002 | adopted | [§7](#s7); [AC-7-1](#ac-7-1), [AC-7-2](#ac-7-2), [AC-6-2](#ac-6-2) | supplements reader route; K-2/§10.4/evidence scope unchanged | D/E |
| <a id="req-g-003"></a>REQ-G-003 | adopted | [§7](#s7); [AC-7-1](#ac-7-1), [AC-7-2](#ac-7-2), [AC-6-2](#ac-6-2); [§8 participant evidence](#s8), [AC-8-2](#ac-8-2) | supplements reader route; K-2/§10.4/evidence scope unchanged | D/E/P |
| <a id="req-g-004"></a>REQ-G-004 | adopted | [§7](#s7); [AC-7-1](#ac-7-1), [AC-7-2](#ac-7-2), [AC-6-2](#ac-6-2) | supplements reader route; K-2/§10.4/evidence scope unchanged | D/E |
| <a id="req-g-005"></a>REQ-G-005 | adopted | [§7](#s7); [AC-7-1](#ac-7-1), [AC-7-2](#ac-7-2), [AC-6-2](#ac-6-2); [§8 participant evidence](#s8), [AC-8-2](#ac-8-2) | supplements reader route; K-2/§10.4/evidence scope unchanged | D/E/P |
| <a id="req-g-006"></a>REQ-G-006 | adopted | [§7](#s7); [AC-7-1](#ac-7-1), [AC-7-2](#ac-7-2), [AC-6-2](#ac-6-2) | supplements reader route; K-2/§10.4/evidence scope unchanged | D/E |
| <a id="req-g-007"></a>REQ-G-007 | adopted | [§7](#s7); [AC-7-1](#ac-7-1), [AC-7-2](#ac-7-2), [AC-6-2](#ac-6-2) | supplements reader route; K-2/§10.4/evidence scope unchanged | D/E |
| <a id="req-g-008"></a>REQ-G-008 | adopted | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2) | preserves CONTRIBUTING/OPERATOR/ABI/§10.4; ordinary PR simplicity | D/E/P |
| <a id="req-g-009"></a>REQ-G-009 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements consent/adoption/publication; evidence/Q authority unchanged | D/P |

### A.G2 — [G2 source](design/18_contribution-path.md)

| ID | Candidate / rationale | Finding / outline | Proposed acceptance |
|---|---|---|---|
| REQ-G-101 | Reuse public guides/forms; separate ordinary PR/internal duties. | F1; OL-18–21 | Small PR path needs no Q/ledger; human merge retained. |
| REQ-G-102 | Accept no-run reports and native evidence/access losses. | F2; OL-15–18/20 | No-binary/export-failure/restricted-input report routes work. |
| REQ-G-103 | Preserve original failure; compare candidate for verifyfixed. | F3; OL-07/15–18/21 | Regression fails without fix; old evidence/changed inputs retained. |
| REQ-G-104 | Offer bounded no-provider outside verify/recompute. | F4; OL-07/16/18–20 | Clean recipient reports phases/costs/verdict/relationship; failures retained. |
| REQ-G-105 | Separate fresh replication admission/funding. | F4; OL-10–11/15–16/20/24 | Missing parity/pins/resources prevents program dispatch; no fixture upgrade. |
| REQ-G-106 | At most one optional built/sealed/invoked component. | F5; OL-03/13/17–18/20 | Phase/tamper/no-helper evidence; early return cannot pass required case. |
| REQ-G-107 | Map starter scope to owner/regression/relevant checks. | F1–F5; OL-17–18/21 | Review without new platform; contracts/schema/dependencies routed correctly. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-g-101"></a>REQ-G-101 | adopted | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2) | preserves CONTRIBUTING/OPERATOR/ABI/§10.4; ordinary PR simplicity | D/E/P |
| <a id="req-g-102"></a>REQ-G-102 | adopted | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2) | preserves CONTRIBUTING/OPERATOR/ABI/§10.4; ordinary PR simplicity | D/E/P |
| <a id="req-g-103"></a>REQ-G-103 | adopted | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2) | preserves CONTRIBUTING/OPERATOR/ABI/§10.4; ordinary PR simplicity | D/E/P |
| <a id="req-g-104"></a>REQ-G-104 | adopted | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2) | preserves CONTRIBUTING/OPERATOR/ABI/§10.4; ordinary PR simplicity | D/E/P |
| <a id="req-g-105"></a>REQ-G-105 | adopted | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2) | preserves CONTRIBUTING/OPERATOR/ABI/§10.4; ordinary PR simplicity | D/E/P |
| <a id="req-g-106"></a>REQ-G-106 | adopted | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2) | preserves CONTRIBUTING/OPERATOR/ABI/§10.4; ordinary PR simplicity | D/E/P |
| <a id="req-g-107"></a>REQ-G-107 | adopted | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2) | preserves CONTRIBUTING/OPERATOR/ABI/§10.4; ordinary PR simplicity | D/E/P |

### A.G3 — [G3 source](design/19_distribution-and-adoption.md)

| ID | Candidate / finding / outline | Proposed acceptance |
|---|---|---|
| REQ-G-201 | Gate claims, retain study; F1, OL-09/11/15/19–21/24. | Missing admission blocks claims; negative study can publish. |
| REQ-G-202 | Version report/losses/corrections; F2, OL-07/10–11/15–17/19. | Claim→receipt audit retains adverse/unknown entries. |
| REQ-G-203 | Outside clean verify/recompute; F3, OL-16/18–20. | Installation/signer/both reports; mismatch is not pass. |
| REQ-G-204 | Consented bounded branch observation; F3, OL-01/19–20/24. | 4–6 sessions or shortfall; caps/original/recheck retained. |
| REQ-G-205 | Use/contribution denominators; F4, OL-15/18–20. | Missing/assisted/censored visible; traffic not adoption. |
| REQ-G-206 | Bounded manual 30/90 review; F4, OL-19–20. | Recommendation/unknowns/cost; no schedule/spend implied. |
| REQ-G-207 | Channels fit readiness/rules; F5, OL-04/18–20. | GitHub-first; authorized human-authored wider channel. |
| REQ-G-208 | Minimal consented data; F6, OL-16/19–20. | Opt-out/expiry/rights; separate receipt retention. |
| REQ-G-209 | Adverse results/corrections/freshness; F6, OL-11/15–21. | Original/superseding report and updated claims. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-g-201"></a>REQ-G-201 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements consent/adoption/publication; evidence/Q authority unchanged | D/P |
| <a id="req-g-202"></a>REQ-G-202 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements consent/adoption/publication; evidence/Q authority unchanged | D/P |
| <a id="req-g-203"></a>REQ-G-203 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements consent/adoption/publication; evidence/Q authority unchanged | D/P |
| <a id="req-g-204"></a>REQ-G-204 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements consent/adoption/publication; evidence/Q authority unchanged | D/P |
| <a id="req-g-205"></a>REQ-G-205 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements consent/adoption/publication; evidence/Q authority unchanged | D/P |
| <a id="req-g-206"></a>REQ-G-206 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements consent/adoption/publication; evidence/Q authority unchanged | D/P |
| <a id="req-g-207"></a>REQ-G-207 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements consent/adoption/publication; evidence/Q authority unchanged | D/P |
| <a id="req-g-208"></a>REQ-G-208 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements consent/adoption/publication; evidence/Q authority unchanged | D/P |
| <a id="req-g-209"></a>REQ-G-209 | adopted | [§8](#s8); [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | supplements consent/adoption/publication; evidence/Q authority unchanged | D/P |

### A.H1 — [H1 source](design/20_build-handoff-design.md)

| ID | Candidate / rationale | Findings / outline | Acceptance |
|---|---|---|---|
| REQ-H-001 | Preserve completed build/frozen contract/OPERATOR merges. | F1/F3; OL-21/24 | History diff, explicit extension/base. |
| REQ-H-002 | Separate supplement authority; retain candidate aliases. | F2; OL-21/23 | Candidate/source set equality. |
| REQ-H-003 | Trace OL/candidate/AC to one owner/disposition. | F2; OL-14/23 | Guard rejects gaps/duplicate owners. |
| REQ-H-004 | Route changed semantics through owned ADR/amendments. | F3; OL-20/21 | Section/decision/approval audit. |
| REQ-H-005 | Size packages by actual coupling/context. | F3/F4; OL-20/22 | Fresh partition review; no fixed count/days. |
| REQ-H-006 | Append unique numbered contracts/full-tail roles. | F4; OL-21/24 | Grammar/ordering and new signature. |
| REQ-H-007 | Disposition old warnings; enforce new integrity/routing. | F2/F5; OL-21/22 | Baseline/history/guard; old-gate negative test. |
| REQ-H-008 | Require readiness packet and checked dispatch path. | F2/F6; OL-20–24 | Guard/fixture/preflight evidence before dispatch. |
| REQ-H-009 | Separate implementation/research/adoption acceptance. | F6; OL-11/15–17 | Null/negative valid; unsupported claims withheld. |
| REQ-H-010 | Independently disposition parity/method feasibility. | F7; OL-09–11/24 | Reviewed methods/resources or limited scope. |
| REQ-H-011 | Preserve checkpoint order/state-writer/fresh-context duties. | F1/F7; OL-22–24 | Timestamped phase-entry audit. |

| Literal ID | Disposition / target | Governing section and AC hook | Legacy relation | Layer |
|---|---|---|---|---|
| <a id="req-h-001"></a>REQ-H-001 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves frozen canonical/legacy namespace/history/OPERATOR workflow | D/E |
| <a id="req-h-002"></a>REQ-H-002 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves frozen canonical/legacy namespace/history/OPERATOR workflow | D/E |
| <a id="req-h-003"></a>REQ-H-003 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves frozen canonical/legacy namespace/history/OPERATOR workflow | D/E |
| <a id="req-h-004"></a>REQ-H-004 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves frozen canonical/legacy namespace/history/OPERATOR workflow | D/E |
| <a id="req-h-005"></a>REQ-H-005 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves frozen canonical/legacy namespace/history/OPERATOR workflow | D/E |
| <a id="req-h-006"></a>REQ-H-006 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves frozen canonical/legacy namespace/history/OPERATOR workflow | D/E |
| <a id="req-h-007"></a>REQ-H-007 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves frozen canonical/legacy namespace/history/OPERATOR workflow | D/E |
| <a id="req-h-008"></a>REQ-H-008 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves frozen canonical/legacy namespace/history/OPERATOR workflow | D/E |
| <a id="req-h-009"></a>REQ-H-009 | adopted | [§9](#s9); [AC-9-3](#ac-9-3) | preserves distinct engineering/research/adoption acceptance | D/R/P |
| <a id="req-h-010"></a>REQ-H-010 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves frozen canonical/legacy namespace/history/OPERATOR workflow | D/E |
| <a id="req-h-011"></a>REQ-H-011 | adopted | [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | preserves frozen canonical/legacy namespace/history/OPERATOR workflow | D/E |


<a id="appendix-b"></a>
## Appendix B. Operator decisions and current gates

The ledger remains the operator-decision authority. These are its Q rows, preserved from synthesis with Q-7 updated at ratification; exact approval provenance/wording is in its dated change log and brief addenda. Earlier unresolved language in notes is historical. Q-3–Q-6 approve planning policies only; §9.2 execution-specific inputs remain gates, not unanswered broad design choices.

| ID | Question | Recorded decision | Unlocks |
|---|---|---|---|
| Q-1 | Accept this charter/ledger and begin the research phase after Zed review? | Approved 2026-10-29; exact operator wording recorded in the change log and brief addendum. | A1–A3, B1–B2 and subsequent dependency-ready work |
| Q-2 | Which audience and first-wave portfolio, effort envelope and bounded spikes should we commit to? | Approved 2026-10-29: recommended audience/portfolio, restricted-runner investigation and first offline tranche in C2 F5; exact wording/scope in change log. | D–G selected scope; E2 spike envelope |
| Q-3 | Which task suite/version, graders, environments and model/provider matrix should the study use, given current contracts and feasibility? | Approved planning policy: TB2/unchanged upstream tasks and oracle, one environment and fixed supported provider/roles, post-terminal grading and finite-task target; authorize D4 before H2. Exact pins/backend/method admission remain later gates. Exact confirmation/time in change log. | Final D2/E3 design; any required ADR proposal |
| Q-4 | What pilot/confirmatory budgets, resource ceilings and minimum useful effects should apply, and should any runs occur before implementation? | Approved planning policy: $0/no task-provider or cloud-runtime execution now; 60/300 subjects and caps conditional, margins provisional pending D4. One fresh-context source review/bounded analytic note authorized; no tasks, installs or production implementation. Exact confirmation/time in change log. | Final protocols; separately authorized execution only |
| Q-5 | Which artifacts may be redistributed, where should large receipts live, and what retention/sanitization constraints apply? | Approved policy: rights-cleared public index/code/analysis, encrypted controlled staging; optional hosting separately approved. Reports five years/assets two years/permitted controlled review originals 12 months, subject to shorter rights and explicit losses; actual rights/owner/host/quotas/retention quote remain gates. Exact confirmation/time in change log. | Final F1/F3 publication and replication contract |
| Q-6 | What outside replication, contributor and publication scope is appropriate for launch, and how should adoption be evaluated? | Approved design scope: one outside verify/recompute reviewer then 4–6 formative evaluators, 60-minute sessions, 24-hour maintainer ceiling including manual 30/90-day reviews, GitHub first. No recruitment/publication/paid replication or automation authorized. Exact confirmation/time in change log. | Final G2/G3 scope; no automatic outreach authorization |
| Q-7 | Ratify the reviewed supplemental spec, deliberate deferrals and amendment scope, then hand it to decomposition? | Approved 2026-10-30T02:05:32Z, current Codex chat: “Looks good, proceed”. Ratifies reviewed v0.2 and all five Q-7 guide decisions; exact input digests/scope in the change log. Execution and implementation gates remain. | Complete O3 freeze/handoff; later ticket/build-artifact phase |

**Q-7 is resolved.** At 2026-10-30T02:05:32Z, the operator approved the reviewed draft and all five guide decisions through the exact message “Looks good, proceed” in the current Codex chat. This ratifies the first-wave scope and later dispositions, integration/amendment scope, reviewed methods direction, retained execution gates and downstream freeze/handoff. O2-01/O2-02 were independently closed before approval; Appendix E preserves every disposition. No new design scope or favorable scientific outcome is inferred. R1–R9 and S10–S12 are in §0.3; proposed amendments in §9.3 still require ADR/manifest admission. The current task completes ratification and its handoff; decomposition/tooling and implementation remain distinct downstream stages.

<a id="appendix-c"></a>
## Appendix C. Accepted source index

Hashes are SHA-256 of accepted note bytes from the research ledger, checked against disk during O1 assembly. They are source identities, not product execution evidence. The 21 emitting notes supply 180 candidates; H2 is the 22nd accepted note and emits none. Sources retain their own inspected evidence, status and limitations.

| Unit | Accepted note | SHA-256 | Candidate count |
|---|---|---|---:|
| A1 | [01_capability-inventory](research/01_capability-inventory.md) | `6943537a8a135c0887da3369ca338a503d9c9f2f5091b99845701950acfbfe24` | 8 |
| A2 | [02_evidence-audit](research/02_evidence-audit.md) | `55f6f628cd57db17c888bfdd202645123229a92e388467c05f5d9c11dc1f887d` | 6 |
| A3 | [03_newcomer-journey](research/03_newcomer-journey.md) | `5497c3ae3501cec553996406ad6a66348bf952397c6505f445bb9c7c05652ce9` | 5 |
| B1 | [04_audiences-and-jobs](research/04_audiences-and-jobs.md) | `6ceffe781845086f1f1447be25939aca7c2358e3ad8d4983be65bced8e717a3d` | 7 |
| B2 | [05_ecosystem-and-benchmarks](research/05_ecosystem-and-benchmarks.md) | `e3804e189e48fc388263798f73b89fbf7d24e7f1e8cac885d647a6a5bd416525` | 7 |
| C1 | [06_demo-and-study-portfolio](design/06_demo-and-study-portfolio.md) | `fedafb0c482b25c8ffc79f629fdb22d729bc62b905a72da11fd100c139ef4c78` | 8 |
| C2 | [08_feasibility-and-scope](design/08_feasibility-and-scope.md) | `34744883c496491688d4aac8d945e4b2fd944eefeeadac57ffef6716a6b850f3` | 8 |
| D1 | [09_measurement-standard](design/09_measurement-standard.md) | `e727f26dabaae7578f4836e376c382e378104a357a1710108978b86dcc467f13` | 10 |
| D2 | [10_compaction-protocol](design/10_compaction-protocol.md) | `8e92203455d3f48219338b8c0bf25d111c9d69e3ea22841cd5d93f8766a028ec` | 10 |
| D3 | [11_operational-proof-protocols](design/11_operational-proof-protocols.md) | `5f4e9297713f119ff0c54499b41ed6ea123489e346b07514a0005334aec6b382` | 9 |
| D4 | [23_parity-method-review](design/23_parity-method-review.md) | `9e3fc4535aa11f3b07a05e75423a093d740ec85a31c6343d61be250e665e47db` | 10 |
| E1 | [07_execution-seams](design/07_execution-seams.md) | `a46daa783ed5f154eb0765006677064260fc04e390b6b7d9fb311100d9dcfcc8` | 12 |
| E2 | [12_feasibility-findings](design/12_feasibility-findings.md) | `2840f787790cfdabfad0770b49c15ba984aeb0464a4daacc71ce860fc11e99d8` | 10 |
| E3 | [14_integration-design](design/14_integration-design.md) | `8938931bc85c6b2c60339943d998d62bebfadc687248893ccda0fd4357fdec9a` | 10 |
| F1 | [13_receipt-contract](design/13_receipt-contract.md) | `91be56ea1c7ad91da5c90733ec3c6edb7bafa66f23563422812be06db3f0769b` | 8 |
| F2 | [15_examples-and-maintenance](design/15_examples-and-maintenance.md) | `18f55e473c65a258ae30cf49bc5a6c28e344588a542b7fc8813ff9aa6e410eab` | 8 |
| F3 | [16_replication-and-artifacts](design/16_replication-and-artifacts.md) | `5330da367a3e2e42291376803f2fb5a0d9986016f8f7adb0ffb6236486298094` | 8 |
| G1 | [17_reader-journey](design/17_reader-journey.md) | `e7cd3506e62ef74310808e7fdf14d4e924a1944b9311c093367d092e87b7b69a` | 9 |
| G2 | [18_contribution-path](design/18_contribution-path.md) | `53b46810bed2fca4790760e0c5990bac011d885e5cde06bd2319915cdff0f12e` | 7 |
| G3 | [19_distribution-and-adoption](design/19_distribution-and-adoption.md) | `a419cf3e64d22f53ac1b4b6f908dc7e3b817f980ade8e26426b41be0a0e0c508` | 9 |
| H1 | [20_build-handoff-design](design/20_build-handoff-design.md) | `8e87a13be8914d9c7e47cc3ab27faf0c62d1c8b1a5ea6ed67f8c20c71e13aa25` | 11 |
| H2 | [21_synthesis-readiness](design/21_synthesis-readiness.md) | `1ec1cb9ec5c6aef81b8a49a0af722edab9a21b2a031a38ce7a408f4326d20797` | 0 |

<a id="appendix-d"></a>
## Appendix D. Glossary and reserved strings

| Term/string | Meaning and restriction |
|---|---|
| E1 / kernel; Lab | Rust reference runtime; experiment/evaluation/analysis services. CLI is client/renderer, not kernel semantics. |
| `hh-embed/1`, HIR/1, `idp/1` | Existing typed boundary, harness representation and identity protocol. Reuse existing owners/codecs. |
| `evidence-package/1` | Proposed native-bundle profile; not an implemented public format until admission. |
| `execution_settled`, `grade_pending`, `settle_graded`, `plan_disposition_at` | Proposed interface/state semantics for owning-layer admission; pending is admission state, not outcome class or new public RPC promise. |
| `scored`, `budget_exhausted`, `infrastructure_failure`, `oracle_failure`, `refused`, `cancelled` | Six preserved outcome classes, distinct from lifecycle/reward/veto/availability. |
| `open`, `finished` | Bundle lifecycle vocabulary for R0; terminal task wording does not replace it. |
| `fixture-verified`, `foreign-run-verified`, `live-executed` | Scoped existing evidence classes. Live requires ADR-0363 record; foreign requires recorded corpus replay. No fixture/skip upgrade. |
| R0/R1/R2/R3; independence | Native noncumulative basis/levels; actual procedure/verdict/closure and distinct installation/signer are separate. |
| `Unsupported`, `Refused`, `n/a{reason}`, `no_detector`, `estimator_undefined`, `InsufficientReplicates` | Existing typed refusal/unavailability vocabulary as appropriate to owning API; never fabricated success/zero. |
| `not_run{recipe_unimplemented}`, `refresh due` | Descriptive recipe/readiness labels, not invented native events or evidence classes. |
| `same_snapshot`, `cold_start`, `matched_cap`, `search_budget=0`, `fault_profile=none` | Frozen S3 factors; tolerance .10 consumption reporting differs from statistical margin. |
| `tokens.input.total`, `tokens.output.total`, `model_calls`, `time.wall_ms` | Four matched dimensions; token totals derived, physical use/exposure retained. |
| Finite manifest / task-mixture sensitivity / sharp null | Distinct §4 estimands and assumptions; no mixed omnibus or transfer certificate. |
| OL / REQ / R-2.* / ADR / DF / BL | Founding obligation / supplemental candidate / legacy requirement / architecture decision / deferral / backlog. Namespaces/owners/history remain distinct. |
| P1–P8 / S1–S6 / O1–O8 / EX-01–04 | Design packages / scenarios / operational cells / example families. These do not allocate manifest ticket IDs. Operational O1–O8 differs from synthesis process O1–O3. |
| `mergePolicy: OPERATOR`; RETURN PASS | Human merge authority; pending operator decision route. Agent signatures/old approvals cannot substitute. |

Unused REQ family numbers and future ticket/ADR identifiers remain reserved/unallocated. No future page/tool path mentioned as a proposal is a clickable promise of existence.

<a id="appendix-e"></a>
## Appendix E. Independent review dispositions

The [independent O2 review](design/22_independent-spec-review.md) read v0.1 from a fresh context, found two P2 issues and no established P0/P1 defect, and checked the source/requirement/outline identities and bounded numerical cases. The original findings remain unchanged in that note. This v0.2 applies the dispositions below; the note's dated delta-rereview records the actual corrected digest and closure verdict. Q-7 remains an operator decision.

| Finding | Disposition | Correction / evidence and closure condition |
|---|---|---|
| O2-01: one direction versus round trip | adapt | S12 explicitly replaces only REQ-B-102's retained round-trip predicate. §1.4/AC-1-2 and its registry row define one native→external-reader exercise and withhold reverse/round-trip claims; C-007 shares the hook. Verify that a successful export/read without return import has one unambiguous bounded pass. |
| O2-02: participant predicates not routed | accept | §8.4 and Appendix A route B-107, G-001/G-003/G-005 to AC-8-2 P evidence while preserving one primary owner. Analogous A-201/C-007 reuse the same cohort; B-001 is D-only portfolio selection, with empirical demand claims still governed by §8.4. With no observations, the participant predicates remain unmet despite passing guide/receipt tests; no extra cohort is required. |
| C-01: constant binary data versus generic degenerate control (coordinator) | adapt | §7.2 makes controls estimator-specific. Valid all-success/all-failure Beta/KL cells retain uncertainty; undefined bootstrap behavior stays explicitly undefined. This clarifies F2's generic control language under the selected D2/D4 methods. |
| C-02: offline envelope scope (coordinator) | adapt | §9.2 scopes the 8-hour/32-core-hour/2-GiB envelope to D2 numerical validation, not all build/provisioning/synthetic-workflow activity. Other existing envelopes and authorization remain separate; no scope or budget increase. |
| C-03: deliberately violated O8 invariant (coordinator) | adapt | §5.2 requires each case's expected assertions. O8 passes detector acceptance only when the seeded invariant violations are detected, never by claiming its sink invariant held. |

The initial independent review found S10's early-method dependency and S11's equivalence/negative-confidence separation coherent within its stated limits, including the relative-contrast negative branches. It did not validate future production numerics, actual independence, endpoint applicability, runtime integration, costs, rights or participation. Those admission/acceptance gates remain normative. The coordinator dispositions are not substitutes for the dated independent delta-rereview; no unresolved substantive finding may be waived into ratification.

<a id="appendix-f"></a>
## Appendix F. All 24 founding obligations: ratified coverage proof

Every obligation below has a concrete destination and acceptance hook; no OL is silently dropped. Q-7 ratifies this specification coverage, not executed acceptance. The frozen OUTLINE_TRACE records all 24 obligations as covered by the specification. Later-family disposition meets planning coverage while preserving inherited obligations, not funding a study. Appendix A supplies source/finding and legacy relations; later decomposition adds one owning ticket and evidence.

| OL | Founding obligation (verbatim) | Ratified sections / AC hooks | Principal literal requirements | Status |
|---|---|---|---|---|
| OL-01 | Explain what the project is, why it helps and who should use it. | [§1](#s1), [§7](#s7), [§8](#s8); [AC-1-1](#ac-1-1), [AC-7-1](#ac-7-1), [AC-8-2](#ac-8-2) | REQ-B-001, REQ-G-001, REQ-G-009 | Covered by ratified specification; execution remains owed. |
| OL-02 | Audit implemented capabilities, accessible interfaces, maturity and honest limitations. | [§1](#s1); [AC-1-1](#ac-1-1) | REQ-A-001→REQ-C-102, REQ-A-007 | Covered by ratified specification; execution remains owed. |
| OL-03 | Critically evaluate the alternative proposal and retain only justified ideas. | [§1](#s1), [§5](#s5), [§7](#s7), [§8](#s8); [AC-1-1](#ac-1-1), [AC-5-2](#ac-5-2), [AC-7-1](#ac-7-1), [AC-8-1](#ac-8-1) | REQ-C-001→REQ-C-102, REQ-C-008, REQ-G-106 | Covered by ratified specification; execution remains owed. |
| OL-04 | Research adjacent work and integration opportunities using primary sources. | [§1](#s1), [§8](#s8); [AC-1-1](#ac-1-1), [AC-8-2](#ac-8-2), [AC-1-2](#ac-1-2) | REQ-B-101, REQ-B-102, REQ-B-103, REQ-B-104, REQ-B-105, REQ-B-106, REQ-B-107 | Covered by ratified specification; execution remains owed. |
| OL-05 | Demonstrate a useful task with independently checkable output. | [§2](#s2), [§5](#s5), [§7](#s7); [AC-2-1](#ac-2-1), [AC-5-1](#ac-5-1), [AC-7-1](#ac-7-1) | REQ-E-201, REQ-E-202, REQ-E-203, REQ-E-204, REQ-E-205, REQ-D-201, REQ-G-004 | Covered by ratified specification; execution remains owed. |
| OL-06 | Explain a run through an annotated, inspectable trace. | [§2](#s2), [§7](#s7); [AC-2-1](#ac-2-1), [AC-7-1](#ac-7-1); [§8](#s8), [AC-8-2](#ac-8-2) | REQ-A-203, REQ-G-003, REQ-F-105 | Covered by ratified specification; execution remains owed. |
| OL-07 | Separate replay, bundle verification, analysis recomputation and fresh rerun guarantees. | [§6](#s6); [AC-6-1](#ac-6-1), [AC-6-2](#ac-6-2); [§8](#s8), [AC-8-2](#ac-8-2) | REQ-F-005, REQ-F-007, REQ-F-201, REQ-F-202, REQ-F-203, REQ-F-204, REQ-F-205 | Covered by ratified specification; execution remains owed. |
| OL-08 | Teach the Lab with a small runnable offline experiment and explicit fixture limits. | [§7](#s7), [§9](#s9); [AC-7-1](#ac-7-1), [AC-9-1](#ac-9-1) | REQ-F-101, REQ-F-102, REQ-F-103 | Covered by ratified specification; execution remains owed. |
| OL-09 | Make a real controlled harness comparison central to the first-wave proposal. | [§4](#s4); [AC-4-1](#ac-4-1), [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | REQ-C-004, REQ-D-101, REQ-D-105, REQ-D-301, REQ-D-308 | Covered by ratified specification; execution remains owed. |
| OL-10 | Specify rigorous measurement, controls, accounting, uncertainty and pilot/confirmation separation. | [§2](#s2), [§3](#s3), [§4](#s4); [AC-2-3](#ac-2-3), [AC-3-1](#ac-3-1), [AC-4-1](#ac-4-1), [AC-4-2](#ac-4-2), [AC-4-3](#ac-4-3), [AC-4-4](#ac-4-4) | REQ-D-001, REQ-D-002, REQ-D-003, REQ-D-005, REQ-D-006, REQ-D-007, REQ-D-008, REQ-D-009, REQ-D-102, REQ-D-107, REQ-D-109, REQ-D-302, REQ-D-303, REQ-D-304, REQ-D-305, REQ-D-306, REQ-D-307 | Covered by ratified specification; execution remains owed. |
| OL-11 | Treat null, negative and inconclusive results as valid; separate research completion from default adoption. | [§3](#s3), [§4](#s4), [§8](#s8), [§9](#s9); [AC-3-2](#ac-3-2), [AC-4-4](#ac-4-4), [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | REQ-C-005, REQ-H-009 | Covered by ratified specification; execution remains owed. |
| OL-12 | Design bounded reliability, recovery and authorization evidence with explicit assumptions. | [§5](#s5); [AC-5-2](#ac-5-2) | REQ-D-203, REQ-D-204, REQ-D-205, REQ-D-206, REQ-D-207, REQ-E-205 | Covered by ratified specification; execution remains owed. |
| OL-13 | Demonstrate an actual extension/conformance path with realistic packaging requirements. | [§8](#s8); [AC-8-1](#ac-8-1) | REQ-A-006, REQ-D-209, REQ-G-106 | Covered by ratified specification; execution remains owed. |
| OL-14 | Disposition every major capability family, including later-wave studies, with reasons. | [§1](#s1), [§9](#s9); [AC-1-1](#ac-1-1), [AC-9-2](#ac-9-2) | REQ-C-102, REQ-E-210 | Covered by ratified specification; execution remains owed. |
| OL-15 | Distinguish evidence classes, task success, infrastructure success and scientific claims; preserve failures. | [§3](#s3), [§6](#s6), [§8](#s8); [AC-3-1](#ac-3-1), [AC-6-1](#ac-6-1), [AC-8-2](#ac-8-2), [AC-9-3](#ac-9-3) | REQ-A-004, REQ-A-102, REQ-F-003, REQ-F-008 | Covered by ratified specification; execution remains owed. |
| OL-16 | Define versioned receipts, executable analysis and independent replication with practical prerequisites. | [§6](#s6); [AC-6-1](#ac-6-1), [AC-6-2](#ac-6-2), [AC-6-3](#ac-6-3), [AC-1-2](#ac-1-2) | REQ-F-001, REQ-F-002, REQ-F-003, REQ-F-004, REQ-F-005, REQ-F-006, REQ-F-007, REQ-F-008, REQ-F-201, REQ-F-202, REQ-F-203, REQ-F-204, REQ-F-205, REQ-F-206, REQ-F-207, REQ-F-208 | Covered by ratified specification; execution remains owed. |
| OL-17 | Design executable examples, CI drift detection and an affordable refresh policy. | [§7](#s7); [AC-7-1](#ac-7-1), [AC-7-2](#ac-7-2) | REQ-F-101, REQ-F-102, REQ-F-103, REQ-F-104, REQ-F-105, REQ-F-106, REQ-F-107, REQ-F-108 | Covered by ratified specification; execution remains owed. |
| OL-18 | Provide a contribution and reproducible bug-report path. | [§8](#s8); [AC-8-1](#ac-8-1), [AC-6-2](#ac-6-2) | REQ-G-101, REQ-G-102, REQ-G-103, REQ-G-104, REQ-G-105, REQ-G-106, REQ-G-107 | Covered by ratified specification; execution remains owed. |
| OL-19 | Plan honest communication, community participation and measurable adoption. | [§8](#s8); [AC-8-2](#ac-8-2) | REQ-G-201, REQ-G-202, REQ-G-203, REQ-G-204, REQ-G-205, REQ-G-206, REQ-G-207, REQ-G-208, REQ-G-209, REQ-B-005, REQ-B-006, REQ-B-007 | Covered by ratified specification; execution remains owed. |
| OL-20 | Account for engineering seams, dependencies, costs, licensing, privacy and resource decisions. | [§2](#s2), [§6](#s6), [§9](#s9); [AC-2-1](#ac-2-1), [AC-2-3](#ac-2-3), [AC-6-3](#ac-6-3), [AC-9-1](#ac-9-1) | REQ-C-103, REQ-C-104, REQ-C-105, REQ-C-106, REQ-C-107, REQ-E-210, REQ-F-207, REQ-F-208 | Covered by ratified specification; execution remains owed. |
| OL-21 | Preserve frozen contracts, history and existing gates; use compatible ADR/spec/build workflows. | [§0](#s0), [§9](#s9); [AC-9-1](#ac-9-1), [AC-9-2](#ac-9-2) | REQ-H-001, REQ-H-004, REQ-H-006, REQ-H-007 | Covered by ratified specification; execution remains owed. |
| OL-22 | Support bounded fresh-context work with a single state writer and durable evidence. | [§9](#s9); [AC-9-2](#ac-9-2) | REQ-H-005, REQ-H-008, REQ-H-011 | Covered by ratified specification; execution remains owed. |
| OL-23 | Trace founding intent through research and supplemental requirements to later tickets and acceptance evidence. | [§9](#s9); [AC-9-2](#ac-9-2) | REQ-H-002, REQ-H-003, REQ-H-008 | Covered by ratified specification; execution remains owed. |
| OL-24 | Require independent review and explicit operator checkpoints before implementation orchestration. | [§9](#s9); [AC-9-2](#ac-9-2), [AC-9-3](#ac-9-3) | REQ-C-108, REQ-H-008, REQ-H-010, REQ-H-011 | Covered by ratified specification; execution remains owed. |
