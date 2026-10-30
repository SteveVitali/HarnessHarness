# 01_capability-inventory — Implemented surfaces and their limits

Stream: A — Capability and evidence audit  
Researched: 2026-10-29 UTC  
Author: /root/a1_capabilities  
Ledger rows covered: A1  
Repository baseline: `001490c14a6568972d16a663bb04c07a4255d30d`; uncommitted charter, ledger, trace, conventions and documentation-navigation planning changes were present. No implementation changes were made.  
Confidence overall: high for inspected dispatch and request shapes; medium for breadth of family coverage and completeness of composed execution paths.

The runtime exposes substantially more than a basic agent loop: durable runs, mediated effects, context and control variants, recovery, replay, Lab scheduling and analysis, extension contracts and experimental governance services. These are different kinds of capability. A callable operation, a library implementation, a fixture battery and a recorded live run must not collapse into one maturity score. The most immediate technical issue found is a mismatch between several Lab CLI request shapes and the implemented embed handlers (F7).

## Inspection scope and reading rules

Source inspection occurred **2026-10-29T18:08:34Z–2026-10-29T18:11:42Z** at the baseline above. Each finding's `Retrieved` field refers to this actual inspection interval; the historical executions in F12 have their own recorded times and revisions. `VERIFIED` below means the stated source or record fact was inspected, not that code was executed in this research unit.

Started with `bash scripts/deferrals-open.sh`: exit 0, empty output (zero currently indexed open rows). Read root and documentation instructions, the crate map, founding brief and approval addendum, A1/dependencies, conventions, outline trace and the synthesis run workflow. Inspected bounded source sections, selected test bodies and test scope headers. Contract references below name sections of the [frozen canonical specification](../../spec/CANONICAL_SPEC.md); they describe obligations, not independent implementation evidence. Source comments retaining old stage/deferral language are subordinate to current function bodies and later records.

**No builds, Rust tests, examples, provider calls or live legs were executed.** Test-file links are inspection anchors, not new passes. Historical receipts were read, not replayed or independently revalidated. This is the capability inventory; A2 owns the exhaustive receipt audit and E1 owns the full executor/grader seam trace.

## Capability matrix

Each row is indexed to the material findings below. “Fixture” means inspected test coverage uses scripted inputs, planted records or fake providers; it does not award a fresh `fixture-verified` result. “Library” means an owning Rust entry point exists; it does not imply an equivalent ready-to-use CLI workflow.

