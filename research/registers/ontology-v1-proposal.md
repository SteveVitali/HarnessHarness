# Ontology Register — v1 proposal (replacement candidate for `ontology.md` §1–§4)

**Status:** **PROMOTED** into `ontology.md` v1 at Phase 1 synthesis (2026-09-10; ADR-0012/0013/0014 ratified). This file is kept as the WS-A2 proposal of record; `ontology.md` is authoritative. Sections §5–§7 of `ontology.md` are **not** replaced; the amendments they need are listed in §4h below.
**Version line to become:** `v1 (Phase 1 synthesis) — v0 seed + v0.1 folds + WS-A2 formal model, planes-as-partition, boundaries, control boundary, validity/compliance pair, compatibility surface, participant formalization, first-class/derived split.`
**Rules kept from v0.1:** every v0/v0.1 term keeps its definition unless a row below says `amended` with a rationale; canonical names per ADR-0008 and ADR-0011 (product = **HarnessHarness**); typed representation of any term marked `typed-by: WS-A3` is WS-A3's; nothing here names a language, runtime, package ecosystem or framework.

---

## 1. Core definitions (v1)

| Term | Definition | Source | Status |
|---|---|---|---|
| **Harness** | The model-external machinery that creates and governs an agent's closed loop: it mediates perception, context, action, state, control, verification, security, and coordination for one or more foundation models operating toward goals in an external environment. | doc 2 §1 | v0-seed (kept) |
| **Harness engineering** | The empirical design, implementation, measurement, and evolution of that runtime system. | doc 2 §1 | v0-seed (kept) |
| **Agent** | A closed-loop system: model observes state → decides → acts through tools/environment → receives consequences → updates working state → eventually terminates. *Agent = Model + Harness (+ Environment).* | doc 2 §1 | v0-seed (kept) |
| **Model (M)** | A **model snapshot**: provider, version identity, and sampling parameters; induces the stochastic policy π_M(a \| c) over rendered contexts c. *Amended:* "frozen" replaced by "snapshot" because provider-side updates can change behaviour without a version bump (S-047); the configuration pins the snapshot identity it *believes* it has, and drift is a fact to detect, not an assumption to make. | doc 2 §1; WS-A2 F-disconfirming | v0-seed → **amended (v1)** |
| **Model set** | The set of model snapshots a harness binds in one run (primary, router alternates, subagents, judges). A configuration names the whole set; "the" model is the primary. | WS-A2 §5 | proposed (v1) — OQ-048 |
| **Environment (E)** | The external world the agent acts in (filesystem, shell, browser, APIs, tracker, humans), with a state s and an effect semantics; reached only through the **environment boundary** (§2b). | doc 2 §1 | v0-seed (amended: boundary pointer) |
| **Task distribution (D)** | The distribution of goals over which a harness is evaluated. | doc 2 §1 | v0-seed (kept) |
| **Harness parameterization (θ)** | θ = (h, p, β, π_pol): a Harness Definition h in HIR, a Model Profile p, a **control boundary** β (§2c), and the policies h references (permission, budget, retry, stopping). *Amended:* the v0 list (prompts, context-selection policies, tool schemas, middleware, state machines, memory/retrieval policies, subagent topology, validators, permission rules, retry policies, stopping conditions) is the *content* of h and p; v1 names the structure. | doc 2 §1; WS-A2 §6.2 | v0-seed → **amended (v1)** |
| **Budget (B)** | A vector over the resource dimensions (tokens, model calls, wall-clock, environment-minutes, network, subagent fan-out, human approvals, spend — exact dimensions `typed-by: WS-L2`) bounding a run; every Goal and AgentProcess references one (`typed-by: WS-A3`). | doc 2 §1, §4 | v0-seed (kept; dims deferred) |
| **Harness objective** | J(θ; M, E, D, B) = E_{τ∼π_system}[U(success, quality) − λ_c C(τ) − λ_l L(τ) − λ_r R(τ)] s.t. c(τ) ≤ B, safety and permission constraints. | doc 2 §1 | v0-seed (kept; notation fixed) |
| **Policy stack** | π_system = H_{θ,E,B}[π_M]: the closed-loop policy over environment states induced by iterating the **harness step** (§4d). Harness edits are policy interventions. *Partially programmable* because β assigns each decision point to code, model or human. | doc 2 §1; WS-A2 F3 | v0-seed (kept; made precise) |
| **Harness step** | One iteration of the loop: `render` (P1) → `propose` (model boundary) → `interpret` (P2) → `authorize` (P6) → `execute` (P2 → environment boundary) → `verify` (P4) → `persist` (P5) → `decide` (P3), with `observe` (P7) recording every operation. Each is a **plane operator** owned by its home plane. | WS-A2 §6.2 | proposed (v1) |
| **Harness effect** | The *paired* quantity Δ(θ, θ₀ \| M, E, D, B) = J(θ) − J(θ₀) at matched B (T-LCD-14). Never an absolute score. | doc 2 §1, §8; WS-A2 | proposed (v1) |
| **Model–harness–environment configuration** → **configuration** | One point κ = (M-set, h, p, E, B, seed); the ledger keys results by configuration id. (v0.1 definition kept; `M` generalized to the model set.) | doc 2 §1, §6; ADR-0008 | ratified v0.1 (kept) |
| **Harness validity** | For a harness artifact α, `validity(α, at) ∈ {valid, invalid, unknown}` decided by a protocol-layer check that never consults the beneficiary model (version stamp, validator, oracle, declared validity interval); `unknown` when no check exists. *Amended:* three-valued; "deterministic at the protocol layer" holds exactly where a check exists (S-077 mechanism). | doc 2 §1, §5.1; S-077; WS-A2 F4 | v0-seed → **amended (v1)** |
| **Harness compliance** | The model-conditioned chain of events after delivery of a valid artifact: `activated(α)` (the model loaded/used/retrieved α) and `followed(α)` (behaviour consistent with α). Measured, never asserted; each event carries `detector ∈ {deterministic, judged}` and provenance. See §4e. | doc 2 §1, §5.1; S-088; WS-A2 F4 | v0-seed → **amended (v1)** |
| **Realized-benefit decomposition** | E[Δ] ≈ P(valid) · P(activated \| delivered, valid) · P(followed \| activated) · E[Δ \| followed] — the reporting shape for any artifact-level effect (S-077 algebra, S-088 stages). | WS-A2 §6.2 | proposed (v1) |
| **Compatibility surface** | Ψ_θ : F → Dist(Δ), a mapping from the **factor space** F (model snapshot, task distribution, environment image, budget vector, context window, tool-surface target, control boundary, …) to the *distribution of the paired harness effect*. A **derived view** estimated by designed experiment and persisted only as a **fitted-surface report**; unprobed factor levels are `unknown`, never interpolated across categorical axes. *Amended:* v0's "response surface … describing realized benefit" made formal; "optimization target" retained. | doc 2 §1, §10; S-167; WS-A2 F5 | v0-seed → **amended (v1)** |
| **Portability / conditionality (of θ)** | Portability = sign-stability of Ψ_θ along the model axis; conditionality = the region of F where Δ > 0 with stated confidence. | WS-A2 §6.2 | proposed (v1) |
| **Assumption debt** | Every model-specific rule is a hypothesis about a failure mode that carries evidence, ownership, an expiry condition, and a removal test (unit: *conditioned rule* / *assumption-debt record*, v0.1). | doc 2 §5.9 | v0-seed (kept) |
| **Moving boundary of control** | The migration, as models change, of the **control boundary** β (§2c) — the allocation of each decision point to code, model or human. *Amended:* v0's informal "allocation of cognition" is given the object β so it can be varied (WS-F1/F2) and attributed (WS-I7). | doc 2 §9; WS-A2 F3 | v0-seed → **amended (v1)** |
| **Mechanism evidence** vs **performance evidence** | Unchanged. | doc 2 §6; doc 3 §3.1 | v0-seed (kept) |
| **Harness artifact** | Any identified, versioned, provenance-bearing thing a harness delivers to a model: `kind ∈ {instruction, rule, memory, tool_surface, procedure, observation}`; carries `validity_check_ref?` and `activation_observable: bool`. Identity is content-addressed (WS-L4). `typed-by: WS-A3` (as `ContextItem` payloads with `delivery_id`). | WS-A2 §6.4 | proposed (v1) |
| **Detector** | The component that emits an `activated` or `followed` event: `deterministic` (a P1/P2 signal: skill load, retrieval injection, tool call naming the artifact, an executable validator referenced by the artifact) or `judged` (a P4 judge configuration with its own provenance and cost attribution — OQ-059). | WS-A2 F4 | proposed (v1) |

