## 2. Ontology and formal model

### 2.0 Purpose, scope and reading rules

This section is the vocabulary and formal model every other section resolves its symbols against. It specifies scope item **R-2.1.1** (ontology: seven-plane model, policy-stack formalism, validity vs compliance, compatibility surface, two participant classes), tier **C0**, **Must**, status `specified-by-ADR` (ADR-0012, ADR-0013, ADR-0014). Its text is Ontology **v4** (`registers/ontology.md`; §1–§4 are the v1 text of those three ADRs, unchanged through v2–v4) plus the amendments Phases 2–4 logged on them and their neighbours (ADR-0048, ADR-0145, ADR-0183, ADR-0208). Nothing is decided anew; a missing decision is marked as a gap inline.

| Rule | Statement | Source |
|---|---|---|
| Symbol resolution | θ, M, E, D, B, κ, β, Δ, 𝒟, Ψ, J, π_M, π_system each have exactly one definition, in §2.5 (AC-A2-2). Δ is the harness effect (§2.5.4); the closed decision-point set over which β ranges is written 𝒟 (§2.5.5) — ADR-0012 uses Δ for both, reconciled as a notation ruling in CF-474. | ADR-0012; CF-474 |
| Vocabulary tie-breaker | The Ontology register breaks vocabulary ties; WS-A2 owns concepts, WS-A3 arbitrates typed IR vocabulary, WS-B1 owns event identifiers (`plane.noun.verb`, prefix = home plane). The readiness glossary check fails on unregistered synonyms. | ADR-0008 rule 4; ADR-0048 rule 0 |
| Vocabulary rules | Prose *artifact*, identifiers `artefact` (CF-032). No bare noun *capability*: use `ToolCapability` (P2 entity), `Permission` (P6 grant), *capability declaration* / *capability vector* (hosted participants) or *authority handle* (kernel object-capability) (CF-037). *Native participant* / *hosted participant* are the short forms of ADR-0001's *IR-native participant (white-box)* / *hosted-external participant (black-box)*; first use expands. "Plane" is reserved for P1–P7; *comparison plane* is the one sanctioned exception. | ADR-0012 decision 9; ADR-0008; Ontology §4h |
| Neutrality | Contracts are operations, inputs/outputs, invariants and failure modes; no language, runtime, package or library is named. | ADR-0050 §8 |
| Evidence | The C0 content (definitions, event schema, factor tuple, class formalization) rests on Tier-A/B evidence — response-surface methodology (S-167), the options-framework lineage (S-168), source audits at pinned commits. The 2026 preprints S-077 and S-088 are `provisional (mech)` corroboration only; no number from them is load-bearing. | ADR-0012/0014 evidence (a); doc 3 §3.1 |

### 2.1 Two levels of the ontology

Every term belongs to exactly one of two levels (ADR-0012 decision 1). The split is implicit in every audited precedent — a harness kernel knows nothing of experiments, an experiment runner knows nothing of planes (WS-A2 F7) — and collapsing it is how the historical product-name collision arose (CF-013).

| Level | What lives there | Realized by |
|---|---|---|
| **Object level** | One harness: its seven planes, two boundaries, IR entities and artifacts, component classes/variants, policies, θ, the harness step, π_system. | The reference runtime; Harness IR and the compiler (§3). |
| **Instrument level** | The Harness Lab and everything that *compares* harnesses: participants and descriptors, configurations, arms, runs and the **run envelope** (run/turn lifecycle), metrics and `MetricDeclaration`s, conformance records, fitted-surface reports, the Hosting ABI, the comparison plane. | The Lab and results store (§6); telemetry and scorecard (§5, §10). |

Level rules: (1) the evolution service (C4) is an object-level P7 component that *calls* the instrument level; proposal is separated from deployment (ADR-0002). (2) An *exposure definition* — the Lab's own MCP artifact (ADR-0173) — is an object-level Harness Definition whose `ToolCapability` entities call instrument operations; instrument operations never become object-level entities (ADR-0183, CF-366). (3) A hosted participant appears at the object level only as `AgentProcess.hosted = OpaqueProcess`, never with sub-entities (ADR-0016/0018). (4) A run's discriminator is `run_kind ∈ {agent, experiment, surface, inbox, fleet}` (ADR-0183 §C.1; ADR-0208 CF-441); only `agent` runs drive a harness and carry a configuration, the others have no configuration, environment or metric cells, hold their own fenced writer, chain by `continued_from` and never enter subject aggregates.

### 2.2 The seven planes and the home-plane rule

#### 2.2.1 The planes

Names, core questions, mechanisms and failures are the v0 table, kept (ADR-0012 decision 2; doc 2 §4).

| # | Plane | Core question | Typical mechanisms | Characteristic failure |
|---|---|---|---|---|
| P1 | **Observation & context** | What does the model see now? | System instructions, history, retrieval, compaction, tool descriptions, artifacts, multimodal observations | Context rot, omitted evidence, stale/poisoned memory |
| P2 | **Action & tools** | What can it do, and how? | Shell/code, filesystem, browser, MCP, APIs, computer use, dynamic tool discovery | Tool misuse, schema friction, excessive tokens, non-composable interfaces |
| P3 | **Control & orchestration** | Who decides the next step? | ReAct loop, FSM/workflow, routing, planning, retries, stopping, subagent scheduler | Loops, premature stopping, control-flow hallucination, coordination overhead |
| P4 | **Verification & feedback** | How does the system know it is right? | Tests, linters, validators, end-state checks, critics, task contracts, proof-of-work | Plausible but ungrounded completion; reward hacking; weak judges |
| P5 | **State & durability** | What survives a turn/process/window? | Filesystem, git, event log, checkpoints, session store, progress files, memory layers | Lost progress, duplicate side effects, unrecoverable crashes |
| P6 | **Security & governance** | What is the maximum allowed blast radius? | Sandbox/VM, permissions, egress controls, credential isolation, audit | Prompt injection, exfiltration, over-broad privileges, approval fatigue |
| P7 | **Measurement & evolution** | How does the harness improve? | Traces, evals, A/B tests, attribution, candidate edits, canaries, rollback | Overfitting, regressions, model-specific brittleness, untraceable changes |

**Design principle (v0, kept):** hard invariants live in deterministic software; open-ended search, decomposition, interpretation and recovery tactics live in the model; move the boundary only when evaluation shows a benefit. ADR-0012 decision 6 reads this as β's *default*, not a fixed decision.

#### 2.2.2 What a plane is

- A plane is a **partition of harness responsibilities**, not an architectural layer: planes carry no dependency direction; the build DAG (§4, §9) is by tier and stage, never by plane (ADR-0012 decision 2).
- The seven planes are a **superset classification**: every audited industrial decomposition (AIOS S-030/S-031, LangChain S-045, Cloudflare S-056, Harness-Bench S-062) is a strict subset, and all omit P4 and treat P7 as telemetry (WS-A2 F1). Stating P4 and P7 as *planes of the harness* is what makes novelty claim (a) of ADR-0006 ("an IR spanning all seven planes") checkable.
- **Home-plane rule.** Every IR entity kind, component class, plane operator and event family declares exactly one `home ∈ {P1..P7} ∪ {model_boundary, environment_boundary} ∪ {run_lifecycle}`; cross-plane relations are typed edges; `classify_home(kind)` is total over registered kinds and an unclassifiable kind is the schema error `UnclassifiedKind` at IR validation — the reflexive LCD check that nothing hides "between planes" (ADR-0012 decision 2; ADR-0016). On every HIR node `home_plane` is a derived, total attribute, stored for querying and never free-set (ADR-0016).
- **Adding a plane or boundary** = an ADR + reclassification of every affected kind + a re-run of the totality check; the event `plane` enum change bumps the ledger `schema_version` and the HIR dialect; no `state` value is added to the enum; no "misc" column ever (ADR-0048 rule 17 resolving OQ-052; CF-030).