| Family | Reachable surface / owner | Contract anchor | Inspected coverage and practical limit | Finding |
|---|---|---|---|---|
| Run lifecycle and durable history | `hh run start/status/events/inspect/resume/cancel`; `open_session`, `submit`, `read`, `project`, `verify`; `EmbedService` | R-2.2.1; R-2.11.4 | Scripted boundary lifecycle; selected historical live bindings. Read success is distinct from task outcome. | F1, F12 |
| Definition assembly, identity and registry | `hh definition plan/apply/diff/explain/validate/identity/compile`; `hh registry`; `lab.assembly.*`, `lab.registry.*` | R-2.1.2–4; R-2.10.1–2 | Assembly source differs from bare HIR. Typed diagnostics and fixture conformance. | F1 |
| Providers, model profiles, routing and caches | Definition `model_port`; `GatewayModel`; gateway/budget/context crates; CLI `profile` family | R-2.3.1–4 | Local-compatible, OpenAI and Anthropic lane implementations; selected real-call receipts, fake-provider tests; provider capability differences remain typed. Routing/cache breadth is mapped, not exhaustively audited here. | F3, F12 |
| Context building and compaction | Definition `context_policy`/compaction bindings; `KernelAssembler`, context policy and compaction families | R-2.4.1–2 | Real builder wiring with fixtures; summarizer/provider ports. A compaction policy's existence does not establish its quality effect. | F3 |
| Retrieval, memory lifecycle and consolidation | `MemoryStore`, indexed retrieval, definition consolidation rules, memory-store ABI | R-2.4.3–4 | Boundary tests exercise durable procedure/memory rows and rehydration; fake/recorded embedding inputs. | F3 |
| Skills and Procedure IR | Procedure supplies; compiler targets; `hh/workflow`, `hh/program` | R-2.4.5 | Procedure selection/activation fixture coverage and plan-walker implementation; not proof of arbitrary imported skill success. | F3, F4 |
| Control and planning | Definition `control_strategy`; minimal/steerable/plan-execute/workflow/program variants; durable steering | R-2.6.1–2 | Source dispatch plus scripted driver/steering tests. Unknown variants refuse. | F4 |
| Delegation, topology and coordination | `hharness_subagent::spawn`; `Orchestrator`; driver delegate seam | R-2.6.3; R-2.6.5 | T0/T1/T2/T3/T6 admitted by current topology code; T4/T5/T7 absent from implemented set. Requires bound ports. | F4 |
| Value of computation / budgeted choice | Definition `compute_policy`; `policy_for_configured` | R-2.6.4; R-2.1.6 | Static/uniform/rules/bandit/surface_prior/predictor selected in code; no measured efficiency improvement established here. | F4 |
| Resource budgets and telemetry | Budget definitions; `account`; ledger-derived cost/trace views | R-2.1.6; R-2.9.1 | Reserve-before-spend, attribution and matched-budget contracts; ceilings operate at decision points, not arbitrary effect interruption. | F5 |
| Authorization, approvals, containment and secrets | `hh approval`; `respond_permission`; environment/helper and credential broker | R-2.8.1–7; R-2.5.5 | Typed permission/environment paths and focused attack/secret batteries; no general security certification. | F5 |
| Verification and independent critics | Completion gate, validators/reconcilers; lane-backed judge adapters | R-2.7.1–3 | Scripted completion-gate tests and fake-provider judge tests; model claims are not authoritative grades. | F5 |
| Recovery and environment lifecycle | `suspend`, `compensate`, `heal`, `continue_goal`, `subscribe`; `env.*` | R-2.2.3–5 | Fixture lifecycle/restart cases; unsupported snapshot/hibernate capabilities remain explicit. | F5 |
| Replay, branching and causal attribution | `fork`, `replay`, `counterfactual`; `lab.attribution.*` | R-2.2.4; R-2.9.7 | Reconstruct versus deterministic replay; attribution folds supplied outcomes and can open arms. Fresh causal evidence is a separate task. | F8 |
| Benchmark integration and grading | `BenchmarkAdapter`, `hh-bench-adapter`, `grade` | R-2.9.4 | A/C/D/E adapters are fixture adapters; grade consumes supplied verifier output. A real independent grader bridge must be identified. | F6 |
| Experiment scheduling and matching | `lab.experiment.*`; CLI `experiment` wrappers | R-2.10.3 | Durable launch/settle infrastructure and exemplar documents; composed tests synthesize terminal/grade rows. Several CLI operands cannot reach required params. | F6, F7 |
| Evaluation, results, comparison and analysis | `lab.eval.*`, `lab.results.*`, `lab.analysis.*`, leaderboard methods; Rust analysis kernel | R-2.9.2; R-2.10.4–5 | Class-aware results and estimator plumbing; typed n/a/refusals; CLI `compare report` request mismatch. | F6, F7 |
| Portable bundles and import/export | `hh bundle create/validate/reproduce/show`, `hh run export/import`; `kernel.*` | R-2.9.3 | One inspected clean-store receipt is R0; no inference of fresh task execution or maximal-level reproducibility. | F8, F12 |
| Plugins and external harnesses | `plugin_abi/1`, package sealing/load, `VariantHost`; `lab.extension.*`, `lab.hosting.*` | R-2.12.2; R-2.8.5; R-2.10.6 | Concrete packaged binary flow and hosted fixture composition. Missing binary/helper can skip an ordinary test body. | F9 |
| Fleet and external work sources | `fleet.*`; CLI view/items/item/reconcile/observe/capabilities/records/webhook | R-2.12.6 | C4 boundary, fixture source and signed-webhook lane selection. Not an autonomous organizational outcome claim. | F10 |
| Evolution, debt retirement, model drift and co-evolution | `lab.evolution.*`, debt/model/coevolution/org-policy families | R-2.9.5–8 | Record-driven governed services, some C4-gated; HarnessHarness does not train weights. | F10 |
| Web, MCP, ACP, SDK and foreign conformance | Loopback HTTP; generated clients; `hh-web`, `hh lab serve`, `hh acp`, `hh-xcheck` | R-2.5.4; R-2.11.2–4 | Artifact MCP catalog versus Lab MCP execution must be separated; foreign implementations cover a bounded corpus. | F11 |

## Material findings

### F1 — The shared runtime boundary is real; authoring inputs require care

Claim: Runtime operations share one dispatch across bindings, while definition authoring has its own document shape and registry-backed service.