### 1a. Two levels (v1, structural)

Every term in this register belongs to exactly one level.

| Level | What lives there | Realized by |
|---|---|---|
| **Object level** | One harness: its seven planes, two boundaries, entities, artifacts, component classes/variants, policies, θ, the harness step, π_system. | The reference runtime; HIR (WS-A3); the compiler (WS-A4). |
| **Instrument level** | The Harness Lab and everything that *compares* harnesses: participants, descriptors, configurations, arms, runs and the **run envelope** (run/turn lifecycle), metrics and their declarations, conformance records, fitted-surface reports, the Hosting ABI. | The Lab (WS-J1–J6), the results ledger (WS-J5), telemetry (WS-I1/I2). |

Rule: the evolution service (C4) is an object-level P7 component that *calls* the instrument level (ADR-0002 separation of proposal from deployment). A hosted participant appears at the object level only as `AgentProcess.hosted = OpaqueProcess` (WS-A3), never with sub-entities.

## 2. The seven planes (v1: a partition with a home-plane rule)

The table is unchanged from v0 (names, core questions, mechanisms, failures).

| # | Plane | Core question | Typical mechanisms | Characteristic failure |
|---|---|---|---|---|
| 1 | **Observation & context** | What does the model see now? | System instructions, history, retrieval, compaction, tool descriptions, artifacts, multimodal observations | Context rot, omitted evidence, stale/poisoned memory |
| 2 | **Action & tools** | What can it do, and how? | Shell/code, filesystem, browser, MCP, APIs, computer use, dynamic tool discovery | Tool misuse, schema friction, excessive tokens, non-composable interfaces |
| 3 | **Control & orchestration** | Who decides the next step? | ReAct loop, FSM/workflow, routing, planning, retries, stopping, subagent scheduler | Loops, premature stopping, control-flow hallucination, coordination overhead |
| 4 | **Verification & feedback** | How does the system know it is right? | Tests, linters, validators, end-state checks, critics, task contracts, proof-of-work | Plausible but ungrounded completion; reward hacking; weak judges |
| 5 | **State & durability** | What survives a turn/process/window? | Filesystem, git, event log, checkpoints, session store, progress files, memory layers | Lost progress, duplicate side effects, unrecoverable crashes |
| 6 | **Security & governance** | What is the maximum allowed blast radius? | Sandbox/VM, permissions, capabilities, egress controls, credential isolation, audit | Prompt injection, exfiltration, over-broad privileges, approval fatigue |
| 7 | **Measurement & evolution** | How does the harness improve? | Traces, evals, A/B tests, attribution, candidate edits, canaries, rollback | Overfitting, regressions, model-specific brittleness, untraceable changes |