#### 2.2.3 Home assignments

Event families are homed by the identifier-prefix rule (ADR-0048 rule 0) through the `plane` enum mapping of ADR-0012 decision 3 / ADR-0026 (CF-030).

| Kind | Home | Source |
|---|---|---|
| `context.*` (incl. `context.artefact.delivered/activated`, `context.assembled`, `context.memory.*`, `context.compaction.*`) | P1 | prefix rule |
| `action.*` (`action.tool.*`, `action.effect.*`, `action.environment.*`) | P2 | prefix rule; `execute` crosses the environment boundary |
| `control.*` (`control.decision`, `control.guard.fired`, `control.budget.*`, `control.subagent.*`, `control.compute.decided`) | P3 | prefix rule |
| `verification.*` (`verification.artefact.followed`, `verification.validator.*`, `verification.claim.*`) | P4 | prefix rule |
| P5 state events | P5 via the owning operator `persist` | no `state` prefix exists; state-changing events are stamped by their owning operator's plane (CF-030 ruling) |
| `security.*` | P6 | prefix rule |
| `measurement.*` (incl. the instrument-produced `measurement.experiment.*`) | P7 | prefix rule |
| `model.*` (`model.call.*`, `model.route.decided`, `model.cache.resolved`, `model.surface.relowered`) | `model_boundary` | `model → model_boundary` |
| `lifecycle.*` | `run_lifecycle` (instrument level) | `lifecycle → run_lifecycle`; the run envelope owns no harness responsibility |
| Operators `render`, `interpret`, `authorize`, `execute`, `verify`, `persist`, `decide`, `observe` | P1, P2, P6, P2, P4, P5, P3, P7 | ADR-0012 decision 5 |
| Operator `propose` | `model_boundary` | the model call is the crossing point |
| Model gateway ("model adapter"), router, Profile Compiler, prompt/KV/semantic caches, model-call interposition | `model_boundary` | ADR-0012 decision 3; ADR-0118, 0121, 0124, 0127…0129 |
| Environment/sandbox handle, effect capture, egress mediation | `environment_boundary` | ADR-0012 decision 3; ADR-0136, 0100…0102, 0060…0062 |
| Component classes `control_strategy`, `compute_policy` | P3 | ADR-0103; ADR-0188 |
| Component class `context_policy` | P1 | ADR-0072 |
| IR entity kinds `Goal, Observation, ContextItem, Memory, Procedure, ToolCapability, Permission, Effect, Artifact, Validator, AgentProcess, Budget, HarnessRule`; leaves `Text`, `CompiledPayload` | derived by `classify_home(kind)` | ADR-0016; the per-kind value table is §3.1.3 (thirteen rows, one plane each) — interim under [[DEFERRED ADR-0216: OQ-467 — no ratified ADR publishes the per-kind `classify_home` table; ADR-0216 fixes the §3.1.3 rows as MUST-data placeholders until WS-A2/WS-A3 ratify them by an ADR-0048-class ruling]]. |
| Other registered component classes (compaction, retrieval, memory store, tool registry, verifier, critic, sandbox helper, hosting adapter, evolution proposer, …) | per their `ClassRecord` | ADR-0023. (Resolved by ADR-0151 D5 as amended: the registry's `ClassRecord` carries `home` beside `contract_version`, `declaration_schema`, `conformance_suite_ref`, `decision_points`, `metrics_declared`, `slot_key` and `tier` — §6.2 heading 3; the HIR/1 `ClassRecord` of ADR-0023 is its projection.) |

### 2.3 The two boundaries

The **model boundary** (M | H) and the **environment boundary** (H | E) are first-class; there is no eighth plane (ADR-0012 decision 3). In every audited runtime the objects doc 3 §2.3 grouped as a "Model plane" sit at the boundary (AIOS `LLMAdapter` behind a syscall queue; Omnigent `Executor.run_turn`; Cloudflare "model execution" inside the harness — WS-A2 F2), and "how do we call the model" is the crossing point of `propose`, not a responsibility; the "Model plane" is a scope-catalogue grouping of model-boundary components (CF-026).

| Boundary | Between | Boundary components (`home` = the boundary) | Crossing operator |
|---|---|---|---|
| **Model boundary** | the harness and every model snapshot it binds | model gateway, router, Profile Compiler, caches, model-call interposition | `propose` |
| **Environment boundary** | the harness and the external world | environment/sandbox handle, effect capture, egress mediation | `execute` |

The Hosting ABI is an *instrument-level* boundary, not one of these two (ADR-0012 decision 1). It is one of three verb-shaped bindings — `hh-embed/1` (embedding contract), `plugin_abi/1` (plugin ABI), `hh-hosting/1` (Hosting ABI) — and prose names the binding, never "the ABI" (ADR-0183 CF-383). Hosted participants are outside the router's scope; a Lab-set `model` coordinate on a hosted participant (interception or `set_coordinate`) is a `configuration-level` factor for that role only and never changes class (ADR-0121 G-7; ADR-0165 decision 9; ADR-0013).

### 2.4 Cross-cutting properties as mandatory attributes

The three cross-cutting properties are mandatory *attributes of every entity and event*, never planes (ADR-0012 decision 4).

| Property | Definition (v0, kept) | Attribute | Owner |
|---|---|---|---|
| **Resource economics** | Every plane consumes resources; the harness is a scheduler; "better" is judged on a capability–cost Pareto frontier. | `resource` accounting over the kernel dimension list; `Budget.dimensions` is that list | ADR-0039…0041; §8 |
| **Provenance** | Origin/version/authority sufficient to decide trust; flattening sources is the root cause of privilege escalation and revoked-memory failures. | `provenance{origin, authority: AuthorityClass, taint, readers, scope, derived_from, …}` — the single CF-006 scheme; `kernel > definition > principal > delegate > environment > external > unverified` | ADR-0033…0035; ADR-0016; §8 |
| **Model conditioning** | The same policy/tool/memory has different effects across models; portability = stable semantic interface + model-conditioned compilation. | `conditioning{semantic_id, surface_owner}` — the semantic/surface split of every HIR node | ADR-0015/0016; ADR-0020/0124; §3 |

### 2.5 The formal model

The formalism's only duty is to fix **what is held constant and what is varied**; the runtime never computes it (ADR-0012 decision 5). Its lineage — temporal abstraction over a base policy (S-168) — is lineage, not a load-bearing result.

#### 2.5.1 Objects

| Symbol | Object | Definition | Source |
|---|---|---|---|
| **M** | model snapshot | Provider, version identity and sampling parameters; induces π_M(a \| c) over rendered contexts c. Replaces "frozen model": a configuration pins the snapshot it *believes* it has, and served-model drift is a ledger fact (`model.call.completed{served_model, snapshot_id?, substitution}`; the served model is the call's model coordinate), never an assumption. | ADR-0012 (CF-033); ADR-0120 decision 4 |
| **M-set** | model set | The snapshots a run binds, with roles `ModelRole ∈ {primary, utility, compaction, subagent, judge, router_predictor}` (ADR-0121 D2; CF-468); typed as the sealed `ModelRoleTable{role → {model_ref, profile_ref}}` whose `semantic_id` is `configuration_id.model_ref`; the realized table is a manifest end-state fact. "The" model is the `primary` role. | ADR-0012 (OQ-048); ADR-0036; ADR-0121/0124 (CF-313) |
| **E** | environment | The external world with state s and an effect semantics, reached only through the environment boundary; identified by an `EnvironmentRecord` in κ. | doc 2 §1; ADR-0136/0137 |
| **D** | task distribution | The distribution of goals; every result row carries a task with suite id and split label. | doc 2 §1; ADR-0045 |
| **B** | budget vector | A vector over the kernel resource dimensions — counters (`tokens.*`, `model_calls`, `tool_calls`, `turns`, `retries`, `spawns`, `approvals.*`, `evaluator_calls`, `network.*`, `time.*`, `env.*`, `spend`) and gauges (`context.occupancy`, `fan_out`, `delegation_depth`); every Goal and AgentProcess references one. | ADR-0039 decision 1; ADR-0016 |
| **θ** | harness parameterization | θ = (h, p, β, π_pol): a Harness Definition h in HIR, a Model Profile p, a control boundary β, and the policies h references (permission, budget, retry, stopping). The v0 content list (prompts, context policies, tool schemas, middleware, state machines, memory policies, subagent topology, validators, permission rules, retry and stopping) is the *content* of h and p. h names component variants, parameters and policies and *may constrain* profiles; the profile is a configuration coordinate (CF-054). | ADR-0012 decisions 5, 9 |
| **H** | harness transformation | Native: H = compile(h, p), transparent. Hosted: opaque. | ADR-0013 decision 1 |

#### 2.5.2 Plane operators and the harness step

One **harness step** is the composition, around the model boundary, of plane-owned operators (ADR-0012 decision 5):

| Operator | Home | Operation |
|---|---|---|
| `render` | P1 | σ_t ↦ c_t: the context builder assembles a `ContextPlan` into the sealed `ProviderRequestPlan{transcript, tool_payload, params}` under budget; emits `context.assembled` and one `context.artefact.delivered` per artifact per model call. |
| `propose` | model boundary | a_t ~ π_M(· \| c_t) when β assigns the point to `model`, else a deterministic (code) or human choice; the gateway stamps `model.call.*`. |
| `interpret` | P2 | Parse/validate a_t into a typed action against the compiled surface (`action.tool.proposed`). |
| `authorize` | P6 | The reference monitor confers or refuses authority out-of-band from text (`security.permission.decided{decider}`); always code-owned. |
| `execute` | P2 → environment boundary | Effect on E through the environment handle and sandbox helper, captured as `action.effect.intended … committed … observed`. |
| `verify` | P4 | Validators, oracles, critics reconcile claims with state; emits `verification.artefact.followed` where the target carries a `delivery_id`. |
| `persist` | P5 | Append events, checkpoint; the run ledger is the authoritative record (ADR-0026). |
| `decide` | P3 | The strategy *proposes* a `ControlDecision`; the value-of-compute scheduler *binds* unbound parameters (never proposes, refuses, grants or owns β); the envelope *checks*; the driver *executes* (`control.decision{decision_point, owner, verdict}`). The four parties never merge (ADR-0103; ADR-0188; ADR-0208 §A). |
| `observe` | P7 | Records every operation; no telemetry store, exporters are subscribers (ADR-0042). |

Rules: a *turn* is principal-input → idle (`lifecycle.turn.*`), a *harness step* is one `propose` cycle (ADR-0103 decision 9, CF-222); every `action.effect.intended` carries its licensing `decision_id` in `causes[]` (ADR-0103 I1); no run-time operation may deliver a surface or artifact without a compiled identity — the closed-world rule that makes `context.artefact.delivered` attributable (ADR-0019).

#### 2.5.3 The policy stack

**π_system = H_{θ,E,B}[π_M]** is the closed-loop policy over environment states induced by iterating the harness step; harness edits are policy interventions at named decision points; the stack is *partially programmable* because β assigns each decision point to `code`, `model` or `human` (ADR-0012 decision 5). Three objections are absorbed by definition (ADR-0012 evidence (c)): π_M is nonstationary (hence *snapshot* with drift detection), a harness binds several models (hence *model set* with roles), and π_system is a description, not a computable object (hence never computed).

#### 2.5.4 Objective and harness effect

- **Objective.** J(θ; M, E, D, B) = E_{τ∼π_system}[U(success, quality) − λ_c C(τ) − λ_l L(τ) − λ_r R(τ)] subject to c(τ) ≤ B and the safety and permission constraints (v0, notation fixed; ADR-0012).
- **Harness effect.** Δ(θ, θ₀ | M, E, D, B) = J(θ) − J(θ₀), always **paired**, always at **matched B**, never an absolute score (ADR-0012; T-LCD-14). Operational forms: `paired_effect(arm_a, arm_b, metric) → Dist(Δ)`, refusing `UnmatchedBudget`; and the `ComparisonReport` with `benefit_kind ∈ {artifact_benefit, search_time_benefit, transfer}` and `budget_match.status` (ADR-0041/0046; §10) — the only admissible form of any "X beats baseline" claim in this spec. For `matched_cap`, every arm's `budget_enforcement[dimension]` must be `enforced` for each dimension in the `MatchSpec`, else `IncommensurableMatch` (ADR-0165 decision 4).

#### 2.5.5 The control boundary β

β is the object named by doc 2 §9's "moving boundary of control" (ADR-0012 decision 6).

| Element | Specification | Source |
|---|---|---|
| Decision points 𝒟 | 𝒟 = HIR/1 `DecisionPoint = {plan, act, retrieve, compact, verify, delegate, authorize, retry, stop, escalate}`, closed; extended only by dialect bump (`route`/`effort` scheduled for HIR/2, OQ-425). | ADR-0048 rule 10 (CF-107); ADR-0208 |
| β | β : 𝒟 → {code, model, human} with per-decision-point guards (ADR-0012 writes the domain Δ; 𝒟 here per CF-474 so that Δ keeps its single meaning as the harness effect). | ADR-0012; CF-474 |
| Representation | `ControlBoundary{assignments: map<DecisionPoint, {code, model, human}>, guards: map<DecisionPoint, Predicate>}` — a typed **record on `AgentProcess.native`**, not an entity (OQ-051). `guards` is the typed content of the F2 `EnvelopePolicy` (content-addressed, sealed with the definition); guards are predicates over ledger-derived state, may reference deterministic Validators `{schema, predicate, executable}` at C0, and a `judge` Validator enters only at C2 as an input to `escalate` (OQ-062). | ADR-0016; ADR-0106 decision 1; ADR-0103 decision 8 |
| Envelope-reserved points | `authorize` is always `code`; `stop{budget_exhausted \| context_exhausted \| cancelled}` and infrastructure `retry` are always `code`; `validate`/`open` refuse a contrary `ControlBoundary` (`IncompatibleBoundary`). A `model`-owned `stop{completed}` is a *proposal*, accepted only when open effects are closed, a required submission exists and every `stop` guard passes. | ADR-0103 I5, I6 |
| As a factor | `component-level` (native only); hosted participants expose β only through ABI-settable coordinates. CF-005 remains a Lab comparison target. | ADR-0012; ADR-0013 |
| As an observable | Every `control.decision` carries `{decision_point, owner}`; `boundary_observed` vs declared `assignments` is `boundary_drift`. A code-owned step has no model call (precedents: Harbor ATIF `llm_call_count == 0`; Omnigent `handles_tools_internally`). | ADR-0103 I8; ADR-0012 |
| Stage | *Representable* at C0/Stage 1; *variable* at C0/Stage 3 (WS-F1). | ADR-0012 evidence (e) |

#### 2.5.6 Configuration and arm

- **Configuration** κ = (M-set, h, p, E, B, seed) is one factorial point; results are keyed by configuration id (ADR-0012). Two identities exist (ADR-0036 decision 3, CF-083): **`configuration_id`** — semantic coordinate over `semantic_id`s, seed excluded; results *aggregate* by it — and **`configuration_version_id`** — exact-bytes manifest key over `version_id`s, seed included; rows are *keyed* by it. `configuration_id.model_ref` is the `ModelRoleTable`'s `semantic_id`; `environment_ref` is the `EnvironmentRecord.semantic_id`; `bundle_id` is recorded but never a coordinate (CF-108).
- **Seed** is the replicate axis (CF-095): `replicate` carries seed material (harness RNG, requested sampling seed, image digest); pairing across arms is by task, by replicate only where every participant declares `seed_honoured` (ADR-0045 decision 2).
- **Result-row key** (T-LCD-09): `(model snapshot(s) with roles, harness at a declared granularity, environment image + fault/perturbation profile, task with split label, budget vector, replicate index)` (ADR-0045).
- **Arm.** A set of κ under one hypothesis with matched B and separate `search_budget` / `eval_budget` (ADR-0046; T-LCD-14); `Design` and `PreRegistration` are first-class and immutable once the first run opens (ADR-0045).

#### 2.5.7 The compatibility surface

**Ψ_θ : F → Dist(Δ)** maps the **factor space** F — model snapshot, task distribution, environment image, budget vector, context window, tool-surface target, control boundary β, component variant, profile, hosting mechanism — to the *distribution of the paired harness effect* (ADR-0012 decision 7). It is a response surface in the textbook sense (S-167, Tier A); its interesting terms are interactions on mostly categorical axes with curves only along budget-like axes (WS-A2 F5).

| Rule | Statement | Source |
|---|---|---|
| Derived view | Never stored as truth; persisted only as a **`FittedSurfaceReport`** `{metric, factors, levels, design_ref, model_form, sample_sizes, estimates_with_ci{main_effects[], interactions[], curves[]}, conditionality_region, portability{sign_stability, n_families}, configuration_ids[], unknown_cells[], fitted_at, expiry_condition, status ∈ {active, expiring, expired}, debt_record_ref, generated_from}`. | ADR-0012 decisions 7–8; ADR-0160 decision 4 |
| Operation | `fit_surface(rows, factors[], metric, model_form, MatchSpec) → FittedSurfaceReport + AssumptionDebtRecord`; `model_form ∈ {contrast (C1 default), factorial_glmm (C2), curve_on_ordered_axis}`; one `MatchSpec` per surface (`IncommensurableMatch` otherwise). | ADR-0160 decisions 1–2 (OQ-050) |
| No interpolation | Categorical axes are never interpolated; an unprobed level is `UnknownLevel`; cells below the minimum design (≥ 2 levels, ≥ 3 replicates, ≥ 30 tasks) are `unknown{insufficient, n}`, never dropped or estimated (T-LCD-07). | ADR-0012; ADR-0160 decision 3 |
| Admissibility | Component-level axes yield a native-only surface; hosted rows join only on SUPPORTED configuration-level coordinates. | ADR-0160; ADR-0013 |
| Expiry | The report's `AssumptionDebtRecord` expires on supersession/retirement of any snapshot, profile, suite or image in `configuration_ids` or on `max_age` (placeholder 90 days, OQ-368); it is then annotated `expired`, never hidden (T-LCD-05 reflexively). | ADR-0160 decision 5 |
| Portability / conditionality | *Portability* of θ = sign-stability of Ψ_θ along the model axis (reported only with ≥ 2 families and ≥ 1 held-out level; any `unknown` model-axis cell ⇒ `inconclusive`); *conditionality* = the region of F where Δ > 0 with stated confidence. | ADR-0012; ADR-0160 decision 7 |
| Consumers | `conditionality_region` entries may serve as `evidence_refs` in a rule's debt record; the engine never emits a rule, profile, surface or routing decision. | ADR-0160 decision 6 (OQ-019) |

### 2.6 Validity vs compliance as a measured pair

Harness **validity** (the artifact encodes a correct policy or fact) and **compliance** (the beneficiary model notices, activates and follows it) are measured separately and never reported as one number (ADR-0014). S-077 (validity decided by protocol-layer stamps is identical across models while compliance with identical bytes varies by model and flips across versions of one family) and S-088 (compliance fails at *activation* and *following* separately) corroborate the mechanism as `provisional (mech)`; the C0 content rests on Tier-B source precedents (OpenHands `SystemPromptEvent` / `activated_skills`; AIOS `ContextInjector` records) and the LCD battery (T-LCD-13).

#### 2.6.1 Validity

| Item | Specification | Source |
|---|---|---|
| Contract | `validity(α, at: event_id) → {valid, invalid, unknown, check_ref}`, decided by a protocol-layer check that **never consults the beneficiary model** — version stamp, executable validator (`validates` edge), external oracle, or the declared validity interval; `unknown` when none exists. | ADR-0014 decision 1 |
| Invariant (testable) | Identical result for the same artifact version and check across all configurations (the S-077 property). | ADR-0014 |
| Two faces, one notion | WS-A3's `ValidityState` / `validity{valid_from, valid_until \| invalidation_condition}` is the *declared interval* on a Memory/ContextItem/Permission; `validity(α, at)` is the *outcome of a check at an event*. For memory versions it is the `check_ref` projection of the derived `lifecycle_state(v, at) → valid \| superseded{by} \| revoked{record} \| expired{reason} \| stale_by_dependency{revoked_inputs[]} \| unknown`, precedence `revoked > superseded > expired > stale_by_dependency > valid > unknown`; no version is ever mutated. | ADR-0012 decision 9 item 10; ADR-0081 decision 1 |
| Memory obligation | Every memory artifact declares a check or interval (CF-008/CF-028); retrieval order validity → authority → readers → relevance is a C0 contract. | ADR-0014; ADR-0034 P5; ADR-0048 stage note R-2.4.4 |

#### 2.6.2 The compliance chain

Events are typed by WS-B1 with identifiers fixed at synthesis (ADR-0048 rule 1, CF-101); all three carry the envelope's `participant_class`, `observability_level` and `configuration_id` (ADR-0026).

| Event | Home | Payload beyond the envelope | Rule |
|---|---|---|---|
| `context.artefact.delivered` | P1 | `artefact_id, delivery_id, version, kind, activation_observable, model_call_id, position, rendering_ref, provenance` | emitted by the context builder for every artifact placed in c_t; one per artifact per model call; requires a compiled identity; `UndeliverableArtifact` if identity/provenance is missing |
| `context.artefact.activated` | P1 | `artefact_id, delivery_id, detector ∈ {deterministic, judged, human} (one sum with `Verdict.detector`, ADR-0110; ADR-0014 as amended, CF-483), signal? ∈ {cited, tool_used, procedure_invoked, loaded, retrieved}, detector_ref, confidence, evidence` | deterministic detectors are P1/P2 components (skill load, handle-only expansion, retrieval injection, tool call naming the artifact); judged detectors are P4; never emitted for kinds with `activation_observable = false` |
| `verification.artefact.followed` | P4 | `artefact_id, delivery_id, detector, detector_ref (validator_ref or judge configuration), verdict, confidence, evidence_ref` | emitted by `verify` when the target carries a `delivery_id`; deterministic only via a validator the artifact references; else judged; `NoDetector` ⇒ `n/a{no_detector}` |

A judge configuration is a `Validator{kind: judge}` with a `ProfileRef`, `calibration_ref`, independence from every beneficiary snapshot and `charged_to = instrument` (ADR-0047, ADR-0115/0116, CF-114); it is a conditioned component with expiry (T-LCD-05 reflexively).

**Harness artifact.** Any identified, versioned, provenance-bearing thing delivered to a model: `kind ∈ {instruction, rule, memory, tool_surface, procedure, observation}`, with `validity_check_ref?` and `activation_observable: bool`; `artefact_id` is the content-addressed `semantic_id` (for `tool_surface`, the surface record's hash). `HarnessArtifact` is a projection over existing HIR kinds, not a new entity; `activation_observable` sits on `ContextItem` and each delivery bears a `delivery_id` (ADR-0012; ADR-0016). Kinds without an activation signal (instructions inside a rendered prompt) collapse the chain to delivered → followed and say so in the metric (ADR-0014 decision 5).

**Detectors per kind (OQ-049, resolved).** Activation is deterministic on handle-only expansion for `procedure`, `tool_surface` and `memory` (`signal = loaded`), on plan entry for a workflow node, and on child spawn for a subagent task (ADR-0073, ADR-0085, ADR-0083). Following is deterministic iff the artifact references a `schema` or `trace_predicate` validator evaluable from `events`/`end_state` alone: `tool_surface` (args conform, effect class as declared), typed `procedure` (ordered decision/tool subsequence matches its steps), kernel-executed and soft-budget `rule`s (postcondition holds in the ledger), closed-schema and `recovery` `memory` (citation-in-action; first-try canonical-args match), and `instruction` only with such a `validates` edge; prose instructions and prose memories stay judged-only (ADR-0111 decision 5; ADR-0083 decision 7). The per-profile share of deliveries with `detector = deterministic` is a reported metric (AC-G1-9). `followed` has no source-code precedent and remains the chain's main unproven obligation at Stage 3 (ADR-0014 evidence (b)).

#### 2.6.3 Decomposition and class-scoped metrics

- **Reporting shape.** E[Δ] ≈ P(valid) · P(activated | delivered, valid) · P(followed | activated) · E[Δ | followed]; each factor is reported with its detector class and sample size; no bare "compliance score" (ADR-0014 decision 3). Compliance is keyed by model *snapshot* through the configuration; a sign flip across versions of one family is data (decision 4). ADR-0002's evidence item "beneficiary-model compliance" is computable through this chain.
- **Class scoping.** `MetricDeclaration` (single full form, §10; ADR-0045 as amended) carries `requires_observability ⊆ {events, model_io, end_state, ledger}`, `applies_to_classes ⊆ {native, hosted}`, `admissible_granularities`, `detector_classes_allowed ⊆ {deterministic, judged, human}` (CF-483), `oracle_classes_allowed`, and (Phase 3) `requires_capabilities`, `requires_mediation ∈ {any, observed, mediated(effects | egress | model_calls)}`; applicability = class ∧ observability ∧ capabilities ∧ mediation (ADR-0165 decision 6). Deterministic stages require `{events}`, judged stages `{model_io}`; unavailable ⇒ the typed `n/a{reason ∈ {class, observability, capability, mediation, estimator_undefined, not_run, no_detector}}`, never 0, never a configuration-level proxy (T-LCD-15).

### 2.7 Participant classes and admissible granularities

ADR-0001's classes are unchanged; ADR-0013 supplies their formal reading. The **comparison plane** — one experiment/eval/scorecard/analysis surface spanning both classes — is *precedented* (HAL, S-135); the class criterion and the admissibility derivation are the novel elements (ADR-0004 items (ii)/(iii)).

#### 2.7.1 Class criterion: transparency of H

| Class | Criterion | θ | Typed form |
|---|---|---|---|
| **native participant** | θ is a Harness Definition h in HIR known to the Lab; H = compile(h, p) is transparent; component-level variation is meaningful. | (h, p, β, π_pol) | `AgentProcess.native` |
| **hosted participant** | H is opaque. | product version + ABI-settable coordinates | `AgentProcess.hosted = OpaqueProcess{participant_ref, declared_capabilities (tri-state), observability_levels, version_identity, hosting_mechanism, supplies, budget, permissions}`, no component sub-entities |

Rules (ADR-0013 decisions 1–2, 5): (1) **class is declared, never inferred** from mechanism or observability (`ClassUndeclared`); observability is a level, never a class; grey-box = `hosted` with `observability_level ⊇ {model_io}`; model-boundary interception is an observability upgrade, never a class change (OQ-038; ADR-0165 decision 9). (2) **Class is relative to the Lab's knowledge of θ**: a native harness observed by a lab that does not own its definition is hosted *for that lab*; one exposed through the Hosting ABI (ADR-0005 "act as a participant") stays native for the lab that owns it. (3) A third "grey-box" class is rejected — it differs from hosted only in observability.

#### 2.7.2 Participant descriptor

`ParticipantDescriptor` is instrument-level, immutable per run, stored in the run manifest (`lifecycle.run.created`) — ADR-0013 decision 2; ADR-0026.

| Field | Domain | Rule |
|---|---|---|
| `class` | `{native, hosted}` | declared |
| `hosting_mechanism` | `{none, session_abi, model_boundary_intercept, container_installed}` (underscore form in records, hyphenated in prose) | `none` only on a native descriptor; `OpaqueProcess` never carries it (CF-351); a network-remote A2A peer is `session_abi` + `process_placement = remote_service` (ADR-0164 decision 7) |
| `capability_declaration` | a versioned, append-only record; the manifest pins one `version_id` (CF-084) | the claimed record; typed `CapabilityDeclarationRecord` with tri-state fields (ADR-0018) |
| `version_identity` | a `version_id` over the declaration | ADR-0036 |
| `observability_level` | `⊆ {events, model_io, end_state, ledger}` | **`ledger ∈ observability_level ⇔ class = native`** — entailed, never declarable (CF-059): a native-shaped export from a hosted product does not make H transparent |
| `model_binding` | `{bound(M-set), self_selected}` | a Lab-set `model` coordinate for one role leaves the participant hosted (OQ-048) |
| `hh.hosting/1` extension fields | `process_placement ∈ {in_environment, lab_host, remote_service}`; `budget_enforcement: map<dimension, {enforced, advisory, unenforceable}>` (derived from mechanism × placement × interception); `model_io_intercept ∈ {base_url, client_patch, none, unknown}`; `permission_surface ∈ {protocol, hook, approval_mirror, none}` | ADR-0165 decisions 2–3, 9; ADR-0164 decision 2 |

**Mediation** is orthogonal to observability: every hosted ledger row stamps `mediation ∈ {mediated, observed, unobserved}` (the kernel produced or gated the fact; reported live by the participant; reconstructed or synthesized); the adapter cannot assert `mediated`, and participant-reported tool events never produce `action.effect.*` rows (ADR-0164 decisions 4–6).

#### 2.7.3 Capability declaration vs capability vector

CF-020 is confirmed: the two stay distinct (ADR-0013 decision 3). `capability_vector = reconcile(capability_declaration, probes) → {declared, probed, unknown}` per dimension, verdicts `SUPPORTED / UNSUPPORTED / PARTIAL / NOT_APPLICABLE / UNKNOWN / SKIPPED / DRIFT`; DRIFT only when both sides are concrete and differ (symmetric), stored as experimental data in a `conformance_report{subject_kind: participant}` — one record kind, three subjects (ADR-0183 CF-325/387). An entry omitted on the wire (ACP "omitted = UNSUPPORTED") is re-mapped to `unknown` at the analysis layer (CF-027); `unknown` is never coerced in either direction (T-LCD-07). Until probes exist (C2/Stage 4) every hosted entry is `unknown` and admissible granularities collapse to `product-level` (ADR-0013 evidence (d)).

#### 2.7.4 Comparison granularity and admissible granularities

**Comparison granularity** — the unit at which a factor varies — has the values `{component-level, configuration-level, product-level}` (CF-021 resolved by rename; *configuration* keeps its single meaning): an IR component variant or parameter (native only); a non-component coordinate the Hosting ABI lets the Lab vary for any participant (model, context/tool/skill supply, permission policy, budget); a participant version as a whole.

| Class | `admissible_granularities(P)` | Engine behaviour |
|---|---|---|
| native | ⊇ {component-level, configuration-level, product-level} | β and every component variant are component-level factors |
| hosted | ⊆ {configuration-level restricted to SUPPORTED capability-vector coordinates, product-level} | a factor outside the set is `InadmissibleFactor`; the report renders `n/a{class}`, never 0 and never a proxy (T-LCD-15) |

`admissible_granularities` is derived from class and capability vector, never declared; it is an engine precondition at C0/Stage 1 and the mixed-class fixture is an acceptance test (ADR-0013 decision 4; AC-A2-4).

#### 2.7.5 ABI projection

`proj_ABI : native ledger → minimum observable event set` is **total**: every native run can be presented as a hosted run (ADR-0005), and no dependency edge runs from any HIR entity to the Hosting ABI (T-LCD-06). The Hosting ABI's descriptor *lowers to* `ParticipantDescriptor` and `OpaqueProcess` and may not add fields to native entities (ADR-0013 decision 5). Adapter zero — native runs through the projection — is the executable form of this decision; with `HostedEvent` and `proj_ABI` it is a C0/Stage 3 schema and fixture removable with the hosting tier (ADR-0166 AC-J6-2; ADR-0184 stage note R-2.10.6).

### 2.8 First-class entities, derived views and reports

Every concept is one of three things (ADR-0012 decision 8; Ontology §4g): derived views are never stored as truth; reports are computed once and persisted with provenance. An *analysis* spans many runs and depends on registry/pricing versions and a resampling seed, so it is a report, not a view (ADR-0157).

| First-class (identified, versioned, stored) | Derived views (recomputable; never truth) | Reports (computed once; persisted with provenance) |
|---|---|---|
| Harness Definition, Model Profile, component variant, harness artifact, Budget, Configuration, Arm, Run (+ run envelope), ledger Event, `ParticipantDescriptor`, `CapabilityDeclaration`, `conformance_report`, `MetricDeclaration`, `AssumptionDebtRecord`, `EnvironmentRecord`, task/suite, `Design`, `PreRegistration` | π_system, Ψ_θ, capability vector, `admissible_granularities`, scorecard values, compliance rates, opacity ratio (`opacity_ratio` static / `opacity_dynamic`, CF-100), memory `lifecycle_state`, comparison granularity, planes, cross-cutting attributes, `home_plane`, `boundary_observed` | `LcdReport`, lowering loss report, opacity report, `FittedSurfaceReport`, `EquivalenceReport`, `ComparisonReport`, `AnalysisReport` (+ ledger `AnalysisRecord`), `AttributionReport/1`, `DebtReport`, `EvolutionAcceptanceReport` |

### 2.9 Contracts, acceptance criteria and build stage

#### 2.9.1 Interface contracts

| Operation | → Output | Invariant / failure | Section |
|---|---|---|---|
| `classify_home(kind)` | `plane \| boundary \| run_lifecycle` | total over registered kinds; `UnclassifiedKind` at IR validation | §3 |
| `deliver(run, artefact_id, surface_ref, position)` | `context.artefact.delivered` | one per artifact per model call; compiled identity required; `UndeliverableArtifact` | §5 (D1) |
| `detect_activation(run, artefact_id)` | `context.artefact.activated{detector, detector_ref, confidence}` | never when `activation_observable = false` | §5 (D1/D3/D5) |
| `detect_following(run, artefact_id, window)` | `verification.artefact.followed{…}` | deterministic only via a referenced validator; `NoDetector` ⇒ `n/a{no_detector}` | §5 (G1/G3) |
| `validity(artefact_id, at)` | `{valid, invalid, unknown, check_ref}` | model-free; identical across configurations | §5 (D4) |
| `describe(participant)` | `ParticipantDescriptor` | immutable per run; `ClassUndeclared` | §6 (J6) |
| `admissible_granularities(participant)` | granularity set | derived; `unknown` never coerced; `InadmissibleFactor`; `n/a` never 0 | §6 (J3/J4) |
| `paired_effect(arm_a, arm_b, metric)` | `Dist(Δ)` | matched B; `UnmatchedBudget`, `IncommensurableMatch` | §6 (J4), §10 |
| `fit_surface(rows, factors[], metric, model_form, MatchSpec)` | `FittedSurfaceReport + AssumptionDebtRecord` | no categorical interpolation; `UnknownLevel` | §6 (J4) |
| `proj_ABI(native ledger)` | minimum observable event set | total; no reverse edge | §6 (J6) |

#### 2.9.2 Acceptance criteria

*(The nine headings of doc 3 §7.2 item 5 for R-2.1.1: responsibility §2.0; interface contract 2.9.1; acceptance criteria 2.9.2; build stage 2.9.3; data model 2.9.4; dependencies 2.9.5; failure modes 2.9.6; security & provenance obligations 2.9.7; test-matrix hooks 2.9.8. Numbering keeps the cross-references of other sections stable.)*

| AC | Criterion | Discharged by |
|---|---|---|
| AC-A2-1 | §2 contains the two-level statement, the seven planes with core questions, the two boundaries, the three cross-cutting attributes, and a `home` assignment for every entity kind, component class and event family named anywhere in the spec; a missing row fails readiness (the interim rows of §3.1.3 under ADR-0216 count as rows until OQ-467 closes). | §2.1–§2.4; §2.2.3; §3.1.3 `classify_home` table |
| AC-A2-2 | The formal model of §2.5 is present; every symbol used elsewhere (κ, B, β, Δ, Ψ, θ, M, J) resolves to §2. | §2.5 |
| AC-A2-3 (T-LCD-13) | The taxonomy contains the three chain events with `detector` provenance; a per-profile compliance metric is computable from ledger events alone for deterministic stages. | ADR-0026; ADR-0045; §5, §10 |
| AC-A2-4 (T-LCD-15) | Every metric carries `applies_to_classes` and `requires_observability`; a mixed-class comparison renders `n/a` on inadmissible cells; a one-native-one-hosted fixture demonstrates it. | ADR-0045; ADR-0013; §6, §10 |
| AC-A2-5 (T-LCD-09) | A configuration record has all six factors explicit and β is representable as a factor; "does variant V help M₁ more than M₂?" is expressible against the data model. | ADR-0036; ADR-0045; ADR-0160 |
| AC-A2-6 (T-LCD-06/-07) | `admissible_granularities` never coerces `unknown`; the spec DAG shows no HIR → Hosting ABI edge while `proj_ABI` is total on native ledgers. | ADR-0013; ADR-0164; §4 DAG check |
| AC-A2-7 | The glossary check passes with the v4 register: no unregistered synonym, no "meta-" sub-concept, "HarnessHarness" only as the proper noun. | §2.11; readiness pass |

#### 2.9.3 T-LCD statement and build stage

The ontology satisfies T-LCD-05 (reflexively: surface reports and judge configurations expire), -06, -07, -09, -13, -14 and -15; it does not itself satisfy T-LCD-01/-02/-03/-04/-10/-11 (§3) and does not supply detectors for every artifact kind (§5 G1/D4) — the ADR-0012/0013/0014 T-LCD statements.

Build stage (ADR-0012/0013/0014 evidence (e)): **C0/Stage 1** — this text; the `home` table; κ and both configuration ids; β *representable*; the descriptor schema with `class`; `admissible_granularities` as an engine precondition; the chain event *schema* with `detector`; `MetricDeclaration.detector_classes_allowed`; `activation_observable`; the `validity()` contract. **C0/Stage 2** — `delivered` emitted by the context builder. **C0/Stage 3** — β *variable*; `paired_effect` with the matched-budget precondition; deterministic `activated` and validator-based `followed` on the reference harness; the per-profile compliance metric; adapter zero. **C2/Stage 3–4** — judged detectors with provenance and cost attribution; `FittedSurfaceReport`s (`contrast` at C1, `factorial_glmm` at C2); capability declarations, probes, conformance records and the `proj_ABI` implementation.

#### 2.9.4 Data model

Records this item owns or fixes the shape of; each is canonical in the section named and cited here by owning ADR.

| Record | Fields (as fixed here) | Owning ADR | Canonical section |
|---|---|---|---|
| `ParticipantDescriptor` | `{class ∈ {native, hosted}, hosting_mechanism ∈ {none, session-abi, model-boundary-intercept, container-installed}, capability_declaration, version_identity, observability_level ⊆ {events, model_io, end_state, ledger}, model_binding ∈ {bound(M-set), self_selected}}` plus the `hh.hosting/1` extension fields; immutable per run | ADR-0013 D1; ADR-0164/0165 | §2.7.2; §6.6 |
| `CapabilityDeclaration` → `capability_vector` (derived) | per-dimension `{declared, probed, unknown}` with verdicts `SUPPORTED / UNSUPPORTED / PARTIAL / NOT_APPLICABLE / UNKNOWN / SKIPPED / DRIFT`; stored as `conformance_report{subject_kind: participant}` | ADR-0013 D3; ADR-0183 (CF-325/387) | §2.7.3; §6.6 |
| `home` table row | `(kind \| class \| operator \| event family) → home ∈ {P1..P7} ∪ {model_boundary, environment_boundary} ∪ {run_lifecycle}` | ADR-0012 D2; ADR-0016 (`home_plane` derived on every node) | §2.2.3; §3.1.3 |
| `MetricDeclaration` | `{…, applies_to_classes, requires_observability, detector_classes_allowed, veto?}`; `n/a{reason}` as a typed value | ADR-0045 | §5h.2 |
| Chain events | `context.artefact.delivered`, `context.artefact.activated`, `verification.artefact.followed` — payloads per §2.6.2 (canonical §5c.1 / §5f.1; §8.5); no second field list here | ADR-0014; ADR-0026 (schema per class) | §5c.1, §5f.1; §8.5 |
| `ControlBoundary` | `{assignments: map<DecisionPoint, {code, model, human}>, guards: map<DecisionPoint, Predicate>}` — a record on `AgentProcess.native` | ADR-0016; ADR-0106 D1; ADR-0103 D8 | §2.5.5; §3.1.3 |
| `FittedSurfaceReport` | as in §2.5.7 | ADR-0012 D7–D8; ADR-0160 D4 | §6.4 |
| `ComparisonReport` (form fixed here, owned elsewhere) | `benefit_kind ∈ {artifact_benefit, search_time_benefit, transfer}`, `budget_match.status` | ADR-0046 D4 | §10.2 |

#### 2.9.5 Dependencies

*Consumes:* the authority lattice and provenance record (ADR-0033/0035 — §8.1) for `authority` on every entity; the identity model `semantic_id`/`version_id` and both configuration ids (ADR-0036 — §8.3); the ledger envelope and class schema (ADR-0026 — §5a.1) for the chain events and `control.decision{decision_point, owner}`; `MetricDeclaration` and the outcome classes (ADR-0045 — §5h.2); the hosting extension fields and mediation stamp (ADR-0164/0165 — §6.6); the closed `DecisionPoint` sum (ADR-0048 rule 10 — §3.1). *Produces for:* §3 (the entity-kind set, `home_plane`, `ControlBoundary`), §5a–§5i (the chain events, β as an observable, the participant-class rule on every metric), §6 (`ParticipantDescriptor`, `admissible_granularities`, `proj_ABI`, `FittedSurfaceReport`), §10 (Δ as the only reportable effect; the `ComparisonReport` form).

#### 2.9.6 Failure modes

| Failure | Raised by | Rule |
|---|---|---|
| `ClassUndeclared` | `describe(participant)` | class is declared, never inferred from mechanism (ADR-0013 D1; CF-059) |
| `UnclassifiedKind` | `classify_home(kind)` at IR validation | `classify_home` is total over registered kinds (ADR-0012 D2) |
| `InadmissibleFactor` | design registration (`admissible_granularities`) | a factor outside the class's set is refused; cells render `n/a{class}` (ADR-0013 D4; T-LCD-15) |
| `UndeliverableArtifact` | `deliver` | compiled identity required; one delivery per artifact per model call (ADR-0014) |
| `NoDetector` ⇒ `n/a{no_detector}` | `detect_following` | deterministic only via a referenced validator; never a proxy (ADR-0014) |
| `UnmatchedBudget`, `IncommensurableMatch` | `paired_effect` | matched B is a precondition (ADR-0012; ADR-0041/0046; T-LCD-14) |
| `UnknownLevel` | `fit_surface` | no interpolation across categorical axes (ADR-0012 D7; ADR-0160) |
| `IncompatibleBoundary` | `validate`/`open` | envelope-reserved points are always `code` (ADR-0103 I5/I6) |
| `boundary_drift` (observable, not an error) | `control.decision` projection | declared `assignments` vs `boundary_observed` (ADR-0103 I8) |

#### 2.9.7 Security & provenance obligations

Participant class is declared, never inferred (ADR-0013 D1); `ledger ∈ observability_level ⇔ class = native` is entailed, never declarable (CF-059); `validity()` is model-free and identical across configurations (ADR-0014); `n/a{reason}` is a typed value never coerced to 0 or a proxy, and `unknown` is never coerced in either direction (T-LCD-07/-15); `model_claim` never satisfies a Validator alone and Observation authority never exceeds its source (ADR-0016 invariants); an adapter cannot assert `mediation = mediated` and participant-reported tool events never produce `action.effect.*` rows (ADR-0164 D4–D6); `ParticipantDescriptor` is immutable per run; the harness effect is always paired at matched B (ADR-0012; T-LCD-14).

#### 2.9.8 Test-matrix hooks

Fixtures: the mixed-class fixture and the one-native-one-hosted fixture (AC-A2-4/-6; ADR-0013 D4), the T-LCD-13 chain fixture with all three events and `detector` provenance (AC-A2-3), the six-factor configuration fixture with β as a factor (AC-A2-5). Checks: totality of `classify_home` over every registered kind, class, operator and event family (AC-A2-1; re-run whenever a kind is added — OQ-052), the §4.4 spec-DAG check for the HIR → Hosting ABI edge (AC-A2-6), the glossary check against Ontology v4 (AC-A2-7). Conformance: `proj_ABI` over the Stage-3 native runs (adapter zero — ADR-0166 AC-J6-2).

### 2.10 Reflexive LCD check

Applied per ADR-0007 to the constructs this section introduces (Ontology §4i).

| Abstraction | Intersection risk | Escape hatch | Silent loss | Mitigation |
|---|---|---|---|---|
| seven planes | a mechanism with no natural home | a "misc" plane | straddling kinds lose a plane | total home rule; new plane only by ADR; typed cross-plane edges |
| participant class | class inferred from mechanism | "grey-box" as a third class | a native run through the Hosting ABI loses `native` | class declared, relative to the Lab; observability orthogonal; `proj_ABI` total |
| validity/compliance | a judged number read as validity | `followed` without a detector | kinds without an activation signal | three-valued validity; detector provenance; `activation_observable`; typed `n/a` |
| compatibility surface | interpolating categorical axes | a stored surface as truth | staleness after model churn | `UnknownLevel`; derived view; debt record with expiry |
| control boundary β | the default treated as the answer | an untyped "owner: other" | β unrecorded in trajectories | β ∈ θ; closed owner set; `owner` on every decision; `boundary_drift` |
| model snapshot | a family name standing for a version | "latest" as a coordinate | provider substitution unrecorded | snapshot per configuration; served-model drift as a ledger fact with `annotate + split` (ADR-0120) |

### 2.11 Glossary

Canonical terms used elsewhere in the spec, with level and ratifying ADR; the Ontology register carries the full row.

| Term | Definition | Level | ADR |
|---|---|---|---|
| **HarnessHarness** | The product: a research instrument and reference runtime; proper noun only (the provisional pre-ADR-0011 name survives only in historical text). | — | ADR-0011, 0003 |
| **harness** / **agent** | The model-external machinery governing an agent's closed loop across seven planes; Agent = Model + Harness (+ Environment). | object | v0 seed |
| **model snapshot** (M) / **model set** / `ModelRoleTable` | Provider + version + sampling parameters, inducing π_M; the roled set a run binds; the sealed role table whose `semantic_id` is `configuration_id.model_ref`. | object | ADR-0012, 0036, 0121/0124 |
| **environment** (E) / **task distribution** (D) / **budget** (B) | The external world behind the environment boundary; the goal distribution; the vector over kernel resource dimensions. | object / instrument / object | v0 seed; ADR-0136; ADR-0039 |
| **harness parameterization** (θ) | θ = (h, p, β, π_pol). | object | ADR-0012 |
| **Harness Definition** / **Harness IR (HIR)** | The `HirDocument` naming variants, parameters and policies (may constrain profiles); the typed, behavioural, provenance-bearing representation (HIR/1, HIR/2; never "HTIR"). | object | ADR-0016, 0023; ADR-0008, 0015 |
| **Model Profile** / **Profile Compiler** | The versioned, expirable owner of every model-conditioned surface decision; the boundary component applying it. | object | ADR-0008, 0020, 0124 |
| **model gateway** ("model adapter") / **router** | The model-boundary transport; the model-boundary component choosing the snapshot per call and role. | object | ADR-0118; ADR-0121 |
| **plane** / **home plane** (`home_plane`) | A partition of responsibilities with a total home rule; the single plane, boundary or `run_lifecycle` a kind answers to (derived). | object | ADR-0012, 0016 |
| **model boundary** / **environment boundary** / **boundary component** | M \| H and H \| E, crossed by `propose` and `execute`; a component homed on a boundary. | object | ADR-0012 |
| **cross-cutting property** | `resource`, `provenance`, `conditioning` — attributes on every entity and event. | object | ADR-0012 |
| **plane operator** / **harness step** / **turn** | `render … observe`; one `propose` cycle; principal-input → idle. | object / instrument | ADR-0012, 0103 (CF-222) |
| **policy stack** (π_system = H[π_M]) | The closed loop induced by iterating the step; partially programmable via β. | object | ADR-0012 |
| **harness objective** (J) / **harness effect** (Δ) | The v0 objective; the paired difference at matched B, never absolute. | instrument | ADR-0012 (T-LCD-14) |
| **decision point** / `DecisionPoint` (the set 𝒟) | `{plan, act, retrieve, compact, verify, delegate, authorize, retry, stop, escalate}`, closed; the domain of β (CF-474). | object | ADR-0048 (CF-107), 0016 |
| **control boundary** (β) / `ControlBoundary` / **moving boundary of control** | β : 𝒟 → {code, model, human} with guards (CF-474); a record on `AgentProcess.native`; a component-level factor; its migration as models change. | object | ADR-0012, 0016, 0103, 0106 |
| **control envelope** / `EnvelopePolicy` / **control strategy** | The F2 kernel interpreting the sealed policy that is β's `guards` (only stops, refuses, tightens); the class owning `decide` (its `program` variant is the *deterministic-envelope strategy*). | object | ADR-0106; ADR-0103 (CF-220) |
| **configuration** (κ) / `configuration_id` / `configuration_version_id` | One factorial point; the semantic seedless coordinate; the seeded manifest key. | instrument | ADR-0008, 0012, 0036 |
| **replicate** / **arm** / **factor space** (F) | The seed axis of κ; a set of κ under one hypothesis with matched B and split budgets; the mixed space of factors. | instrument | ADR-0045, 0046, 0012 |
| **comparison granularity** / **admissible granularities** | `{component-level, configuration-level, product-level}`; the set a participant admits, else `InadmissibleFactor`/`n/a`. | instrument | ADR-0012/0013 (CF-021) |
| **compatibility surface** (Ψ_θ) / **fitted-surface report** / **portability** / **conditionality** | F → Dist(Δ), a derived view; its only persisted, expiring form; sign-stability along the model axis; the region where Δ > 0 with confidence. | instrument | ADR-0012, 0160 |
| **harness artifact** / `activation_observable` | A delivered, identified, versioned, provenance-bearing thing of kind `instruction, rule, memory, tool_surface, procedure, observation` (a projection over HIR kinds, one `delivery_id` per delivery); the per-kind activation flag. | object | ADR-0012, 0014, 0016 |
| **harness validity** / **harness compliance** / **compliance chain** | `validity(α, at) ∈ {valid, invalid, unknown}` model-free; delivered → activated → followed with `detector ∈ {deterministic, judged, human}` (CF-483). | object / instrument | ADR-0014 (T-LCD-13) |
| `context.artefact.delivered` · `context.artefact.activated` · `verification.artefact.followed` | The chain events (P1, P1, P4). | object | ADR-0014, 0048 (CF-101) |
| **detector** / **realized-benefit decomposition** | The deterministic or judged emitter of a chain event; E[Δ] ≈ P(valid)·P(activated \| delivered, valid)·P(followed \| activated)·E[Δ \| followed]. | object / instrument | ADR-0014 |
| **class-scoped metric** / `MetricDeclaration` / `n/a{reason}` | The single full metric form (`requires_observability`, `applies_to_classes`, `admissible_granularities`, `detector_classes_allowed`, `requires_capabilities`, `requires_mediation`); the typed not-applicable value, never 0. | instrument | ADR-0045 (CF-094), 0014, 0165 |
| **native participant** / **hosted participant** / **class criterion** | The ADR-0001 classes; native ⇔ θ known to the Lab (H transparent), hosted ⇔ H opaque; declared, relative to the Lab. | instrument | ADR-0001, 0008, 0013 |
| **participant descriptor** / **hosting mechanism** / **observability level** / **mediation** | The per-run immutable record; `session_abi` / `model_boundary_intercept` / `container_installed` (`none` only native); `⊆ {events, model_io, end_state, ledger}` with `ledger ⇔ native`; `{mediated, observed, unobserved}`, orthogonal. | instrument | ADR-0013, 0165; ADR-0005, 0183 (CF-351); ADR-0164 |
| **capability declaration** / **capability vector** / `conformance_report` | The claimed versioned record; `reconcile(declaration, probes) → {declared, probed, unknown}`; the one record kind for `{variant, profile, participant, snapshot_pair}` verdicts `SUPPORTED … DRIFT`. | instrument | ADR-0013 (CF-020), 0018, 0183, 0208 |
| **ABI projection** (`proj_ABI`) / **comparison plane** / **Hosting ABI** (`hh-hosting/1`) | The total map native ledger → minimum event set; the one surface spanning both classes; the thin observational ABI, one of three bindings with `hh-embed/1` and `plugin_abi/1`. | instrument | ADR-0013, 0005; ADR-0001, 0004; ADR-0164…0166, 0183 |
| **Harness Lab** ("the Lab") / **run envelope** (`run_lifecycle`) / `run_kind` / **run ledger** | Assembly, registry, experiment, comparison, results, hosting; the instrument-level run/turn lifecycle; `{agent, experiment, surface, inbox, fleet}`; the authoritative log + blobs + manifest. | instrument / object | ADR-0008, 0183; ADR-0012 (CF-030); ADR-0183, 0208; ADR-0026 |
| **first-class entity** / **derived view** / **report** | Stored-and-versioned vs recomputable-never-truth vs computed-once-with-provenance. | both | ADR-0012 |
| **component class** / **component variant** | A Core-defined operation-shaped slot contract; one registered implementation. | object | ADR-0008, 0023 |
| **conditioned rule** / **assumption-debt record** | A profile-owned rule justified by a hypothesised model deficiency; its mandatory record with hypothesis, evidence, owner, expiry and removal test. | object | ADR-0007/0008, 0197 |
| **surface vs semantic identity** / **opacity ratio** | `semantic_id` over the semantic projection (surfaces are profile-owned renderings); the pair `opacity_ratio` / `opacity_dynamic`. | object / instrument | ADR-0012 (CF-034), 0015; ADR-0045 (CF-100) |
| **LCD trap** / **escape hatch** | The intersection-only portability failure; any path bypassing the typed, profile-owned route. | — | ADR-0007 |
| **authority classes** / **authority handle** | `kernel > definition > principal > delegate > environment > external > unverified` (lifted content is `origin = import`, `authority = unverified`); the kernel object-capability, never in model context. | object | ADR-0033, 0048 (CF-082); ADR-0051 |
| **maturity flag** / `charged_to` | `{instrument-grade, research-grade}` on every C3/C4 item (`research-grade` never grounds a program claim); `{subject, instrument}` on every charge. | instrument | ADR-0208 (CF-453), 0209; ADR-0039 (CF-109) |