Status: VERIFIED  
Evidence kind: source inspection  
Evidence: At the baseline, [embed/lib.rs](../../crates/hh-embed/src/lib.rs) lines 1–31 identifies all three bindings and the run writer/attach model; [service.rs](../../crates/hh-embed/src/service.rs) `dispatch` lines 724–1005 shows the actual runtime and Lab method routing. [conformance.rs](../../crates/hh-embed/tests/conformance.rs) `s_open_submit_stream_close_the_ac1_flow` lines 548–602 opens a scripted session, submits, reads durable hash-bearing frames and closes it. [assembly_ops.rs](../../crates/hh-embed/src/assembly_ops.rs) `source_of` decodes `AssemblySource` and delegates to the owning assembly service; [assembly_service.rs](../../crates/hh-lab/tests/assembly_service.rs) lines 1–8 describes codec, dry-run and drift coverage. [reference.md](../reference.md), “CLI entry points,” identifies the bare-HIR versus AssemblySource distinction. Contract: canonical spec R-2.2.1 responsibility, R-2.10.1–2 and R-2.11.4 §2.1–2.2.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: Give each example its exact input schema and boundary; keep CLI code a client, not a new authority or analysis engine.  
Outline delta: CONFIRMS OL-02; EXTENDS OL-17 — example validation must include the input format, not only command spelling.  
Limitations: Test sources were inspected, not run. A dispatch arm alone does not establish every parameter combination or composed workflow.

### F2 — Maturity, protocol opt-in and compiled feature gates are distinct

Claim: Available operations are gated independently by handshake, stable/experimental protocol tier, host capability and compiled feature availability.

Status: VERIFIED  
Evidence kind: source inspection  
Evidence: [ops.rs](../../crates/hh-embed-schema/src/ops.rs) `OpSpec` defines `tier`, `implemented` and `requires_capability`; [service.rs](../../crates/hh-embed/src/service.rs) lines 724–765 orders `NotInitialized`, `ExperimentalRequired`, `CapabilityNotDeclared`, then `Refused{stage_pending}`. [embed Cargo.toml](../../crates/hh-embed/Cargo.toml) enables `tier-c4` by default for optional fleet/evolution/debt crates. [ledger](../../crates/hh-ledger/Cargo.toml), [environment](../../crates/hh-env/Cargo.toml) and [helper](../../crates/hh-helper/Cargo.toml) expose `tier-c1`; their documented absent-tier refusals are explicit. [reference.md](../reference.md), “Rust libraries and feature tiers,” warns that disabling facade defaults does not remove transitive defaults. Contract: CC6/CC7 and R-2.11.4 handshake; protocol stability is not the C0–C5 maturity vocabulary.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: An example receipt must record build/features and negotiated capabilities; do not advertise a feature-reduced build without resolving its dependency graph.  
Outline delta: CONFIRMS OL-02, OL-13.  
Limitations: No tier-off build or dependency-resolution check was executed here. Historical stage comments are not a current list of missing operations.

### F3 — Context and memory are composed into runs, with provider-specific adapter limits

Claim: The runtime wires real context/memory machinery and live model ports, but context-provider capabilities differ by lane and efficacy remains unmeasured by source inspection.

Status: VERIFIED  
Evidence kind: source inspection  
Evidence: [gateway_model.rs](../../crates/hh-embed/src/gateway_model.rs) lines 1–55 defines the production `ModelPort` and its three dialect coordinates; [r4_5b.rs](../../crates/hh-embed/tests/r4_5b.rs) and [r4_5d.rs](../../crates/hh-embed/tests/r4_5d.rs) explicitly use loopback fake providers. [runtime.rs](../../crates/hh-embed/src/runtime.rs) lines 1–47 wires `KernelSink`, `KernelAssembler` and memory/retrieval ports. [policy_family.rs](../../crates/hh-context/src/policy_family.rs) `policy_for_variant` lines 65–84 selects default/full_window, priority_eviction, world_state_diff, jit_handles, recency_plus_pins and retrieval_augmented. [compact_family.rs](../../crates/hh-context/src/compact_family.rs) contains `SummarizeRolling`, `OffloadThenSummarize`, `StructuredCheckpoint`, `WorldStateRefresh`, `ProviderCompactionVariant`, `FreshWindowWithNotes` and model-owned compaction operations; these are implementation families, not ranked recommendations. [r2_5.rs](../../crates/hh-embed/tests/r2_5.rs) lines 1–23 and [r3_4.rs](../../crates/hh-embed/tests/r3_4.rs) lines 1–37 describe procedure supply/activation, consolidation, retrieval and resume fixtures. [context_adapters.rs](../../crates/hh-embed/src/context_adapters.rs) `LaneEmbedder::new` lines 235–249 rejects non-ChatCompletions families; `LaneProviderCompaction::new` lines 556–574 rejects non-Messages families. [r4_7b.rs](../../crates/hh-embed/tests/r4_7b.rs) lines 1–24 describes fake-provider and recorded-vector coverage. Contract: R-2.4.1–5, especially compaction's kernel invariants and non-authoritative derived summaries.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: Select compaction arms only after checking their actual runtime bindings, required ports and provider restrictions; keep fixture teaching separate from the real comparison.  
Outline delta: CONFIRMS OL-02, OL-09; EXTENDS OL-14 — memory, retrieval, procedures and context-policy coverage need explicit dispositions alongside compaction.  
Limitations: The provider restrictions are facts about this baseline's code, not a fresh audit of current external APIs. This unit did not inspect every cache/routing path or execute a context benchmark.

