# Ontology Register — canonical vocabulary (living glossary)

**Version:** **v0.1** (Phase 0 synthesis, 2026-09-09) — v0 Preflight seed (doc 2 §1 definitions, §4 seven planes, three cross-cutting properties, ADR-0001 participant classes) **plus** the §5 terms folded from WS-A1 and WS-L7 and ratified by ADR-0004/0005/0008, the canonical-name table (§6), and the pointer to the LCD-trap test battery (§7, `registers/lcd-test-battery.md`, ADR-0007). WS-L1's program vocabulary is appended as `proposed` (ratify with ADR-0009 at end of Phase 1).
**v0 → v0.1 change log:** short-forms *native participant* / *hosted participant* adopted (ADR-0001 long forms remain the definitions); *Hosting ABI* is the canonical name of the *thin observational ABI*; *Harness IR (HIR)*, *Harness Definition*, *Harness Lab*, *Model Profile / Profile Compiler*, compiler vocabulary (target/lowering/lifting/lowering loss report), *component class / component variant*, *configuration / arm*, *class-scoped metric*, *capability declaration / capability vector*, *hosting mechanism*, *observability level*, *comparison granularity*, *observed conformance / drift*, *trajectory interchange*, *reference runtime*, *research instrument*, *LCD trap*, *escape hatch*, *surface vs semantic identity*, *conditioned rule / assumption-debt record* ratified. Two items flagged for WS-A2 (CF-020, CF-021); *opacity ratio* stays `proposed` pending OQ-036.
**Role:** the tie-breaker for vocabulary in cross-track synthesis (doc 3 §7.1). WS-A2 owns v1; WS-A3 arbitrates typed IR vocabulary. Every term added by a workstream is appended under §5 with its owner and status (`v0-seed` / `proposed` / `ratified`).

---

## 1. Core definitions (v0)

| Term | Definition | Source | Status |
|---|---|---|---|
| **Harness** | The model-external machinery that creates and governs an agent's closed loop: it mediates perception, context, action, state, control, verification, security, and coordination for one or more foundation models operating toward goals in an external environment. | doc 2 §1 | v0-seed |
| **Harness engineering** | The empirical design, implementation, measurement, and evolution of that runtime system. | doc 2 §1 | v0-seed |
| **Agent** | A closed-loop system: model observes state → decides → acts through tools/environment → receives consequences → updates working state → eventually terminates. *Agent = Model + Harness (+ Environment).* | doc 2 §1 | v0-seed |
| **Model (M)** | A frozen foundation model; a stochastic policy generator π_M. | doc 2 §1 formal view | v0-seed |
| **Environment (E)** | The external world the agent acts in (filesystem, shell, browser, APIs, tracker, humans). | doc 2 §1 | v0-seed |
| **Task distribution (D)** | The distribution of goals over which a harness is evaluated. | doc 2 §1 | v0-seed |
| **Harness parameterization (θ)** | Prompts, context-selection policies, tool schemas, middleware, state machines, memory/retrieval policies, subagent topology, validators, permission rules, retry policies, stopping conditions. | doc 2 §1 | v0-seed |
| **Budget (B)** | Resource bound on a run: tokens, calls, wall-clock, VM minutes, network, fan-out, human approvals. | doc 2 §1, §4 | v0-seed |
| **Harness objective** | max_θ E_{τ∼D}[U(success, quality) − λ_c C(τ) − λ_l L(τ) − λ_r R(τ)] s.t. budget, safety, permission constraints. | doc 2 §1 | v0-seed |
| **Policy stack** | π_system ≈ H_{θ,E,B}[π_M]: the deployed policy is the model policy transformed by the harness. Harness changes are policy interventions, not cosmetic prompt edits. | doc 2 §1 | v0-seed |
| **Model–harness–environment configuration** | The true unit of capability measurement; a "model score" is a score for one such configuration. | doc 2 §1, §6 | v0-seed |
| **Harness validity** | The harness artifact (rule, memory, tool shape, procedure) encodes a semantically correct policy/fact. Deterministic at the protocol layer. | doc 2 §1, §5.1 | v0-seed |
| **Harness compliance** | The beneficiary model actually notices, activates, and follows a valid artifact. Model-conditioned; measured separately from validity. | doc 2 §1, §5.1 | v0-seed |
| **Compatibility surface** | A conditional, resource-bounded response surface over model family/version × task distribution × environment × budget (× tool ABI × context budget) describing realized harness benefit. The optimization target replaces "the best harness". | doc 2 §1, §10 | v0-seed |
| **Assumption debt** | Every model-specific rule is a hypothesis about a failure mode that should carry evidence, ownership, an expiry condition, and a regression/removal test. | doc 2 §5.9 | v0-seed |
| **Moving boundary of control** | The allocation of cognition among weights, context, code, tools, memories, subagents, and humans — and its migration as models change. The field's most distinctive research object. | doc 2 §9 | v0-seed |
| **Mechanism evidence** vs **performance evidence** | Evidence that a failure mode exists / an architecture is implementable, vs a benchmark lift. Mechanisms may be adopted on fresh evidence; performance claims from unreplicated preprints may not drive design commitments. | doc 2 §6; doc 3 §3.1 | v0-seed |