**Design principle (v0, kept):** hard invariants live in deterministic software; open-ended search, decomposition, interpretation and recovery tactics live in the model; move the boundary only when evaluation shows a benefit.

### 2a. What a plane is (v1)

- A plane is a **partition of harness responsibilities**, not an architectural layer: planes carry no dependency direction; the build DAG is by tier (C0–C4) and stage, never by plane.
- **Home-plane rule:** every IR entity kind (WS-A3), component class (WS-A5), plane operator and event family (WS-B1) declares exactly one `home ∈ {P1..P7} ∪ {model_boundary, environment_boundary} ∪ {run_lifecycle}`. Cross-plane relations are typed edges. `classify_home(kind)` is total over registered kinds; an unclassifiable kind is a schema error (`UnclassifiedKind`) — the reflexive LCD check that nothing hides "between planes".
- The seven planes are a **superset classification**: every industrial decomposition audited (AIOS kernel S-030/S-031; LangChain anatomy S-045; Cloudflare runtime/harness S-056; Harness-Bench definition S-062) is a strict subset, and all omit P4 and treat P7 as telemetry only (WS-A2 F1).
- Adding a plane or boundary requires an ADR plus reclassification of affected kinds (OQ-052). No eighth column may be added ad hoc.
- Event stamps: WS-B1's `plane ∈ {lifecycle, context, model, action, control, verification, security, measurement}` maps as `lifecycle → run_lifecycle` (instrument level), `model → model_boundary`, the rest → P1..P7 in order (CF-030).