### F4 — Control variants are selectable; delegation still requires a concrete composition

Claim: First-party control and compute variants exist, but supported topology and bound execution ports limit what a team demonstration can promise.

Status: VERIFIED  
Evidence kind: source inspection  
Evidence: [open.rs](../../crates/hh-embed/src/open.rs) `strategy_for` lines 3440–3445 aliases round_robin and delegates to [react.rs](../../crates/hh-control/src/react.rs) `variant_for` lines 1327–1343 (minimal, steerable, queue, plan-execute, workflow, program). Unknown variants return `None`. [compute.rs](../../crates/hh-control/src/compute.rs) `policy_for_configured` lines 1420–1452 selects static/uniform/rules/bandit/surface_prior/predictor or `VariantNotAdmitted`; [open.rs](../../crates/hh-embed/src/open.rs) `arm_driver` region 2526–2664 checks the compute binding and procedure requirements. [topology.rs](../../crates/hh-orchestrator/src/topology.rs) `implemented` lines 88–101 admits T3 as well as T0/T1/T2/T6, correcting its older module header; [s5_5.rs](../../crates/hh-orchestrator/tests/s5_5.rs) lines 395–428 asserts depth-two admission and depth-zero refusal. [r3_3.rs](../../crates/hh-control/tests/r3_3.rs) lines 1–31 describes the explicit `UnbackedPort{delegate}` refusal and actual plan-walker tests. Contract: AC-R-2.6.1-1, AC-R-2.6.3-1, AC-R-2.6.4-1 were inspected directly; static compute must not introduce compute decisions.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: Solo/team, workflow and value-of-compute examples must identify their bound ports and admitted topology; unsupported team presets cannot be disguised by a fixture.  
Outline delta: CONFIRMS OL-02; EXTENDS OL-14.  
Limitations: No claim that the normal CLI start path automatically binds an orchestrator or that any policy improves outcomes. E1 must trace the chosen composition. T4/T5/T7 topology absence does not imply no ensemble-related library code exists.

### F5 — Operational mechanisms support bounded claims, not blanket safety guarantees

Claim: Budgets, permission handling, completion verification and recovery expose typed state transitions whose proof must remain tied to a specified fault or attack case.

Status: PARTIALLY VERIFIED  
Evidence kind: source inspection  
Evidence: [budget/lib.rs](../../crates/hh-budget/src/lib.rs) lines 1–61 specifies reserve-before-spend, attribution, matched budgets and E1–E5 decision-point exhaustion semantics. [service.rs](../../crates/hh-embed/src/service.rs) lines 712–820 shows read-only-session refusal and durability/environment dispatch. [r2_3.rs](../../crates/hh-embed/tests/r2_3.rs) lines 1–24 explicitly scopes suspend/compensate/heal fixtures and direct-store setup; [r2_4.rs](../../crates/hh-embed/tests/r2_4.rs) lines 1–23 preserves unsupported memory-snapshot behavior. [s3_10.rs](../../crates/hh-control/tests/s3_10.rs) lines 1–28 describes completion-gate and replay assertions, while [r4_7e.rs](../../crates/hh-embed/tests/r4_7e.rs) lines 1–20 explicitly uses a fake judge provider and recorded answers. [r4_6_live.rs](../../crates/hh-embed/tests/r4_6_live.rs) lines 1–33 scopes the AgentDojo-shaped credential test to carried/refused/leak outcomes, not a whole benchmark score. [r4_15e.rs](../../crates/hh-embed/tests/r4_15e.rs) lines 1–11 scopes signer custody, rotation and no-leak checks; [service.rs](../../crates/hh-embed/src/service.rs) lines 111–125 describes honest absent-signer n/a. Contract: directly inspected AC-R-2.8.1-1 and AC-R-2.8.7-1 require matched gate-off comparison and authorization before preparation, respectively.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: Choose concrete fault/attack matrices and verify observable outcomes; preserve infrastructure failure, oracle failure, unknown effects and unsupported capabilities. A budget demonstration must account for already committed effects.  
Outline delta: CONFIRMS OL-12, OL-15.  
Limitations: Breadth is based partly on inspected test scope headers and owning contracts; no complete security review or recovery campaign was performed. No claim that all independent graders are wired into the ordinary run path.