## 2. The seven planes (v0)

| # | Plane | Core question | Typical mechanisms | Characteristic failure |
|---|---|---|---|---|
| 1 | **Observation & context** | What does the model see now? | System instructions, history, retrieval, compaction, tool descriptions, artifacts, multimodal observations | Context rot, omitted evidence, stale/poisoned memory |
| 2 | **Action & tools** | What can it do, and how? | Shell/code, filesystem, browser, MCP, APIs, computer use, dynamic tool discovery | Tool misuse, schema friction, excessive tokens, non-composable interfaces |
| 3 | **Control & orchestration** | Who decides the next step? | ReAct loop, FSM/workflow, routing, planning, retries, stopping, subagent scheduler | Loops, premature stopping, control-flow hallucination, coordination overhead |
| 4 | **Verification & feedback** | How does the system know it is right? | Tests, linters, validators, end-state checks, critics, task contracts, proof-of-work | Plausible but ungrounded completion; reward hacking; weak judges |
| 5 | **State & durability** | What survives a turn/process/window? | Filesystem, git, event log, checkpoints, session store, progress files, memory layers | Lost progress, duplicate side effects, unrecoverable crashes |
| 6 | **Security & governance** | What is the maximum allowed blast radius? | Sandbox/VM, permissions, capabilities, egress controls, credential isolation, audit | Prompt injection, exfiltration, over-broad privileges, approval fatigue |
| 7 | **Measurement & evolution** | How does the harness improve? | Traces, evals, A/B tests, attribution, candidate edits, canaries, rollback | Overfitting, regressions, model-specific brittleness, untraceable changes |

**Design principle (v0):** hard invariants (permissions, budgets, idempotency, transaction boundaries, schema validation, objective stopping) live in deterministic software; open-ended search, decomposition, interpretation, and recovery tactics live in the model; move the boundary only when evaluation shows a benefit.

## 3. Three cross-cutting properties (v0)

| Property | Definition | Owner WS |
|---|---|---|
| **Resource economics** | Tokens, model calls, wall-clock, VM minutes, network, subagent fan-out, human approvals are resources consumed by every plane; the harness is a scheduler; "better" is judged on a capability-cost Pareto frontier. | WS-L2 |
| **Provenance** | Every observation, memory, tool result, artifact, instruction, and delegated capability carries origin/version/authority information sufficient to decide trust. Flattening heterogeneous sources into one token stream is the shared root cause of context-privilege escalation and revoked-memory failures. | WS-L3 |
| **Model conditioning** | The same policy/tool/memory has different effects across models. Portability = stable semantic interface + model-conditioned compilation, never behavioral interchangeability. | WS-A4, WS-C3, WS-E2 |

## 4. Participant classes (ratified, ADR-0001)

| Term | Definition |
|---|---|
| **IR-native participant (white-box)** — short-form (v0.1): **native participant** | A harness expressed in the MetaHarness IR and run on the native runtime; component-decomposable, swappable, ablatable, causally attributable, evolvable. The primary path. |
| **Hosted-external participant (black-box)** — short-form (v0.1): **hosted participant** | A third-party harness (e.g. Codex, Claude Code, OpenHands, Cursor CLI) joined through the **thin observational ABI** (start/resume/cancel, stream events, supply context/tools/skills, request permission, account cost). Shares environments, evals, scorecards, cost/latency/audit; never pretends to be component-decomposable. First-class but secondary. |
| **Comparison plane** | The single experiment/eval/scorecard/analysis surface spanning both participant classes. |
| **Thin observational ABI** | The deliberately minimal boundary through which hosted-external participants join (depth decided by WS-J6 in Phase 3). **Canonical name (v0.1, ADR-0008): Hosting ABI.** Reframed by ADR-0005 as a *minimum observable event set* satisfiable by each hosting mechanism plus a per-participant *capability declaration*; bounded by T-LCD-06/-07 (ADR-0007). |