### 2b. Two boundaries (v1)

| Boundary | Between | Boundary components (home = boundary, not a plane) | Precedent |
|---|---|---|---|
| **Model boundary** (M \| H) | the harness and every model snapshot it binds | model gateway ("model adapter", WS-C1), model router, Profile Compiler, prompt/KV/semantic cache, model-call interposition | AIOS `LLMAdapter` behind a syscall queue; Omnigent `Executor.run_turn`; Cloudflare "model execution" inside the harness |
| **Environment boundary** (H \| E) | the harness and the external world | environment/sandbox handle, effect capture, egress mediation | doc 2 §11 execution environments; WS-B5/E5 |

Doc 3 §2.3's "Model plane" is a scope-catalogue grouping of model-boundary components (CF-026). `propose` (the model call) and `execute` (the environment effect) are the two plane operators that cross a boundary.

### 2c. Control boundary β (v1)

- **Decision points** Δ = {next_action, continue_or_stop, retrieve_or_compact, delegate, authorize, verify, retry, escalate} (extensible by ADR).
- **Control boundary** β : Δ → {code, model, human}, with per-decision-point guards (the control envelope, WS-F2). β is an element of θ (`typed-by: WS-A3`; representation open — OQ-051), a `component-level` factor at the instrument level, and observable in trajectories (a step whose owner is `code` has no model call — Harbor ATIF `Step.llm_call_count == 0`; Omnigent `Executor.handles_tools_internally`).
- The v0 design principle becomes a *default assignment* of β, not a fixed decision (CF-005 stays a Lab comparison target).

## 3. Three cross-cutting properties (v1: mandatory attributes, never planes)

| Property | Definition (v0, kept) | v1 attribute on every entity and event | Owner WS |
|---|---|---|---|
| **Resource economics** | Resources consumed by every plane; the harness is a scheduler; "better" is judged on a capability–cost Pareto frontier. | `resource` accounting (dimensions `typed-by: WS-L2`) | WS-L2 |
| **Provenance** | Origin/version/authority sufficient to decide trust; flattening sources is the shared root cause of privilege escalation and revoked-memory failures. | `provenance{origin, version, authority_class}` (scheme = the single CF-006 scheme, `typed-by: WS-L3`/WS-A3) | WS-L3 |
| **Model conditioning** | The same policy/tool/memory has different effects across models; portability = stable semantic interface + model-conditioned compilation. | `conditioning{semantic_id, surface_owner}` — the surface/semantic split (WS-A3 `surface record`) | WS-A4, WS-C3, WS-E2 |

## 4. Participant classes (v1: formalized; ADR-0001 definitions unchanged)