### F6 — Experiment orchestration and analysis are implemented; fixture closure is not task execution

Claim: The Lab supplies durable scheduling, matching and analysis primitives, but inspected exemplar completion synthesizes subject evidence rather than demonstrating a real task-and-grader runner.

Status: VERIFIED  
Evidence kind: source inspection  
Evidence: [exemplars.rs](../../crates/hh-lab/src/exemplars.rs) lines 1–40 explicitly calls exemplars document shapes; default five replicates and tolerance are packaged values, not pilot-derived protocol choices. [experiment_ops.rs](../../crates/hh-embed/src/experiment_ops.rs) lines 1–17 states records-in/out resolver requirements and honest `Unresolvable`. [engine.rs](../../crates/hh-experiment/src/engine.rs) `launch` lines 1041–1165 and 1386–1498 opens a bound subject run and returns `subject_writer`; `settle` lines 1508–1585 requires terminal subject facts and derives outcome/accounting. [cap_2_composed.rs](../../crates/hh-embed/tests/cap_2_composed.rs) `mint` around 1080, `exp_drain` lines 1191–1282 and `cap2_experiment_exemplar_close_completes` lines 1422–1476 supplies `task_success` and invokes `finish_subject` before settlement; its completed status is a plumbing result. [adapters.rs](../../crates/hh-bench/src/adapters.rs) lines 1–65 names A/C/D/E `FixtureAdapter`s. [grade.rs](../../crates/hh-bench/src/grade.rs) lines 156–234 consumes caller-provided `verifier_stdout` and `InfraSignal`; its `separate_verifier` boolean follows the declared isolation value, so this function alone does not launch or attest an isolated verifier process. [analysis/lib.rs](../../crates/hh-analysis/src/lib.rs) lines 1–43 maps results rows to one estimator implementation, with matching refusals, uncertainty and multiplicity behavior; [eval/lib.rs](../../crates/hh-eval/src/lib.rs) lines 1–20 keeps typed n/a distinct from zero. Contract: R-2.9.2, R-2.9.4, R-2.10.3–5.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: E1 must name who materializes tasks, executes the harness, captures submissions, runs the independent verifier and produces trustworthy result rows. Reuse the scheduler and estimators; do not declare the fixture exemplar an efficacy study.  
Outline delta: CONFIRMS OL-08; EXTENDS OL-05, OL-09, OL-20 — the executor/grader bridge is an explicit feasibility deliverable.  
Limitations: This does not prove there is no bridge anywhere else in the repository. It proves the inspected launch/grade APIs and exemplar test are insufficient evidence for that bridge. No dataset-wide absence claim is made.

### F7 — Several CLI Lab wrappers cannot supply their handlers' required inputs

Claim: At this baseline, `experiment claim/launch/settle` and `compare report` contain concrete request-shape mismatches with the production embed handlers.