## 4a. Adjacent-discipline boundary (v0, from doc 2 §1)

Prompt engineering (subset: instruction wording) · Context engineering (major subset: full token state) · Agent engineering (overlapping umbrella) · Agent framework (a means; not the configured harness) · Orchestration (one subsystem) · AgentOps/LLMOps (overlaps at telemetry/evals/rollouts) · Compound AI systems (broader) · Evaluation harness (a different sense of "harness"; can be used to measure runtime harnesses).

## 4b. Candidate IR entity/edge vocabulary (v0, from doc 2 §11 — **not yet typed; WS-A3 owns**)

Entities: `Goal, Observation, ContextItem, Memory, Procedure, ToolCapability, Permission, Effect, Artifact, Validator, AgentProcess, Budget, HarnessRule`.
Edges: `depends-on, supersedes, authorizes, produced-by, validates, delegated-to`.
Every entity carries version + provenance. Behavioral, not framework-specific (a `ToolCapability` is semantics/effects with compilers to MCP / native function calling / shell wrapper / provider tool; a `Procedure` compiles to instructions, a workflow node, or a subagent task).

## 4c. Core tiers & build stages (from doc 3 §1.3, §2; doc 2 §11)

`C0` MetaHarness Core (minimal inspectable runtime + IR + measurement backbone) · `C1–C4` extension tiers along a declared DAG · Build stages `Stage 0` minimal baseline → `1` deterministic runtime semantics → `2` context & procedural memory → `3` evaluation first → `4` subagents for measurable reasons → `5` model-conditioned profiles → `6` adaptive evolution (doc 2 §11), to be re-derived by Phase 5 as the staged build ladder.

---

## 5. Workstream additions (append-only; owner + status)

Status values: `proposed` · `ratified (v0.1)` (Phase 0 synthesis, ADR cited) · later `ratified (v1)` by WS-A2.

