# Ontology Register — canonical vocabulary (living glossary)

**Version:** v0 (Preflight seed, 2026-09-09) — pasted/condensed from doc 2 §1 (definition, formal view, policy stack, adjacent disciplines) and §4 (seven planes, control question, three cross-cutting properties), plus the doc 3 §1.1 participant-class ratification (ADR-0001).
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
| **IR-native participant (white-box)** | A harness expressed in the MetaHarness IR and run on the native runtime; component-decomposable, swappable, ablatable, causally attributable, evolvable. The primary path. |
| **Hosted-external participant (black-box)** | A third-party harness (e.g. Codex, Claude Code, OpenHands, Cursor CLI) joined through the **thin observational ABI** (start/resume/cancel, stream events, supply context/tools/skills, request permission, account cost). Shares environments, evals, scorecards, cost/latency/audit; never pretends to be component-decomposable. First-class but secondary. |
| **Comparison plane** | The single experiment/eval/scorecard/analysis surface spanning both participant classes. |
| **Thin observational ABI** | The deliberately minimal boundary through which hosted-external participants join (depth decided by WS-J6 in Phase 3). |

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

| Term | Definition | Owner WS | Status | Notes |
|---|---|---|---|---|