| Term | Definition |
|---|---|
| **IR-native participant (white-box)** — short-form **native participant** | (ADR-0001, unchanged) A harness expressed in HIR and run on the reference runtime; component-decomposable, swappable, ablatable, causally attributable, evolvable. **Class criterion (v1):** θ is a Harness Definition known to the Lab, so H = compile(h, p) is *transparent*. |
| **Hosted-external participant (black-box)** — short-form **hosted participant** | (ADR-0001, unchanged) A third-party harness joined through the Hosting ABI; shares environments, evals, scorecards, cost/latency/audit; never pretends to be component-decomposable. **Class criterion (v1):** H is *opaque*; θ_hosted = product version + ABI-settable coordinates. Typed as `AgentProcess.hosted = OpaqueProcess` (WS-A3). |
| **Comparison plane** | (kept) The single experiment/eval/scorecard/analysis surface spanning both classes. |
| **Hosting ABI** (= *thin observational ABI*) | (kept, ADR-0005/0008) Minimum observable event set + per-participant capability declaration; depth OQ-004. |
| **Participant descriptor** (v1) | `ParticipantDescriptor{class ∈ {native, hosted}, hosting_mechanism ∈ {none, session-abi, model-boundary-intercept, container-installed}, capability_declaration, version_identity, observability_level ⊆ {events, model_io, end_state, ledger}, model_binding ∈ {bound(M-set), self-selected}}` — immutable per run; **class is declared, never inferred from mechanism**; `ledger ∈ observability_level ⇔ class = native` (CF-059 settled). |
| **Admissible granularities** (v1) | native ⊇ {component-level, configuration-level, product-level}; hosted ⊆ {configuration-level (only coordinates whose capability-vector entry is SUPPORTED), product-level}. A factor outside the set is `InadmissibleFactor`; results render `n/a`, never 0 (T-LCD-15). |
| **ABI projection** (v1) | proj_ABI : native ledger → minimum observable event set is *total* (any native run can be presented as a hosted run — ADR-0005 "act as a participant"); no edge runs from any HIR entity to the Hosting ABI (T-LCD-06). Grey-box = `hosted` with `observability_level ⊇ {model_io}`; observability is a level, never a class. |
| **Capability declaration / capability vector** (v0.1, kept; relation added) | `capability_vector = reconcile(capability_declaration, probes)` → `{declared, probed, unknown}`; DRIFT only when both sides are concrete and differ (Omnigent `verdict.py`); omitted-on-the-wire (ACP "UNSUPPORTED") is re-mapped to `unknown` at the analysis layer (CF-027). CF-020 confirmed distinct. |

### 4a. Adjacent-discipline boundary (v0, kept verbatim)

Prompt engineering (subset: instruction wording) · Context engineering (major subset: full token state) · Agent engineering (overlapping umbrella) · Agent framework (a means; not the configured harness) · Orchestration (one subsystem) · AgentOps/LLMOps (overlaps at telemetry/evals/rollouts) · Compound AI systems (broader) · Evaluation harness (a different sense of "harness"; can be used to measure runtime harnesses).

### 4b. IR entity/edge vocabulary (v1: typed by WS-A3)

Entities `Goal, Observation, ContextItem, Memory, Procedure, ToolCapability, Permission, Effect, Artifact, Validator, AgentProcess, Budget, HarnessRule` + leaves `Text, CompiledPayload`; edges `depends-on, supersedes, authorizes, produced-by, validates, delegated-to, derived-from`. Every entity carries version + provenance and a `home` plane. **This ontology defines the concepts; WS-A3 defines their typed representation** — see `WS-A3.additions.md` for `Text leaf`, `CompiledPayload`, `OpaqueProcess`, `semantic_id/version_id`, `surface record`, `extension slot`, `HIR dialect`, `sealed definition`, `HirDiff`.

### 4c. Core tiers & build stages (v0, kept)

`C0` HarnessHarness Core · `C1–C4` extension tiers along a declared DAG · Stages 0–6 per doc 2 §11, to be re-derived by Phase 5 as the staged build ladder.

### 4d. Formal model (v1, for Spec §2.3)

Objects M (snapshot; π_M), E, D, B, θ = (h, p, β, π_pol) as in §1. **Plane operators** compose into the harness step (§1 *Harness step*); π_system = H_{θ,E,B}[π_M] is the closed-loop policy induced by iterating it. **Objective** J and **harness effect** Δ as in §1. **Configuration** κ and **arm** (a set of κ under one hypothesis with matched B and separate `search_budget` / `eval_budget`) as in v0.1. Lineage, not load-bearing: temporal abstraction over a base policy (S-168); the formalism's only spec duty is to fix *what is held constant and what is varied* — it is never computed by the runtime.

### 4e. Validity vs compliance as a measured pair (v1)