Status: VERIFIED  
Evidence kind: source inspection  
Evidence: [lab.rs](../../crates/hh-cli/src/lab.rs) `cmd_experiment_op` lines 762–780 sends `experiment_id` plus optional `cell` and `run_id`; [experiment_ops.rs](../../crates/hh-embed/src/experiment_ops.rs) lines 715–800 requires `run_plan_id` for claim/launch/settle and `lease_id` for launch. The command does not populate those members. [lab.rs](../../crates/hh-cli/src/lab.rs) `cmd_compare_report` lines 915–940 sends only `baseline` and `candidate` to `lab.analysis.analyze`; [analysis_ops.rs](../../crates/hh-embed/src/analysis_ops.rs) lines 165–190 immediately requires `spec` and decodes tasks. The CLI helper at lab.rs lines 50–53 and [boundary.rs](../../crates/hh-cli/src/boundary.rs) lines 159–164 forward params verbatim. [cli.rs](../../crates/hh-cli/src/cli.rs) lines 1094–1124 actually routes these wrappers; these are not merely unused functions. The composed fixture's explicit `run_plan_id`/`lease_id` inputs (F6) illustrate the working lower-level contract.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: Repair and regression-test selected end-user Lab paths or teach an explicitly supported SDK runner. Do not publish those wrappers as a complete experiment tutorial before validation.  
Outline delta: CORRECTS OL-02 — command presence is not usability; EXTENDS OL-08, OL-17, OL-20.  
Limitations: This is a verified static mismatch, not an executed failure report. No existing source or test was changed. A3 should reproduce the precise error with a minimal valid setup; other wrappers require independent checking.

### F8 — Replay, attribution and bundles prove different things

Claim: Deterministic replay consumes recorded inputs, attribution accepts supplied outcomes, and the inspected clean-store bundle receipt is explicitly limited to R0.

Status: VERIFIED  
Evidence kind: source inspection; recorded execution (inspected only)  
Evidence: [replay_ops.rs](../../crates/hh-embed/src/replay_ops.rs) lines 1–23 and `replay` lines 38–85 separate view reconstruction from deterministic re-drive and inspect the bound variant's replay declaration. [l4_6b.rs](../../crates/hh-embed/tests/l4_6b.rs) `l46b_effect_dispatch_replay_byte_identical` lines 418–459 tracks model/effect rows around replay; its separate live-record test is ignored under ADR-0363 at lines 632–633. [attribution_ops.rs](../../crates/hh-embed/src/attribution_ops.rs) lines 1–13 explicitly folds caller-supplied `ArmOutcome`s, optionally opening real counterfactual arms; [s6_3b.rs](../../crates/hh-embed/tests/s6_3b.rs) lines 1–8 confirms that coverage split. [bundle_ops.rs](../../crates/hh-embed/src/bundle_ops.rs) lines 1–23 preserves matched budgets and unverified provenance for imported member bytes. The [bundle receipt](../build/reports/live/2026-10-23-capstone-bundle-repro.json), recorded `2026-10-23T03:13:09Z` at `9412b2ba82e70eec4ca8484656e3860aff83ae1a`, states clean-store `level=R0`, verdict reproduced. Contract: R-2.2.4 explicitly distinguishes reconstruction from decision reproducibility; R-2.9.3 names declared R0–R3 levels.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: Give verification, recomputation, replay and fresh execution separate commands, prerequisites, comparisons and claims. Do not call replay “the same task again for free.”  
Outline delta: CONFIRMS OL-07, OL-16; EXTENDS OL-14 for causal attribution.  
Limitations: The historical bundle and replay transcripts were not re-executed; this unit does not establish all higher-level bundle reproduction paths or intervention efficacy.

### F9 — Extension packaging is concrete; execution and absence need explicit evidence

Claim: A packaged variant can use the versioned host ABI, but its build/package prerequisites and hosted-participant boundary cannot be omitted from an extension tutorial.

Status: VERIFIED  
Evidence kind: source inspection  
Evidence: [package_variant.rs](../../crates/hh-varhost/tests/package_variant.rs) `materialize` lines 69–88 copies a built executable, seals code pins and loads the package; `packaged_variant_runs_end_to_end` lines 132–190 spawns through the helper, binds and invokes the compaction contract. Lines 134–142 return early when the variant or helper binary is absent, printing “skipped” without Rust `#[ignore]`: raw passing-test counts therefore cannot prove this subleg ran. [hosting_ops.rs](../../crates/hh-embed/src/hosting_ops.rs) lines 1–28 describes the removable `HostingPlane` and `hosting_plane_absent`; [embed Cargo.toml](../../crates/hh-embed/Cargo.toml) makes the hosting dependency test-only. [cap_2_composed.rs](../../crates/hh-embed/tests/cap_2_composed.rs) lines 1480–1494 explicitly composes a real `HostingService` with `FixtureParticipant`. Contract: R-2.12.2 requires compatible pinned manifests, constrained grants and ABI invocation; R-2.10.6 is a separate participant boundary.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: Demonstrate a real extension build, seal, load and invoke path and preserve skipped sublegs. “Without recompiling anything” is unsupported; avoiding a kernel rebuild is a narrower claim to validate.  
Outline delta: CONFIRMS OL-13, OL-18; CORRECTS OL-03's blanket extension promise.  
Limitations: No packaging or subprocess test was run. The test uses its declared backend and fixture ports; it does not establish all production isolation placements or universal third-party compatibility.