| Term | Definition | Owner WS | Status | Notes |
|---|---|---|---|---|
| **hosting mechanism** | The technical means by which a hosted participant is attached to the comparison plane: **session-ABI** (start/resume/prompt/cancel + typed event stream + permission requests; ACP S-035, Omnigent `Executor` S-118, Codex app-server S-107), **model-boundary interception** (a proxy or client patch that captures every model call; Inspect bridge S-136/S-137, ClawGym II S-140), or **container-installed** (vendor CLI installed in the task environment; trajectory reconstructed from native logs; Harbor S-132). Each mechanism yields a different observability level; no mechanism is privileged by the analysis engine. | WS-J6 (proposed by WS-A1) | ratified (v0.1, ADR-0005) | |
| **observability level** | The set of evidence a participant run can supply to the ledger: `events` (typed lifecycle/tool/permission events), `model_io` (complete model requests/responses), `end_state` (environment state + verifier), `ledger` (full IR-native typed ledger). Native participants supply all; hosted participants supply the subset their mechanism permits, declared per version. Every metric declares the levels it requires; missing ⇒ `n/a`, never 0. Stamped on every ledger event (WS-B1). | WS-J6 / WS-B1 (proposed by WS-A1) | ratified (v0.1, ADR-0004/0005) | grey-box interposition option: OQ-038 |
| **comparison granularity** | The unit at which an experimental factor varies: `component` (an IR component variant/parameter — native participants only), `configuration` (a non-component coordinate of a configuration that the Hosting ABI lets us vary for any participant: model, context/tool/skill supply, permission policy, budget), `product` (a participant version as a whole). Refines ADR-0001's "configuration-level, not component-level" note. | WS-A2 (proposed by WS-A1) | ratified (v0.1, ADR-0004) | enum value name vs *configuration* (factorial point) flagged CF-021 |
| **capability declaration** | A typed, versioned record of what a participant *claims* to support (hosting mechanism, streaming, interrupt, steer, live queue, resume mode, fork, compaction, images, subagents, permission surface, instruction delivery, model family, effort vocabulary, trajectory export, native config). Immutable for the life of a run; stored in the run ledger. Precedents: Omnigent `HarnessCapabilities`, Harbor `AgentCapabilities`. | WS-J6 (proposed by WS-A1) | ratified (v0.1, ADR-0005) | vs *capability vector*: CF-020 |
| **observed conformance / drift** | The result of probing a participant version against its capability declaration: `SUPPORTED / UNSUPPORTED / PARTIAL / NOT_APPLICABLE / UNKNOWN / SKIPPED / DRIFT` (declared ≠ observed). DRIFT is a fact about a participant version, stored as experimental data (`ConformanceRecord`) and visible in comparisons; whether it excludes or annotates is OQ-034. Precedent: Omnigent `tests/harness_bench/verdict.py`. | WS-J6 (proposed by WS-A1) | ratified (v0.1, ADR-0004) | |
| **trajectory interchange** | A lossy, participant-class-neutral serialization of a run (steps with source/model/message/tool calls/observation/metrics; subagent references; final metrics) used to import/export runs across evaluation planes. Distinct from the native run ledger; lossless only for hosted rows; imported rows land at `product` granularity with `observability_level` set from content. Precedent: Harbor ATIF (S-132). Format choice: OQ-030. | WS-J6 / WS-I3 (proposed by WS-A1) | ratified (v0.1, ADR-0005) | |
| **reference runtime** | The opinionated, minimal, event-sourced runtime that executes native harnesses so that white-box participants exist; its purpose is instrument validity (ablation/attribution), not adoption as a general framework; it does not host other products' core loops. | WS-A1 / WS-A5 | ratified (v0.1, ADR-0003) | positioning term |
| **research instrument** | The assembly + experiment + comparison + analysis apparatus that treats model × harness (component/configuration/product) × environment × budget × seed as explicit factors and spans both participant classes. "MetaHarness is a research instrument and reference runtime" is the Spec §1 thesis. | WS-A1 / WS-L7 | ratified (v0.1, ADR-0003/0006) | positioning term |
| **LCD trap** | The failure mode of a portability abstraction that admits only the intersection of its targets' behaviours, so target-specific behaviour that drives quality becomes reachable only through escape hatches, after which the abstraction is routed around and decays. Signatures: feature intersection; untyped escape hatches; silent loss on lowering. Antidote: capability declaration + verified probing + typed extension slots + profile compilation. Sources S-152 (LSP), S-154 (AWT/WORA), S-035 (ACP), S-118. | WS-L7 | ratified (v0.1, ADR-0007) | tested by `registers/lcd-test-battery.md` |
| **escape hatch** | Any IR/ABI field or path that lets a definition bypass the typed, profile-owned route to a model- or target-specific surface (e.g. a raw schema override). Presence in the IR is a T-LCD-01 failure. | WS-L7 / WS-A3 | ratified (v0.1, ADR-0007) | |
| **opacity ratio** | Fraction of a compiled model-facing surface (by tokens, pending OQ-036) that originates from untyped text leaves rather than typed IR entities. Reported per harness definition and stored in the bundle (T-LCD-02). | WS-I2 (metric); WS-A3 (definition) | proposed | definition open: OQ-036 |
| **surface vs semantic identity** | A capability/procedure/rule's *identity* is content-addressed over its semantics (effects, preconditions, permissions, validators); its *surface* (model-facing name, description, schema layout, error format) is a versioned, profile-owned rendering. Renames are profile edits, not IR edits (T-LCD-10). | WS-A3 / WS-L4 | ratified (v0.1, ADR-0007) | WS-A3 designs identity derivation accordingly |
| **Harness IR (HIR)** | Canonical name of the typed, behavioural, provenance-bearing representation of a harness across all seven planes (§4b entities/edges); versioned dialects HIR/1, HIR/2 … Written "Harness IR" on first use. "HTIR" (HarnessFix trace IR, S-146) must not be used. | WS-A3 (arbitrates typed vocabulary) | ratified (v0.1, ADR-0008) | |
| **Harness Definition** | A declarative artefact in HIR that names component variants, parameters, profiles and policies for one assemblable harness ("harness def"); the unit WS-J1 assembles/validates and WS-J2 registers. Whether it is a pure data document is OQ-044 (WS-A5). | WS-J1 / WS-A5 | ratified (v0.1, ADR-0008) | |
| **native participant** / **hosted participant** | Canonical short-forms of the ratified ADR-0001 classes *IR-native participant (white-box)* / *hosted-external participant (black-box)*. Definitions unchanged; first use in any spec section expands to the long form (guards against "runs locally / in the cloud" misreadings). | WS-A2 | ratified (v0.1, ADR-0008) | does not amend ADR-0001 |
| **Harness Lab** ("the Lab") | Canonical name of the meta-harness laboratory (doc 3 §2.10): assembly, component-variant registry, experiment engine, comparison engine, results ledger/leaderboard, hosting. Top-level CLI/web noun. Not "Bench". | WS-J1–J6 / WS-K1 | ratified (v0.1, ADR-0008) | |
| **Hosting ABI** | Canonical name of the ratified *thin observational ABI* (§4) through which hosted participants join the comparison plane. Named for its purpose; deliberately not "Harness ABI". Depth: OQ-004 (WS-J6). | WS-J6 | ratified (v0.1, ADR-0008) | |
| **Model Profile** / **Profile Compiler** | A versioned, expirable object holding every model-conditioned surface decision (prompt layout, tool shapes/names, error formats, compaction/reminder policy, interaction modes) for one model family/version; and the compiler that applies it during lowering. The profile schema is the *sole owner* of surface fields (T-LCD-01/-10); the profile compiler itself carries an assumption-debt record (T-LCD-05). "Model adapter" remains the gateway (WS-C1). | WS-C3 | ratified (v0.1, ADR-0008) | doc 2 §11 term canonicalised |
| **target** · **lowering** · **lifting** · **lowering loss report** | *Target*: a protocol or runtime a Harness Definition is compiled to (native runtime, MCP, A2A, ACP, provider tool API). *Lowering*: compiling HIR to a target under a profile. *Lifting*: recovering HIR-level metadata from a target artefact. *Lowering loss report*: the compiler's declaration of metadata the target cannot carry (T-LCD-11); `UnexpressibleSurface` is a first-class error. | WS-A4 | ratified (v0.1, ADR-0008) | |
| **component class** / **component variant** | A component class is a Core-defined contract (operations/inputs/outputs/invariants/failure modes — never inheritance, T-LCD-12) for one pluggable slot; a component variant is one implementation registered under that class. Every class contract receives the Model Profile and resource accounting (T-LCD-08). "Plugin" is reserved for third-party packaging (WS-L5). | WS-A5 / WS-J2 / WS-L5 | ratified (v0.1, ADR-0008) | |
| **conditioned rule** / **assumption-debt record** | A conditioned rule is any profile-owned rule that exists because of a hypothesised model deficiency; its assumption-debt record `{rule_id, hypothesis, evidence_refs[], owner, expiry_condition, removal_test_ref, status}` is mandatory on every conditioned rule (T-LCD-05). The record *schema* is C0/Stage 1; the *manager* (WS-I6) is C4. Names the unit of the v0 term "assumption debt". | WS-C3 (rule) / WS-I6 (record schema + manager) | ratified (v0.1, ADR-0007/0008) | |
| **configuration** / **arm** | *Configuration*: one point in the factorial model × harness definition × profile × environment × budget × seed — extends the v0 "model–harness–environment configuration"; the ledger keys results by configuration id. *Arm*: a set of configurations run under one experimental hypothesis with a matched budget (T-LCD-14). | WS-J3 / WS-I2 | ratified (v0.1, ADR-0008) | see CF-021 |
| **class-scoped metric** | A scorecard metric whose `MetricDeclaration{name, requires_observability ⊆ {events, model_io, end_state, ledger}, applies_to_classes}` declares the participant classes and observability levels it applies to; not applicable ⇒ `n/a`, never 0 (T-LCD-15). | WS-J4 / WS-I2 | ratified (v0.1, ADR-0004/0007) | |
| **capability vector** | The derived `{declared: map, probed: map, unknown: set}` triple for a hosted participant, computed from its capability declaration and conformance probes; `unknown` is never coerced and comparisons consuming it must stratify (T-LCD-07). | WS-J6 / WS-J4 | ratified (v0.1, ADR-0007/0008) | vs *capability declaration*: CF-020 |
| **ecosystem class** | A family of language + runtime + package ecosystems grouped by the properties that matter to the WS-L1 criteria (compilation model, typing discipline, native web affinity, research-tool concentration), not by language name (E1–E5 in WS-L1 §6.2). | WS-L1 | proposed (ratify with ADR-0009, end Phase 1) | program vocabulary; names no language |
| **ecosystem boundary** | A designed process, network, or FFI seam across which two components implemented in different ecosystem classes exchange IR-typed messages under the boundary contract (WS-L1 §6.5). Same verb set as the session-ABI hosting mechanism (CF-023). | WS-L1 / WS-K4 / WS-L5 | proposed (ratify with ADR-0009) | |
| **polyglot split** | A candidate architecture that assigns different ecosystem classes to the kernel/runtime, the laboratory/analysis layer, the surfaces, and/or the sandbox helper, with a named boundary mechanism per boundary (candidate E5; sub-variants E5a–d). | WS-L1 | proposed (ratify with ADR-0009) | |
| **boundary mechanism** | One of the recurring implementation patterns for an ecosystem boundary: subprocess + JSON-RPC/JSONL over stdio; localhost HTTP/WebSocket with generated clients; schema-first codegen from a single source of truth; in-process FFI bindings; embedded engine for model-authored code; protobuf/gRPC (M3(a)–(f), each with source-code precedent). | WS-L1 | proposed (ratify with ADR-0009) | |
| **boundary cost** | The recurring cost of an ecosystem boundary: serialization and type duplication (mitigated by codegen), two toolchains, cross-boundary version pinning, cross-boundary debugging, contributor barrier; scored as criterion C12 (penalty on polyglot candidates only). | WS-L1 | proposed (ratify with ADR-0009) | |
| **hard-gate criterion** | A WS-L1 criterion whose failure excludes a candidate before weighted scoring; C2 unconditional; C4, C6 conditional on Phase 1 answers (OQ-040, OQ-045). | WS-L1 | proposed (ratify with ADR-0009) | |
| **ratification questionnaire** | The fixed list of language-neutral questions Q-L1-01…14 (registered as OQ-040…046) that upstream workstreams answer so the language decision can be scored mechanically. | WS-L1 | proposed (ratify with ADR-0009) | |
| **language-leak audit** | The grep-based check (RK-09) run over all dossiers/ADRs at every synthesis pass, confirming no upstream artifact commits to a language, runtime, package ecosystem or framework; acceptance criterion AC8 of ADR-0009. Phase 0 result: clean (CF-025). | Phase synthesis / WS-L1 | proposed (ratify with ADR-0009) | |