- Events (`typed-by: WS-B1`): `context.artefact.delivered` (P1; one per artifact per model call; requires a compiled identity — WS-A4 closed-world rule), `context.artefact.activated{detector, signal?, detector_ref, confidence, evidence}` (P1), `context.artefact.followed{detector, detector_ref, verdict, confidence, evidence_ref}` (P4).
- `validity(α, at)` is model-free and identical across configurations for the same artifact version and check (testable, S-077 property). Artifact kinds declare `activation_observable`; kinds without an activation signal collapse the chain to delivered → followed.
- Compliance metrics are **class-scoped**: `MetricDeclaration{…, detector_classes_allowed ⊆ {deterministic, judged}}`; `requires_observability = {events}` for deterministic stages, `{model_io}` for judged stages; unavailable ⇒ `n/a`.
- Validity's two typed faces (WS-A3 `ValidityState`/interval vs the check outcome here) are one notion — see WS-A2 dossier §6.1 item 10.

### 4f. Compatibility surface as a derived view (v1)

`fit_surface(runs[], factors[], metric, model_form) → FittedSurfaceReport{metric, factors, levels, design, model_form, sample_sizes, estimates_with_ci, interaction_terms, configuration_ids[], fitted_at, expiry_condition}`; refuses `UnknownLevel`; model form is WS-J4's (OQ-050); the report inherits assumption-debt expiry (T-LCD-05 applied reflexively).

### 4g. First-class entities vs derived views (v1)

| First-class (identified, versioned, stored) | Derived views (recomputable; never stored as truth) | Reports (computed once, persisted with provenance) |
|---|---|---|
| Harness Definition, Model Profile, component variant, harness artifact, Budget, Configuration, Arm, Run (+ run envelope), ledger Event, ParticipantDescriptor, CapabilityDeclaration, ConformanceRecord, MetricDeclaration, AssumptionDebtRecord, environment image, task/suite | π_system, compatibility surface Ψ_θ, capability vector, scorecard values, compliance rates, opacity ratio, comparison granularity (a property of a factor), planes (a classification), cross-cutting properties (attributes) | LcdReport, lowering loss report, opacity report, FittedSurfaceReport, EquivalenceReport |

### 4h. Vocabulary rules added at v1 (amendments to `ontology.md` §5–§6)

1. **comparison granularity** enum values renamed `{component-level, configuration-level, product-level}` (CF-021); the noun *configuration* keeps its single meaning.
2. **surface vs semantic identity** row reworded (CF-034): identity is content-addressed over the *semantic projection* (canonical form of kind + semantic fields + references by semantic id), excluding surface, provenance and extension fields; "semantic equivalence" is not promised.
3. **No bare "capability"** in spec prose (CF-037): `ToolCapability` (P2 entity) · `Permission` (P6 grant) · *capability declaration* / *capability vector* (hosted participants) · *authority handle* (WS-H1 runtime object-capability).
4. **observability level** row: add "`ledger` is entailed by `class = native` and never declared independently" (CF-059).
5. **capability vector** row: add the relation `= reconcile(capability_declaration, probes)`.
6. Spelling: prose *artifact*; identifiers registered by WS-B1 and the LCD battery keep *artefact*; the glossary check treats them as one term (CF-032).
7. "Plane" is reserved for P1–P7; *comparison plane* (ratified, instrument level) is the one sanctioned exception and is never counted among the seven.

### 4i. Reflexive LCD check on the ontology's own abstractions

| abstraction | intersection risk | escape-hatch risk | silent-loss risk | mitigation |
|---|---|---|---|---|
| seven planes | a mechanism with no natural home | "misc" plane | straddling kinds lose a plane | home-plane rule is total; new plane by ADR; typed cross-plane edges |
| participant class | class inferred from mechanism | "grey-box" as a third class | native run through ABI loses `native` | class declared; observability orthogonal; proj_ABI total |
| validity/compliance | judged number read as validity | `followed` with no detector | kinds without activation signal | three-valued validity; detector provenance; `activation_observable`; `n/a` |
| compatibility surface | interpolating categorical axes | stored surface as truth | stale after model churn | `UnknownLevel`; derived view; expiry on the report |
| control boundary β | fixed default treated as the answer | untyped "owner: other" | β not recorded in trajectories | β ∈ θ; closed owner set; trajectory marker |