### F10 — Fleet, evolution and retirement are governed record services

Claim: Experimental organizational and evolution features are present as record-driven services, not evidence of autonomous successful work or model training.

Status: VERIFIED  
Evidence kind: source inspection  
Evidence: [fleet_ops.rs](../../crates/hh-embed/src/fleet_ops.rs) lines 1–12 and [evolution_ops.rs](../../crates/hh-embed/src/evolution_ops.rs) lines 1–14 define the supplied source/spec/evidence inputs and absent-C4 refusal. [cli.rs](../../crates/hh-cli/src/cli.rs) lines 1058–1067 selects fleet verbs and source documents, including the webhook lane. [fleet.rs](../../crates/hh-embed/tests/fleet.rs), [evolution.rs](../../crates/hh-embed/tests/evolution.rs), [s6_1b.rs](../../crates/hh-embed/tests/s6_1b.rs) and [s6_4.rs](../../crates/hh-embed/tests/s6_4.rs) opening scope sections describe experimental/capability/C4 gating and fixture record flows. [s5_4.rs](../../crates/hh-embed/tests/s5_4.rs) lines 1–17 describes drift claims, regression and debt folds. Contract: R-2.9.5's governed campaign, R-2.9.6 debt/removal tests, R-2.9.8's explicit no-weight-training boundary and R-2.12.6 human accountability.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: Disposition fleet, evolution, retirement after model changes and co-evolution separately; any later study must supply actual evidence reports and honor human gates.  
Outline delta: CONFIRMS OL-14, OL-21.  
Limitations: No campaign, tracker, webhook or model-regression study was executed. No claim about a future deployment's organizational utility.

### F11 — Client surfaces have sharply different execution scope

Claim: The kernel HTTP surface, web client, artifact MCP server, Lab MCP server and foreign corpus implementations should be taught as distinct integrations.

Status: VERIFIED  
Evidence kind: source inspection  
Evidence: [net.rs tests](../../crates/hh-embed/tests/net.rs) lines 1–11 scopes loopback, bearer, Host/Origin and NDJSON parity checks. [web/lib.rs](../../crates/hh-web/src/lib.rs) lines 1–22 makes the UI a projection client and identifies named, ledgered consequential operations rather than an independent store. [mcp/artifact.rs](../../crates/hh-mcp/src/artifact.rs) `tools_call` lines 398–446 documents catalog/refusal/input-request behavior; ordinary catalog execution is `stage_pending`, as qualified in [status.md](../status.md), “Replay and client compatibility.” [mcp-lab/lib.rs](../../crates/hh-mcp-lab/src/lib.rs) lines 1–40 instead describes Lab read/launch/experiment/result tools and ledgered surface turns. [Python guide](../../foreign/e2-python/README.md), current coverage and limits, describes eight corpus arms/57 checked answers and omitted unported HIR cases; the [TypeScript guide](../../foreign/e3-typescript/README.md), Run section, gives its corresponding comparator path. Both are corpus implementations, not complete alternate kernels. Contract: R-2.11.2–4 and R-2.5.4; schema source remains the boundary authority.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: Name which MCP surface a demonstration uses, pin client compatibility, and describe foreign replay only at corpus scope. A browser screenshot can support inspection, not prove independent execution.  
Outline delta: CONFIRMS OL-02, OL-13, OL-15.  
Limitations: ACP is inventoried from the interface/status guides and CLI dispatch, not deeply audited here; A2 must inspect its cited receipt. No protocol-era external compatibility was rechecked.

### F12 — Existing live receipts already demonstrate why task outcomes need separate reporting

Claim: The inspected live records include a transport/lifecycle pass with task format failure and a failed provider replay record, alongside a narrowly scoped OpenAI binding pass.