---

## 6. Canonical-name table (v0.1, ADR-0008)

| concept | canonical name | short-form / rule |
|---|---|---|
| Product | **MetaHarness** (retained) | Public qualifier recommended: "MetaHarness — harness laboratory & reference runtime"; collision disclosed (CF-013, OQ-035); no sub-concept uses "meta-". |
| The representation | **Harness IR** | **HIR**; dialects HIR/1, HIR/2. Never "HTIR". |
| A harness expressed in HIR | **Harness Definition** | "harness def". |
| Participant classes | **native participant** · **hosted participant** | Short-forms of ADR-0001's long forms; first use expands. |
| Comparison surface | **comparison plane** | Ratified v0; unchanged. |
| The laboratory | **Harness Lab** | "the Lab". Not "Bench". |
| Boundary for hosted participants | **Hosting ABI** | Definition = *thin observational ABI* (ADR-0001). Never "Harness ABI". |
| Model-conditioning object / applier | **Model Profile** · **Profile Compiler** | "profile"; "model adapter" = gateway (WS-C1). |
| Compilation vocabulary | **target** · **lowering** · **lifting** · **lowering loss report** | |
| Factorial point / hypothesis set | **configuration** · **arm** | |
| Pluggable slots | **component class** · **component variant** | "plugin" = third-party packaging (WS-L5). |
| Model-specific rule and its record | **conditioned rule** · **assumption-debt record** | |
| Metrics / hosted features | **class-scoped metric** · **capability declaration** · **capability vector** | |
| Failure mode | **LCD trap** (+ **escape hatch**, **opacity ratio**, **surface vs semantic identity**) | |

Rules (ADR-0008): (1) first use of each name in a spec section expands it; (2) "framework" is reserved for what others build on top of MetaHarness and never describes MetaHarness itself; (3) any proper-noun brand is an *alias* chosen by WS-L6/sponsor and never replaces the canonical name in the spec; (4) the readiness report's glossary check fails on any unregistered synonym.

## 7. LCD-trap test battery (pointer)

The fifteen binding acceptance criteria T-LCD-01…15 (ADR-0007) live in **`registers/lcd-test-battery.md`** — the named, citable artifact that WS-A3/A4/A5 (Phase 1), WS-C3/E2/D1/D2/F1/B1/I1/I2 (Phase 2) and WS-J3/J4/J5/J6/L5 (Phase 3) inherit. Every downstream ADR that introduces an abstraction spanning models, targets or participants must state which T-LCD tests it satisfies.