Status: VERIFIED  
Evidence kind: recorded execution (inspected only)  
Evidence: [quickstart-live](../build/reports/live/2026-10-23-quickstart-live.json) was recorded `2026-10-23T05:18:40Z`, revision `648f3e58600d4e117d1074bad0463b06782897e7`, local `qwen2.5:7b-instruct`, `result: pass`; its assertion explicitly includes `stop_reason.kind: format_failure`. [capstone-run-anthropic](../build/reports/live/2026-10-23-capstone-run-anthropic.json) was recorded `2026-10-23T03:21:40Z`, revision `9412b2ba82e70eec4ca8484656e3860aff83ae1a`, `result: fail:live-tests rc=101`. [capstone-run-openai](../build/reports/live/2026-10-23-capstone-run-openai.json), same revision, recorded `2026-10-23T03:22:13Z`, reports three calls over the binding checks, model `gpt-4.1-mini-2025-04-14`, verify/replay assertions and `result: pass`. These JSON records name their transcripts and hashes; those transcripts were not inspected here. [status.md](../status.md) preserves the later replay repair as a scripted-provider result, not a new Anthropic pass. Its **2026-10-28 R4.18 update** supersedes earlier availability prose: it reports all 49 publishable crates published and launch closeout complete, while still withholding a production-readiness rating.

Retrieved: 2026-10-29T18:08:34Z–18:11:42Z.  
Implication for the spec: Publish terminal task outcome, infrastructure assertions and evidence class separately; retain failed and superseded records. Do not reintroduce the stale pre-launch status or extrapolate release availability into efficacy.  
Outline delta: CONFIRMS OL-15; CORRECTS OL-02 against undated status summaries.  
Limitations: Reading a receipt does not independently verify its transcript, hashes, assertions or present provider behavior. A2 must perform that deeper audit and trace the live task/grade evidence.

## Answer and completion assessment

A1's inventory question is answered at source-inspection scope. The matrix gives C1 a bounded coverage checklist rather than an exhaustive symbol list. It identifies actual runtime/SDK/library surfaces, protocol and feature gates, fixture versus sampled recorded-live coverage, and concrete limits. The source-level CLI mismatches are settled enough to influence scope now; their exact observed errors remain unexecuted. The real experiment executor/grader bridge remains a named E1 uncertainty, not an invented implementation defect or an excuse to substitute fixture results for a study.

No technical blocker prevents accepting this inventory. Acceptance does not mean every family has been exercised or the eventual portfolio is selected. Preliminary design inference: a durable task/trace/replay journey and an honest offline Lab exercise are plausible first-wave candidates; a live compaction comparison needs the E1 bridge decision. Fleet/evolution/team topics remain explicit portfolio candidates, with no disposition silently made here.

## Open questions

| Question | Owner | Affected units | Operator decision now? |
|---|---|---|---|
| Which concrete executor connects a launched subject and its fenced writer to a real harness run, submission and independently executed grader? | E1 architecture worker | C2, D2, E2–E3, F1/F3 | No; identify source-backed options before Q-2/Q-3. |
| Can A3 reproduce the Lab CLI mismatches with a minimal valid setup, and which supported SDK path is the smallest usable interim tutorial? | A3, then E1 | C1/C2, F2, G1 | No; bounded inspection/execution scope to be assigned by coordinator. |
| Which family examples need model/provider-specific ports or feature opt-ins beyond the baseline? | C1/E1 | C2, D2/D3 | Portfolio/resource choices at Q-2/Q-3, not inferred here. |
| Which recorded receipts actually contain independently graded task outcomes and full subleg execution? | A2 evidence worker | C1, D1, F1 | No; preserve negative evidence and ordinary-test early-return skips. |

## Spec requirements emitted

These are **candidate supplemental requirements**, not changes to the frozen contract or existing ticket registers.

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

## Handoff

Dependencies resolved: A1 had none. A2 can begin the receipt audit using F8/F9/F12; A3 can investigate exact command/schema gaps using F1/F7/F11. C1 should carry the entire matrix into dispositioning. E1 should prioritize F6/F7's writer/executor/grader and CLI seams rather than re-surveying crates.

Proposed follow-up work is already covered by A2/A3/E1; no new row or historical deferral is created. No conflicts with accepted notes are known (none were inputs). The portfolio is not approved or changed here. Central-state edits and row acceptance remain the coordinator's responsibility.

Verification record: read-only inspection commands and the zero-open-deferrals query executed; **0 Rust tests executed, 0 new test passes claimed, 0 live legs executed**. The note was written via Python and reviewed with `git diff --no-index /dev/null docs/research/01_capability-inventory.md`; exit 1 is the expected added-file diff result. Relative-link and structural checks are reported in the worker handoff. No commit, source edit, ledger edit, external write or paid call occurred.
