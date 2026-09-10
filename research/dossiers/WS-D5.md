# WS-D5 — Skills & Procedure IR: typed preconditions/effects/validators; compile to instructions, workflow node, or subagent task

**Track:** D · **Phase:** 2 · **Tier(s) fed:** C2 (with a C0/Stage-2 slice and a C1/Stage-3–4 slice proposed below) · **Status:** dossier-written
**Owner agent:** WS-D5 research subagent (Fable 5.1) · **Date:** 2026-09-10
**Feeds spec section(s):** §7.2 #3 (Harness IR & compilation — the Procedure entity and its three lowerings), #5 (context & memory subsystem: procedural memory; verification subsystem: procedure tests), #8 (cross-cutting: provenance, versioning, extensibility of procedures) · **Scope register items:** R-2.4.5 (primary); constrains R-2.4.3 (D3 retrieval of procedures), R-2.8.5 (H5 skills), R-2.6.x (F1 hooks / F3 delegation), R-2.9.x (I5 induced procedures)

Product name per ADR-0011: **HarnessHarness** (the historical string "MetaHarness" appears only inside quoted titles and paths).

## 1. Scope & boundary

This workstream decides **procedural memory as a first-class IR object between prose and code**: (a) the *D5 profile* of the ratified HIR/1 `Procedure` entity (ADR-0016 entity 5) — the semantics of `preconditions`, `expected_evidence`, `allowed_capabilities`, `failure_handlers`, the retrieval surface, parameters/outputs, invocation policy and tests — expressed **only through the ratified `ext` slot at HIR/1 and a proposed HIR/2 promotion list** (CF-038); (b) the **three compilation targets** `instruction | workflow_node | subagent_task` and the total **selection rule** the compiler applies from profile and risk (ADR-0019 stage 2–3); (c) the **validation/test contract** for procedures and the model-free definition of procedure *validity* (ADR-0014); (d) the **lifecycle** of a procedure under the D4/L4 semantics (origin classes, induction from trajectories, promotion, supersession, revocation, expiry); (e) the **Agent Skills file-tree mapping** in both directions (`lift_skill` / `export(skill_tree)`), which makes a "skill" the *packaging* of a Procedure rather than a second entity; (f) the relation to **hooks** (as an input to WS-F1's OQ-H5-04, not a decision); (g) the D5-specific **supply-chain obligations** on top of WS-H5's trust model.

It explicitly leaves to neighbours: **WS-H5** — the extension record, trust record, attestation, `pin`, isolation classes and the model-install path (adopted verbatim); **WS-D1** — candidate kinds `procedure_index`/`procedure_body`, slots, the deterministic `activated` event on expansion; **WS-D3** — the retrieval contract and hierarchy in which procedures are one tier; **WS-D4** — memory lifecycle richness (conflict sets, expiry policies); procedures reuse its records; **WS-F1** — whether hooks are guards or rules (OQ-H5-04) and the `deterministic_replay` declaration; **WS-F3** — subagent spawning, which the `subagent_task` target lowers to; **WS-G1/G3** — executable validators and calibrated judges that procedure tests reference; **WS-E2** — the tool-interface compiler that composite surfaces route to; **WS-I5** — the evolution pipeline that *induces* procedures; **WS-C3** — the profile rule inventory (this dossier requests one new rule kind). Nothing here names a language, runtime, package ecosystem or framework; precedent paths are evidence, never dependencies (ADR-0050 §8).

## 2. Key questions

1. (doc 3 §5, verbatim) Design procedural memory as a first-class IR object between prose and code: procedure entity (preconditions, expected evidence, allowed capabilities, failure handlers, provenance, version, validation tests), retrieval like a skill, compilation (A4) to model-facing instructions, deterministic workflow node, or subagent task depending on profile and risk; relation to Agent Skills file trees and to hooks; supply-chain trust (H5).
2. (doc 3 §2, R-2.4.5) Skills & Procedure IR: typed pre/postconditions, effects, validators; compile to instruction/workflow/subagent.
3. (OQ-023) Is there a representation between prose memory and arbitrary code that supports typed preconditions/effects, composition, retrieval, validation, migration and repair?
4. (CF-038) Which Procedure richness fits HIR/1 through `ext`, and what must wait for an HIR/2 dialect?
5. (OQ-H5-07) May a skill declare arguments, effects and a closed output schema so structured outputs can reach `environment` authority while the prose stays `external`?
6. (OQ-049, procedure kind) Which `activated` / `followed` detectors are deterministic for procedures, per compilation target?
7. Added: what is the total function `target(Procedure, profile, policy)`, and what does the compiler refuse rather than degrade?
8. Added: what is the *validity* of a procedure (three-valued, model-free), and how does it interact with promotion and retrieval order (ADR-0034 P5)?
9. Added: is composition (procedure calls procedure) expressible at HIR/1 without opening `ProcedureStepKind`?

## 3. Sources consulted (4 streams — mark which were actually opened)

| stream | source (S-id or new) | opened? | what it contributed |
|---|---|---|---|
| Academic | S-095 Procedural Graphs (arXiv 2609.09153, abstract re-read 2026-09-10) | yes | `(procedure, relation, procedure)` triplets; "localizes the agent's active node"; guidance "biases the solver's next action without dictating it"; refiner "edits the graph's topology and attributes, committing edits that preserve or improve held-out validation performance while retaining rejected ones". `provisional` (single team, 2 days old). Mechanism only. |
| Academic | S-011 Voyager (abstract) | yes | "ever-growing skill library of executable code"; "self-verification for program improvement"; skills "temporally extended, interpretable, and compositional". Tier A, 2023. |
| Academic | S-015 AFlow / S-025 AFlow code @3f45721 | yes (code) | Workflow-as-code: `Workflow` subclass with `__call__`; optimizer emits `{modification, graph, prompt}` (`optimizer.py` L26–30) into `round_N/graph.py` + `prompt.py` (`graph_utils.py` L115–140); operators are a closed vocabulary (`operators.py` L46–387). |
| Academic | S-023 ADAS / S-170 code @2702bee | via WS-A3 | Archive entries `{thought, name, code, fitness, generation}` — an edit rationale on every procedure version. |
| Academic | **S-WS-D5-01** Boundary-Aware Skill Memory (arXiv 2608.22339, 2026-08-23) | yes (abstract) | "Skill Imitation Trap": retrieving skills from successful trajectories "paradoxically increases the model's confidence in wrong tool calls" (+47% wrong-tool margin); remedy = per-skill "applicability conditions, risk cues, avoidance rules, and recovery notes" turning a skill "from an unconditional action template into state-conditioned guidance". `provisional`; mechanism only. |
| Academic | **S-WS-D5-02** SkillZip (arXiv 2608.05604, 2026-08-06) | yes (abstract) | "unit mismatch: skills are retrieved as packages, compressed as text, and converted into execution graphs only after retrieval, whereas reliable reuse requires a contract-bearing procedural unit"; contracts = "boundary signatures, dependency closure, verifier reachability, and source-level expansion". `provisional`. |
| Academic | **S-WS-D5-03** Managing Procedural Memory (AFTER; arXiv 2606.23127, 2026-06-22) | yes (abstract) | 382 tasks / 22 skills; "some skills generalize broadly across tasks and models, whereas others become specialized to role-specific workflows and lose effectiveness under transfer"; multi-model traces transfer better. `provisional`; the transfer/expiry failure mode. |
| Academic | **S-WS-D5-04** How Well Do Agentic Skills Work in the Wild (arXiv 2604.04323, 2026-04-06) | yes (abstract) | 34,000 real skills; gains "degrade consistently as settings become more realistic, with pass rates approaching no-skill baselines"; primary failure = discovery/selection; query-specific refinement recovers (57.7% → 65.5%). `provisional`; selection is the bottleneck. |
| Academic | **S-WS-D5-05** Agent Workflow Memory (arXiv 2409.07429, 2024) | yes (abstract) | Workflows = "commonly reused routines" induced offline or online from trajectories and "selectively" provided; 24.6% / 51.1% relative gains (single team, 2024; Tier C, not `provisional` by the 2026 rule but unreplicated). |
| Academic | S-088 (validity vs compliance), S-103 (context privilege escalation), S-062 (Harness-Bench) | via WS-A2/L3/D1 | Delivered/activated/followed separation; skill-directory elevation vector; execution-alignment failure class. |
| OSS audit | **S-107 codex @0735c51** — `codex-rs/skills/src/model.rs` L8–95 (`SkillMetadata{name, description, short_description, interface, dependencies: SkillDependencies{tools: [SkillToolDependency{type, value, transport, command, url}]}, policy: SkillPolicy{allow_implicit_invocation, products}, path, scope, plugin_id}`), `parser.rs` L1–90 (frontmatter validation, `MAX_NAME_LEN = 64`, line-oriented repair), `invocation.rs` L1–80 (`detect_implicit_skill_invocation_for_command`: shell-command parsing recognises skill *script runs* and *document reads*), `core/src/skills.rs` L38–209 (`emit_explicit_skill_invocations` / `maybe_emit_implicit_skill_invocation`: telemetry `codex.skill.injected{invoke_type ∈ {explicit, implicit}, plugin_id, model_slug}` + `SkillInvocation{location: Host{path, scope} | Resource{id}}`), `core/src/mcp_skill_dependencies.rs` L1–80 (prompt-and-install missing MCP dependencies before use), `ext/skills/src/dynamic_skill_selector.rs` L1–58 (`CheapSkillSelector` trait: "deterministic, side-effect free, and cheap enough to run in shadow mode on every turn"; ten lexical/n-gram/LRU selectors; `method()` "suitable for experiment metrics"), `config/src/skills_config.rs` L20–90 (per-skill `enabled`, `max_context_tokens`, layer-stack rules), `guardian-context/src/trusted_skills.rs` L1–60 ("host-verified invoked user-owned skill paths" rendered at role `developer`) | yes | The richest skill model in the corpus: typed dependencies, invocation policy, explicit vs implicit *activation* events, deterministic shadow selectors, a token cap on the injected index. Negative: verified skill *paths* still render at a privileged role. |
| OSS audit | **S-119 gemini-cli @ed2ac40** — `packages/core/src/skills/skillLoader.ts` L1–192 (`SkillDefinition{name, description, location, body, disabled, isBuiltin, extensionName}`; frontmatter with fallback line parser; name sanitised), `tools/activate-skill.ts` L1–209 (`ActivateSkill` is a *tool*: confirmation dialog shows description + folder structure; on execute adds the skill directory to the workspace context and returns `<activated_skill>` with `<instructions>` + `<available_resources>`) | yes | Activation as a tool call = a deterministic `activated` event with a human approval of *delivery*; activation also *widens filesystem scope* (a grant conferred by loading — rejected below). |
| OSS audit | **S-111 opencode @9f8db11** — `packages/opencode/src/skill/index.ts` L1–354 (discovery across `~/.claude`, `~/.agents`, project dirs, `skills.paths`, `skills.urls` pulled at start; duplicate-name warning, last wins; `available(agent)` filtered by `Permission.evaluate("skill", name)`), `tool/skill.ts` L1–70 (`skill` tool asks permission `{permission: "skill", patterns: [name]}`, returns body + sampled file list), `packages/core/src/skill/guidance.ts` L1–76 (index rendered as a keyed `SystemContext` section with `update`/`removed` messages when the list changes) | yes | Skill loading gated by the same permission engine as tools; index as a diffable context section (D1 `diffable` slot precedent); URL-sourced skills unpinned (H5 counter-example). |
| OSS audit | **S-112 goose @fae91d0** — `crates/goose/src/skills/mod.rs` L33–60 (`SkillFrontmatter{name, description, metadata: map}` "per the agentskills.io spec … arbitrary metadata lives in this nested mapping"), L162–195 (`loaded_skill_context_with_args`, `argument-hint`, `arguments` list), `skills/arguments.rs` L1–60 (`$ARGUMENTS`, `$1..$n`, `$name` placeholder substitution) | yes | Skills with *positional and named arguments* substituted at load — the parameter precedent. |
| OSS audit | **S-113 pi-mono @400d690** — `packages/coding-agent/src/core/skills.ts` L60–115, L280–350 (spec-conformant name/description validation with diagnostics; `disable-model-invocation`; `sourceInfo`; skills rendered as name/description/location, body read through the `read` tool) | yes | Progressive disclosure through an ordinary read; invocation policy flag. |
| OSS audit | **S-110 software-agent-sdk @3fc7b22** — `openhands-sdk/openhands/sdk/skills/skill.py` L178–300 (`Skill{trigger: KeywordTrigger | TaskTrigger | PathTrigger, mcp_tools, inputs: [InputMetadata], version, license, compatibility, metadata, allowed_tools, disable_model_invocation, resources{scripts, references, assets}}`), L732–800 (`match_trigger` whole-token keyword; `match_path_trigger` glob; `${var}` inputs), `skills/trigger.py` (three trigger kinds), `tool/builtins/invoke_skill.py` L1–196 (`invoke_skill` tool with `readOnlyHint = true`; `DeclaredResources(keys = skill:<name>)`; records `invoked_skills`), `skills/execute.py` L1–119 (inline `` !`cmd` `` executed *at render time* "via shell with full process privileges") | yes | Triggers as precondition kinds (path glob is checkable; keyword is a retrieval hint); typed inputs; MCP dependencies; the **render-time execution** counter-example. |
| OSS audit | **S-117 OpenHarness @9b2efd7** — `src/openharness/skills/types.py` (`SkillDefinition{…, user_invocable, disable_model_invocation, model, argument_hint}`), `_frontmatter.py` (fallback to heading + first paragraph), `loader.py` L1–80 (bundled → user → project → plugin registration order) | yes | A `model` field on a skill (a per-skill model binding — a conditioned surface without a debt record; T-LCD-05 counter-example). |
| OSS audit | **S-109 claude-agent-sdk-python @ad77094** — `src/claude_agent_sdk/types.py` L2230–2250 (`skills: list | "all"` — "a **context filter**, not a sandbox: unlisted skills are hidden … but their files remain readable"), L263–540 (hook events `PreToolUse … PermissionRequest`; `PreToolUseHookSpecificOutput{permissionDecision ∈ {allow, deny, ask, defer}, updatedInput, additionalContext}`) | yes | Skill enablement is a delivery filter, not authority; hooks can *rewrite tool input* and return `allow` — the two hook powers D5 must position against H5's narrow-only rule. |
| Protocol | **S-WS-H5-13** Agent Skills specification (agentskills.io; opened directly) | yes | `name` (≤64, lowercase/digits/hyphens, must match directory), `description` (≤1024), `license`, `compatibility` (≤500), `metadata` (string→string), experimental `allowed-tools`; `scripts/`, `references/`, `assets/`; three disclosure levels (~100 tokens / <5000 tokens / on demand); `skills-ref validate`; no version, provenance, precondition, effect or test field. |
| Protocol | S-153 MCP 2026-07-28 @aa8ce04 — `schema/2026-07-28/schema.ts` L1602–1690 (`Prompt{name, description, arguments: [PromptArgument{name, description, required}], _meta}`, `prompts/get` returns messages) | yes | A protocol-level *procedure with typed arguments* and an extension slot — the lowering target for the `instruction` form of a parameterised Procedure. |
| Protocol | S-180 A2A @98853be — `specification/a2a.proto` L436–450 (`AgentSkill{id, name, description, tags, examples, input_modes, output_modes, security_requirements}`) | yes | Skill = advertised capability with identity, tags and examples; no body — the `subagent_task` form's external descriptor. |
| Production | S-039 Anthropic, Agent Skills (opened) | yes | "pre-loads the `name` and `description` of every installed skill"; body loaded "if Claude thinks the skill is relevant"; bundled scripts run "without loading either the script or the PDF into context"; "installing skills only from trusted sources"; no versioning mechanism. |
| Production | S-049 Cursor, S-047 Anthropic Managed Agents, S-050 Cursor dynamic context | via WS-A4/D1 | Guardrails "long gone" (debt), skills as discoverable file-like resources. |
| Binding | ADR-0014/0015/0016/0017/0019/0020/0021/0024/0025/0033/0034/0035/0037; WS-A3, WS-A4, WS-L3, WS-H5, WS-D1 dossiers; `lcd-test-battery.md`; ontology v1 | yes | Constraints applied throughout. |

Streams triangulated: academic, OSS source-code (nine repositories at pinned commits), protocol specifications (Agent Skills, MCP prompts, A2A skills), production write-ups. New sources are listed in `WS-D5.additions.md` (S-WS-D5-01…05); S-119 promoted S → P (also by WS-H5).

## 4. Findings

### 4a. Mechanism evidence

**F1 — Every audited harness already has a two-level procedure: a *retrieval surface* (name + description, ≤ ~100 tokens) and a *body* loaded on demand; none types the body.** Codex, gemini-cli, opencode, goose, pi, OpenHands and OpenHarness all implement the Agent Skills three-level disclosure; the body is a string rendered inside a wrapper element (`<activated_skill>`, `<skill_content>`, `<available_skills>`). The only typed parts are the frontmatter fields and, in codex/OpenHands, dependencies and triggers. **Implication:** the C0 lift of a skill is a Procedure with one `Instruction(Text)` step and a typed index; the body's opacity is measured (T-LCD-02, `procedure_instruction` bucket), not hidden.

**F2 — Activation is already an observable, deterministic event in three systems, and *implicit* activation is detectable from tool calls.** gemini-cli and opencode make activation a tool call; OpenHands records `invoked_skills`; codex emits `codex.skill.injected{invoke_type ∈ {explicit, implicit}}` where implicit invocation is *detected by parsing shell commands for reads of the skill document or runs of its scripts* (`invocation.rs`). **Implication (OQ-049, procedure kind):** `context.artefact.activated` is deterministic for procedures (D1's expansion event), and a deterministic `followed` detector exists for the `instruction` target — a trace predicate over subsequent tool calls that name the procedure's `allowed_capabilities` or read its bundled artifacts, carrying the procedure's `delivery_id` in `causes[]`. For the `workflow_node` target, `followed` is true by construction (the kernel executed the steps).

**F3 — Selection is the dominant failure mode, and deterministic shadow selectors are a precedent for making it a measurable component.** S-WS-D5-04 (`provisional`) finds gains collapse when the agent must select among 34k skills; S-WS-H5-04 shows description-only attacks flip selection. Codex's `CheapSkillSelector` trait requires selectors to be "deterministic, side-effect free … run in shadow mode on every turn" with a `method()` tag "suitable for experiment metrics" — ten variants (fielded BM25, character n-gram, LRU, routing cards, RRF). **Implication:** `ProcedureSelector` is a registered component class (T-LCD-08/-12) receiving `(ModelProfile, ResourceAccount)`; selection results are ledgered as `context.procedure.selected{method, candidates[], delivered[]}` so the Lab can compare selectors under matched budgets (T-LCD-14) and stratify by profile.

**F4 — Unconditional procedures mislead; preconditions must be part of the unit.** S-WS-D5-01 (`provisional`) measures the Skill Imitation Trap and remedies it with applicability conditions, risk cues, avoidance rules and recovery notes; S-WS-D5-03 (`provisional`) shows role-specific overfitting under transfer; OpenHands' `PathTrigger` ("activated when the agent touches a file whose path matches") is a *checkable* precondition while `KeywordTrigger` is a retrieval hint. Classical planning (preconditions/effects) is the Tier-0 root. **Implication:** split preconditions into **checkable predicates** (kernel-evaluated from records: path globs, capability presence, environment requirements, prior validator verdicts, artifact presence) and **applicability notes** (`Text` leaves rendered to the model; validity `unknown`). A Procedure delivered when a checkable precondition is false is an `Omission{reason: precondition}` at D1 admission — the trap closed structurally where the check exists, and measured (`followed`) where it does not.

**F5 — Parameters, dependencies and invocation policy are already typed in production and are absent from HIR/1's Procedure.** goose substitutes `$ARGUMENTS`/`$n`/`$name`; OpenHands has `inputs: [InputMetadata]` and `${var}`; MCP prompts carry `arguments[{name, description, required}]`; codex carries `dependencies.tools[]` (MCP servers to install first) and `policy.allow_implicit_invocation`; pi/OpenHands/OpenHarness carry `disable-model-invocation`. HIR/1 entity 5 has `steps`, `preconditions`, `expected_evidence`, `allowed_capabilities`, `failure_handlers`, derived `effects` and a `compile_hint` surface — no `parameters`, `outputs`, `index`, `invocation_policy` or `tests`. **Implication:** these travel in a registered `ext` block at HIR/1 and are proposed for HIR/2 (§6.1); `ext` never decides authority, budget or validity (ADR-0015), which is compatible because none of them does — tests decide *validity* only through `validates` edges, which are kernel edges.

**F6 — Render-time execution is a real pattern and must be refused.** OpenHands renders `` !`cmd` `` at skill load "via shell with full process privileges"; goose substitutes arguments at load. **Implication:** ADR-0019 purity and the closed-world rule apply: a procedure body is rendered from *declared slots* filled by declared capability reads (`fs_read`, `memory_read`) that are themselves ledgered effects; shell execution at render is `RenderTimeExecution`, a validation error. Dynamic context a skill wants (`git status`, date) is a `filling` slot bound to a `ToolCapability` invocation with its own effect record.

**F7 — Activation must not confer authority or scope.** gemini-cli's `ActivateSkill` "adds the skill's directory to the workspace context so the agent has permission to read its bundled resources"; codex renders host-verified skill paths at role `developer`; the Agent Skills `allowed-tools` field is "pre-approved tools" by the skill's own declaration. Under ADR-0033 rule 6 and WS-H5 L3 (claims are not grants), loading is delivery; grants come only from a `Permission` issued at ≥ `principal`. **Implication:** `allowed_capabilities` on a lifted skill is a *proposal* recorded in `declared_claims`; it becomes the Procedure's `allowed_capabilities` (and the `authorizes` edges V-EFF needs) only when the author accepts it at `seal` or a principal approves the seal diff — and bundled `references/`/`assets/` are `Artifact`s read through a declared `fs_read` capability scoped to the extension's content address, never through a widened workspace.

**F8 — Workflow-as-code and procedure-as-graph are the same object at different rungs of the ladder.** AFlow's optimizer emits `{modification, graph, prompt}` and loads `graph.py` per round; ADAS keeps `{thought, name, code, fitness}`; Procedural Graphs (`provisional`) keeps typed triplets edited by a refiner with held-out gating and *retains rejected edits*. **Implication:** an induced or optimised procedure is a `HirDiff` with `derived-from{hypothesis, trajectories, candidate_id}` (ADR-0017) whose acceptance is a matched-budget `ComparisonReport` (ADR-0046) — the retained-rejections idea is the Lab's results ledger, not a Procedure field. The `workflow_node` target is where a procedure whose steps are fully bindable stops being prose: the ADR-0024 ladder rung "typed parameter or entity → registered variant".

**F9 — Hooks and procedures are different objects: a hook is a trigger at a decision point; a procedure is a body.** Claude SDK hooks return `permissionDecision`, `updatedInput`, `additionalContext`; codex records `HookSource`; gemini extension policies may only narrow (WS-H5 §4c). A hook's *action* is frequently a script — exactly a `workflow_node` procedure. **Implication (input to OQ-H5-04):** model hooks as `ControlBoundary.guards` (or `HarnessRule`s) whose `action: Ref<Procedure>` with `compile_hint = workflow_node`; the guard's outputs are `{narrow: deny | ask, additional_context: ContextItem at ≤ definition}`; an `updatedInput` rewrite is admissible only as a `Procedure` output re-submitted through `authorize` (the monitor evaluates the rewritten args); `allow` from a non-`definition` origin is `AuthorityWidening`.

### 4b. Performance evidence (all `provisional`; none load-bearing)

| claim | source | tag | use here |
|---|---|---|---|
| +23.8% AppWorld, −4.6% AgentDojo ASR from boundary fields | S-WS-D5-01 | `provisional` | motivates the precondition split; number not used |
| 3.46× compression, 98.7% verifier reachability | S-WS-D5-02 | `provisional` | motivates section-level bodies at C2; not used |
| 3.7–6.7 points per refinement round; 73.1% cross-model | S-WS-D5-03 | `provisional` | motivates multi-model transfer tests before promotion |
| 57.7% → 65.5% Terminal-Bench with query-specific refinement | S-WS-D5-04 | `provisional` | motivates selector as a Lab factor |
| 24.6% / 51.1% relative gains from induced workflows | S-WS-D5-05 | 2024 single team | motivates the induction path; unmatched budget (T-LCD-14) |
| "match or surpass hand-designed" graphs | S-095 | `provisional` | motivates `derived-from` gating; not used |

Every "X beats baseline" above is an unmatched-budget claim under ADR-0041/0046 and is discounted accordingly.

### 4c. Source-code precedents (repo, path, what it does)

| concern | precedent | adopted? |
|---|---|---|
| Two-level disclosure (index + body) | all nine repos (F1); S-039 | yes — `index` surface + `Instruction` body; D1 `procedure_index`/`procedure_body` |
| Activation as tool call with confirmation | gemini `tools/activate-skill.ts` L70–100; opencode `tool/skill.ts` L20–27 | yes as *delivery* approval (H7 budgeted); **no** as scope widening (F7) |
| Implicit activation detection from commands | codex `skills/src/invocation.rs` L27–80; `core/src/skills.rs` L121–209 | yes — the `followed` trace-predicate detector |
| Deterministic shadow selectors with metric tag | codex `ext/skills/src/dynamic_skill_selector.rs` L28–58 | yes — `ProcedureSelector` component class |
| Typed dependencies (MCP servers) resolved before use | codex `skills/src/model.rs` L81–95; `core/src/mcp_skill_dependencies.rs` | yes — `depends-on` edges resolved at `resolve`; install through H5 model-install path |
| Invocation policy | codex `SkillPolicy.allow_implicit_invocation`; pi/OpenHands/OpenHarness `disable-model-invocation`; OpenHarness `user_invocable` | yes — `invocation_policy{model, principal, implicit}` |
| Positional/named arguments | goose `skills/arguments.rs`; OpenHands `inputs`; MCP `PromptArgument` | yes — `parameters` (typed; substitution is a compile-time `filling` slot) |
| Checkable path precondition | OpenHands `PathTrigger` + `match_path_trigger` | yes — `Predicate.path_glob` |
| Keyword trigger | OpenHands `KeywordTrigger`, whole-token | as *retrieval hint* only |
| Per-skill model binding | OpenHarness `SkillDefinition.model` | rejected — a conditioned surface belongs in a profile rule with a debt record (T-LCD-05) |
| Render-time shell execution | OpenHands `skills/execute.py` | rejected — `RenderTimeExecution` |
| Index token cap | codex `skills_config.rs` `max_context_tokens` | yes — D1 `budget_share` on the `procedures` slot |
| Index as diffable section with update/removed notices | opencode `core/src/skill/guidance.ts` L28–70 | yes — D1 `diffable` slot; "list supersedes previous" is kernel-authored |
| Skill enablement as context filter | Claude SDK `types.py` L2230–2250 | yes — `enabled` is data (ADR-0024 MUST-data), never authority |
| Workflow-as-code with edit rationale | AFlow `optimizer.py` L26–30, `graph_utils.py` L115–140; ADAS archive | yes — `derived-from` + `HirDiff`; `workflow_node` target |
| Last-wins duplicate names across locations | opencode `skill/index.ts` L120–135; OpenHands "project skill overrides" | rejected — `NameCollision` at `resolve` (H5); identity is `semantic_id`, names are histories (ADR-0037) |

The integrated design — a Procedure with checkable preconditions, three compile targets selected by a total rule, model-free validity from `validates` edges, and a lift/export pair against Agent Skills with a loss report — is `no-precedent / novel` as a contract; each element has the partial precedent named.

## 5. Disconfirming evidence & negative results

**Against typing procedures at all: the whole ecosystem converged on untyped Markdown bodies and it works.** Nine independent harnesses and the Agent Skills spec keep the body prose; S-039 reports production value from prose skills; typing may cost authoring friction and buy little. *Why this does not overturn:* the C0 lift keeps the body prose (one `Instruction(Text)` step) and adds only an index, provenance and a validity slot; typing beyond that is a *promotion experiment* on the ADR-0024 ladder that must lower the opacity ratio and pass a matched-budget comparison before it is kept. The design admits the prose rung; it refuses to make it the only rung.

**Against `workflow_node` as a default: deterministic execution loses the flexibility that makes skills useful.** S-065/S-070 (`provisional`) argue both sides; S-WS-D5-01's trap is a *retrieval* failure that deterministic execution would make worse if preconditions were wrong. *Why this does not overturn:* the selection rule chooses `workflow_node` only when every step is bindable and every predicate checkable; the target is a Lab factor (β for `act`/`verify` varies), so "deterministic hurts here" is a measurable interaction effect (T-LCD-09), not an assumption.

**Against checkable preconditions: most real applicability conditions are semantic ("use when the user mentions PDFs").** Keyword triggers are the industry's answer and they are not predicates. *Why this does not overturn:* semantic conditions stay as applicability notes (retrieval hints + model-facing text) with validity `unknown`; the design only insists that *where* a predicate is checkable (paths, capabilities, environment, prior verdicts) it is checked by the kernel, and that the two are labelled differently so the Lab can measure how much of a library is checkable.

**Against the `ext`-at-HIR/1 route: extension blocks are the LCD escape hatch by another name.** ADR-0015 admits `ext` precisely because it never decides authority, budget or validity. *Why this does not overturn:* everything D5 puts in `ext` is retrieval/parameter/policy metadata; the decisions that matter (effects → V-EFF, tests → `validates`, delegation → `delegated-to`, budgets → `Loop.bound`) are kernel fields and edges. The HIR/2 promotion list is filed so the block is a staging area, not a permanent home, and its size is reported per definition.

**Against model-free procedure validity: a procedure can pass its tests on one model and mislead another.** True — and it is why validity is *not* the whole story: validity is model-free by ADR-0014; compliance (`activated`/`followed`) is measured per profile; the realized-benefit decomposition is the reporting shape. A procedure "valid but unfollowed on profile P" is the expected finding, not a contradiction.

**Against retrieval-order strictness (validity → authority → readers → relevance): it starves the model of induced procedures, which are `≤ delegate` and often `unknown`-validity.** *Why this does not overturn:* D1 slots may declare `admit_unknown_validity`; induced procedures are deliverable into the `transcript`/`external` slots at their honest class; what the order forbids is silently delivering an untested, model-authored procedure as if it were sealed guidance — the S-103 skill-elevation vector in memory form.

**Negative result:** no audited system carries a validation test, an expiry condition, a typed effect declaration, or a provenance record on a skill; the Agent Skills spec's only extensibility is a string→string `metadata` map. Export to the file tree is therefore necessarily lossy (T-LCD-11 declared-lossy), and the industry's "trusted sources" advice (S-039) is the whole of its supply-chain model.

## 6. Recommendation for the spec

### 6.1 The Procedure/1 profile (HIR/1 `ext` block) and the HIR/2 promotion list — ADR WS-D5-adr-1

HIR/1 entity 5 is unchanged. D5 registers one extension block, `ext["<hh-prefix>/procedure"]` (prefix per OQ-068, WS-L6), with a closed schema `ProcedureProfile/1`:

```
ProcedureProfile/1 {
  index: { description: Text, retrieval_hints: [Text], examples: [Text], tags: [Text] }   // retrieval surface; profile renders it
  parameters: [ { name, type: TypedValueKind, required: bool, description: Text, default?: TypedValue } ]
  outputs?: { schema_ref: Ref<Artifact> }               // closed schema; enables `validator` endorsement → environment
  applicability_notes: [Text]                           // BASM-class boundary prose: conditions, risk cues, avoidance, recovery
  invocation_policy: { model: bool = true, principal: bool = true, implicit: bool = true }
  failure_classes: [FailureClass]                        // domain of failure_handlers[].on (closed, below)
  target_override?: { compile_hint, reason: Text }       // author intent; validated for feasibility, never assumed
  composition: [ { callee: Ref<Procedure>, binding: ArgBinding, inline_at_resolve: true } ]
  relations: [ { kind: ext-registered, to: Ref<Procedure> } ]   // Procedural-Graph relations; C2, vocabulary open
  source: { extension_ref?: VersionedRef, tree_path?: Text }    // for lifted skills (H5 ExtensionRecord)
}
```

Kernel fields get their D5 semantics fixed (no schema change):
- `preconditions: [Ref<Validator> | Predicate]` — `Predicate` is a closed sum of **checkable** predicates evaluated by the kernel from records: `path_glob(scope, pattern)`, `capability_present(Ref<ToolCapability>)`, `artifact_present(Ref<Artifact>)`, `env_requires(EnvironmentRequirement)`, `validator_passed(Ref<Validator>, within)`, `parameter_bound(name)`, `budget_remaining(dimension, ≥ n)`. Anything else is an applicability note. A false checkable precondition at delivery time is a D1 `Omission{reason: precondition}`; at execution time (`workflow_node`) it is `PreconditionFailed` and routes to `failure_handlers`.
- `expected_evidence: [Ref<Validator>]` — postconditions; for any Procedure whose derived `effects` are not all `read_only`, `expected_evidence ≠ ∅` or the procedure's validity is `unknown` and `PromotionPolicy` refuses promotion above `external`.
- `failure_handlers: [{on: FailureClass, then: Handler}]` with `FailureClass = {precondition_failed, step_failed(step), validator_failed(ref), budget_exhausted, permission_denied, effect_unknown, timeout, delegate_failed}` and `Handler = retry(bound: Ref<Budget>) | fallback(Ref<Procedure>) | compensate(Ref<Procedure>) | escalate | abort`. Handlers are total over `failure_classes` or `HandlerIncomplete` (error).
- `allowed_capabilities` — the closed set every `Invoke` must reference; for lifted skills it is populated only at `seal` from `declared_claims` (F7).
- `steps` — composition at HIR/1 is by **resolve-time inlining** (`composition[].inline_at_resolve = true`, recorded as `derived_from{kind: projection}`; call graph acyclic; recursion bounded by Budget). `Call(Ref<Procedure>, ArgBinding)` is proposed as an HIR/2 step kind.
- **Tests** need no new field: a procedure test is a `Validator` with a `validates` edge to the Procedure and `depends-on` edges to its fixture `Artifact`s (environment image digest + task text) and to the profile scope it was run under. `ProcedureTestSuite(P) = {v | validates(v → P)}`.

**Validity (model-free, ADR-0014):** `validity(P, at) = valid` iff every validator in `ProcedureTestSuite(P)` has a `kernel`-recorded pass on its pinned fixture within `P.validity` interval and no `depends-on` target is revoked/stale; `invalid` iff any has a fail; `unknown` otherwise. Retrieval applies it first (ADR-0034 P5).

**HIR/2 promotion list (filed, not decided):** `parameters`, `outputs`, `index`, `invocation_policy`, `FailureClass`/`Handler` sums, `Call` step kind, `ProcedureRelation` edge kind. Promotion criterion: a field is promoted when a kernel invariant or a Lab metric depends on it (today: none of the `ext` fields decides authority, budget or validity — by construction).

### 6.2 Three compilation targets and the selection rule — ADR WS-D5-adr-2

`compile_hint ∈ {instruction, workflow_node, subagent_task}` is a **surface** field (ADR-0016) written by the compiler at stage 2 from a total function; a profile may override it only through a `ProfileRule{kind: procedure_target}` (one new rule kind requested of ADR-0020/WS-C3) carrying an assumption-debt record (T-LCD-05).

Define three kernel-computed predicates on a sealed Procedure `P`:
- **B(P) bindable:** every `Invoke` has a complete `ArgBinding` from `parameters`, prior step outputs (by closed schema) or constants; every `Branch` predicate is a checkable `Predicate` or `Ref<Validator>`; every `Loop` is Budget-bounded; no `Instruction` step is load-bearing for control flow (instructions may only appear as the *body of a bounded model_call sub-step* whose output has a closed schema).
- **R(P) risk:** `max_by_danger` over `effective_risk_class` of `P.effects` (ADR-0031 projection).
- **I(P) isolation-needed:** `P` contains a `Delegate`, or `size(body) > profile.procedure_inline_budget`, or `P.ext.target_override = subagent_task`, or the definition's `context_policy` marks `P` `isolate`.

**Selection rule (total):**
1. If `target_override` is set: honour it if feasible, else `TargetInfeasible(P, hint, reason)` — an error, never silent fallback.
2. Else if `B(P)` and the definition binds a `control_strategy` variant declaring `workflow_execution`: **`workflow_node`**.
3. Else if `I(P)` and the definition binds subagents (F3) and the profile declares `subagents`: **`subagent_task`**.
4. Else **`instruction`**; if `size(body) > profile.procedure_inline_budget` and no subagent path exists: `UnexpressibleSurface(P, profile, exceeds_inline_budget)`.
5. **Risk floor (all targets):** if `R(P) ∋ irreversible ∨ scope = external`, then `expected_evidence ≠ ∅` is mandatory (`ProcedureUnverifiable` error) and every such `Invoke` passes Π at the proposer's effective authority (H1) — the target never changes the authority of an effect.

**What each target lowers to (ADR-0019 stages 2–3):**
- `instruction` → a `procedure_body` `ContextItem` (D1) rendered by the profile's `procedure_render` rule: index line + body `Text` leaves + a typed step listing (Invoke steps rendered as the profile's tool surface names through `SurfaceArgMap`; `Verify` steps rendered as "expected evidence" text). The Validators referenced by `Verify`/`expected_evidence` remain **kernel obligations** executed at `verify` regardless of what the model does. β for `plan/act` = model. `followed` detector: deterministic trace predicate (F2).
- `workflow_node` → `RuntimePlan` nodes (`step`, `branch-on-validator`, `loop`, `delegate`, `stop-rule`; closed set of `RuntimePlan/1`) with `hir_node_id` on every node; `Instruction` steps become bounded `model_call` sub-steps with a closed output schema. β for `act/verify` = code. `followed` = by construction; `activated` = plan entry event.
- `subagent_task` → a `Delegate` to an `AgentProcess.native` child: Goal = `P` (+ bound parameters), `Permission ⊆ grants over allowed_capabilities`, `Budget ≤ parent` (ADR-0016 invariants), fresh context (D1 isolation raises `context_label`), result re-enters at `≤ delegate` with `derived_from.kind = subagent_result` (ADR-0034 P2). Hosted participants receive it through `OpaqueProcess.supplies.procedures` and report `n/a` for component-level procedure metrics (T-LCD-15).

Feasibility errors (never warnings): `TargetInfeasible`, `UnexpressibleAsWorkflow(step, reason ∈ {unbound_arg, uncheckable_branch, unbounded_loop, opaque_without_interface})`, `DelegationUnavailable`, `ProcedureUnverifiable`. The chosen target and the predicate values are recorded in the bundle (`procedure_targets[]`) and on `context.artefact.delivered` so the Lab can vary target as a component-level factor (T-LCD-09).

### 6.3 Validation and test contract

- `validate_procedure(P, def) → ok | [Error]` (inside WS-A3 `validate`): V-EFF; `Invoke.capability ∈ allowed_capabilities`; `Loop` bounded; handlers total; parameters referenced by steps declared; `Predicate` kinds closed; `composition` acyclic; `RenderTimeExecution` (a `Text` leaf containing a declared exec marker, or an `Opaque` step in the index) refused; `PreconditionUncheckable` is *not* an error (it is an applicability note) but is counted.
- `test_procedure(P, suite, profile, target, budget) → ProcedureTestReport{per_validator: [{ref, verdict, run_id}], activated, followed, cost, outcome_class}` — runs as a Lab instrument run (`charged_to = instrument`, ADR-0039), under a `MatchSpec` (T-LCD-14); writes `ProcedureConformance{procedure_version_id, profile_semantic_id, target, validity_verdict, compliance{activated, followed}, run_id}`; validity is the model-free part, compliance the model-conditioned part (ADR-0014).
- `check_preconditions(P, records) → {satisfied: [Predicate], violated: [Predicate], unknown: [Ref<Validator>]}` — pure over ledger records; used by D1 admission and by `workflow_node` entry.
- Detectors (OQ-049, procedure kind): `activated` = D1 expansion / plan entry (deterministic); `followed` = `trace_predicate` validator "≥1 subsequent `Invoke` of an `allowed_capabilities` member, or a read of a bundled artifact, with `delivery_id ∈ causes[]`" (deterministic) for `instruction`; by construction for `workflow_node`; the child run's outcome class for `subagent_task`. Judged following (G3) is optional and labelled.

### 6.4 Lifecycle (D4/L4 semantics applied)

- **Origin classes:** `human(author)` sealed → `definition`; `import` (lifted skill) → `external` unless `pin` (H5 §6.3); `model`/`evolution` (induced from trajectories — AWM/Voyager/Procedural-Graph class) → `≤ delegate`, taint ∋ `{model}`, `derived-from{hypothesis, trajectories[], candidate_id}` mandatory (ADR-0017).
- **States:** `proposed → tested → sealed | pinned | promoted → active → expiring → superseded | revoked`. `promotion` (ADR-0035 basis) to `principal`/`definition` requires a human reviewer *and* `PromotionPolicy{require_validity = valid, require_transfer_test?: [profile_scope]}` — the AFTER transfer finding made a gate (multi-profile test before promotion is C2/I5).
- **Supersession/revocation:** ADR-0037 verbatim — a new version with `supersedes{reason ∈ {edit, revocation, expiry, migration}}`; revocation without replacement is a `RevocationRecord`; `StaleIndex` marks procedures whose `depends-on` (a `ToolCapability` version, an extension record, a fixture) is revoked as `stale-by-dependency`; `resolve(mode = execute)` and retrieval exclude them.
- **Expiry:** `validity.until | condition` on the node; a profile-conditioned `target_override` or `procedure_target` rule expires through its assumption-debt record (T-LCD-05); a procedure whose tests were last run under a retired profile version becomes `unknown`-validity, not `invalid`.
- **Migration ladder rung (ADR-0024):** prose skill body = `Text` leaf (rung 2); typed Procedure with checkable predicates/parameters = entity (rung 3); a bindable procedure lowered to `workflow_node` and given contract tests = candidate variant (rung 4). Each promotion is a paired, matched-budget experiment that lowers the definition's opacity ratio.

### 6.5 Agent Skills file trees — `lift_skill` / `export(skill_tree)` — ADR WS-D5-adr-3

`lift_skill(ExtensionRecord{kind: skill}) → {Procedure, Text[], Artifact[], CompiledPayload[], LoweringLossReport}`:

| SKILL.md / tree | HIR | note |
|---|---|---|
| `name` (must match directory) | name-history entry (ADR-0037); model-facing name = surface | identity = `semantic_id`; rename = `SurfaceEdit` (T-LCD-10); `NameCollision` at `resolve`, never last-wins |
| `description` | `index.description` `Text` (`external`; part of the pinned content) | selection-attack surface (S-WS-H5-04); hashed in `version_id` |
| body | one `Instruction(Text)` step (C0); section-level steps C2 | opacity bucket `procedure_instruction` |
| `allowed-tools` | `declared_claims` → `allowed_capabilities` + `authorizes` only at `seal` | claims are not grants (H5 L3) |
| `compatibility` | applicability note + optional `env_requires` predicate | prose → checkable only if the author types it |
| `metadata.version` etc. | `version_label` claim (CF-087); other keys preserved in `ext` | never identity |
| `license` | provenance attestation field | — |
| `scripts/*` | `CompiledPayload{format_tag: script/<tag>, declared_interface{inputs: argv, outputs: stdout schema?, effects: {exec}}}` bound to `Opaque` steps; run in `subprocess_confined` (H5 §6.5) as `exec` effects | a script with no declared interface is `OpaqueWithoutInterface` |
| `references/*`, `assets/*` | `Artifact`s (content-addressed), handle-only candidates read via `fs_read` scoped to the extension content | never a workspace widening (F7) |
| OpenHands `trigger.path` / `inputs` / `mcp_tools`; goose `arguments`; codex `dependencies`, `policy` | `path_glob` predicate / `parameters` / `depends-on` extension refs / `invocation_policy` | vendor extensions lifted where typed |
| inline `` !`cmd` `` | refused: `RenderTimeExecution`; author converts to a `filling` slot bound to a capability | F6 |

`export(P, target = skill_tree)` is an ADR-0021-style target: schema-conformant tree; minimum carried set travels only as `metadata` string pairs (`hh/semantic_id`, `hh/bundle_id`, `hh/effects`, `hh/permission_class`, `hh/provenance`, `hh/budget_ref`) — the map is string→string, so the export is admitted at `configuration` granularity at best and every typed field (`preconditions`, `expected_evidence`, tests, handlers, `outputs`) is a `no_slot` loss entry; `allowed_capabilities` → `allowed-tools` (claim). Round-trip `lift(export(P)) ∪ loss = P` is the T-LCD-11 test. MCP `Prompt{arguments}` is the export of a parameterised `instruction`-target procedure; A2A `AgentSkill{id, tags, examples}` is the export of a `subagent_task`-target procedure's index.

### 6.6 Relation to hooks (input to WS-F1, OQ-H5-04)

A hook is a **trigger** (`ControlBoundary.guard` at a `DecisionPoint`, or a `HarnessRule` with a decision-point trigger); its **action** is a Procedure with `compile_hint = workflow_node` (kernel-executed, ledgered effects, `subprocess_confined`). Guard outputs are `{narrow: deny | ask | none, additional_context?: ContextItem at ≤ definition, replacement_proposal?: Effect intent}`; a replacement proposal (the `updatedInput` power) re-enters `authorize` as a new proposal at the guard's authority; `allow` is never a guard output (it would be endorsement by a non-listed basis, ADR-0035). This keeps H5's narrow-only rule and lets every hook body be tested, versioned and revoked as a procedure.

### 6.7 Retrieval like a skill

Procedures enter the D3 hierarchy as one tier with the retrieval order of ADR-0034 P5 applied *before* any selector runs. `ProcedureSelector` is a component class: `select(query: ContextView, catalog: [ProcedureIndex], profile, accounting, k) → Selection{method, ranked[], truncated}`; invariants: deterministic given inputs; reads only index surfaces and records; cannot read bodies; emits `context.procedure.selected`. The C0 default is the "index-all-under-budget" selector (name + description of every active procedure that passes admission, capped by the `procedures` slot `budget_share` — codex `max_context_tokens`); C1 variants are lexical/n-gram/LRU (codex precedent) and D3 retrieval-augmented; a model-driven selector is a `Delegate` and its choices are `delegate`-authority observations.

### 6.8 Interface-contract sketch (operations · inputs → outputs · invariants · failure modes)

- `validate_procedure(P, def) → ok | [ValidationError]` — see §6.3; errors `EffectUncovered, HandlerIncomplete, UnboundParameter, RenderTimeExecution, CompositionCycle, OpaqueWithoutInterface, ProcedureUnverifiable`.
- `select_target(P, profile, def) → {target, predicates{B, R, I}, rule_id?} | TargetInfeasible | UnexpressibleAsWorkflow | DelegationUnavailable` — total; pure; recorded in the bundle.
- `lower_procedure(P, target, profile) → ContextItem | PlanNode[] | DelegateSpec` (inside ADR-0019 stages 2–3) — every output carries `hir_node_id`; `trace(bundle, locator)` total over procedure renderings.
- `check_preconditions(P, records) → {satisfied, violated, unknown}` — pure over records.
- `test_procedure(P, suite, profile, target, budget) → ProcedureTestReport` — instrument-charged; refuses without `MatchSpec` (`UnbudgetedArm`).
- `lift_skill(ExtensionRecord) → {nodes, leaves, payloads, loss_report}` / `export(P, skill_tree | mcp_prompt | a2a_skill) → (artefact, LoweringLossReport)` — round-trip invariant per T-LCD-11.
- `induce(trajectories[], hypothesis, budget) → HirDiff` (WS-I5 owns the pipeline; D5 fixes the output shape: a `Procedure` version with `derived-from`, `origin = evolution`, validity `unknown`, target unset).
- `promote(P, reviewer, policy) → EndorsementEvent | PromotionRefused{reason ∈ {validity, transfer, authority}}`.
- `ProcedureSelector.select(...)` — §6.7.

### 6.9 Data-model sketch

`ProcedureProfile/1` (§6.1) · `Predicate` (closed sum) · `FailureClass`, `Handler` (closed sums) · `ProcedureTestReport{procedure: VersionedRef, profile, target, per_validator[], activated, followed, cost, outcome_class}` · `ProcedureConformance{procedure_version_id, profile_semantic_id, target, validity_verdict, compliance, run_id}` · `Selection{method, ranked[], truncated}` · bundle member `procedure_targets[]: {hir_node_id, target, predicates, rule_id?}` · ledger events `context.procedure.selected{method, candidates[], delivered[]}`, `verification.procedure.tested{report_ref}`; `context.artefact.delivered/activated` and `verification.artefact.followed` reused with `artefact.kind = procedure` and `target`.

### 6.10 Acceptance-criteria sketch

- **AC-D5-1 (C0/Stage 2).** `lift_skill` on a spec-conformant skill tree yields a valid HIR/1 document: one Procedure, one `Instruction` step, `index` in `ext`, scripts as `CompiledPayload`s with interfaces, bundled files as `Artifact`s, `text_authority = external`, `allowed_capabilities = ∅` until `seal`; opacity report shows the body under `procedure_instruction`.
- **AC-D5-2 (C0/Stage 2, T-LCD-13).** Delivering the index emits `context.artefact.delivered{kind: procedure}`; expansion emits a deterministic `activated`; a subsequent tool call naming an allowed capability with the `delivery_id` in `causes[]` emits a deterministic `followed`.
- **AC-D5-3 (C0/Stage 2).** A skill body containing a render-time exec marker fails `validate` with `RenderTimeExecution`; a skill whose `allowed-tools` names a capability no in-scope Permission grants seals only after the author's acceptance produces the `authorizes` edge, else `EffectUncovered`.
- **AC-D5-4 (C1/Stage 3, T-LCD-01/-04).** The same Procedure compiles to `instruction` under two profiles with diffs ⊆ profile-owned fields, and to `workflow_node` under a definition whose control strategy declares `workflow_execution`; permission/effect/budget tables are identical across the three bundles.
- **AC-D5-5 (C1/Stage 3).** `select_target` is total on a corpus of ≥50 procedures; every non-`instruction` choice is explained by `{B, I}`; every refusal is one of the four typed errors.
- **AC-D5-6 (C1/Stage 3, T-LCD-14).** `test_procedure` refuses an arm without `MatchSpec`; a valid procedure whose fixture's tool version is revoked reports `stale-by-dependency` and validity `unknown`, never `valid`.
- **AC-D5-7 (C1/Stage 3, T-LCD-11).** `lift(export(P, skill_tree)) ∪ loss_report = P` on the corpus; loss report lists at least preconditions, expected_evidence, tests, handlers, outputs as `no_slot`.
- **AC-D5-8 (C2/Stage 4).** `subagent_task` lowering produces a child with `Permission ⊆` and `Budget ≤` parent, fresh context label, and a result re-entering at `≤ delegate`.
- **AC-D5-9 (C2/Stage 6).** An induced Procedure (`origin = evolution`) cannot be promoted without a human `promotion` event *and* `validity = valid`; the attempt is `PromotionRefused` and ledgered.
- **AC-D5-10 (T-LCD-09).** The Lab runs "target ∈ {instruction, workflow_node} × profile ∈ {P1, P2}" as a two-factor design and reports the interaction.

### 6.11 Build-stage suggestion and dependencies

- **C0 / Stage 2** (a stage note on R-2.4.5, as CF-045/CF-H5-05 did): `ProcedureProfile/1` schema in `ext`; `lift_skill` for the C0 mapping; `instruction` target; `procedure_index`/`procedure_body` candidates and the C0 selector; deterministic `activated`/`followed` detectors; `validate_procedure`; `RenderTimeExecution`. Rationale: doc 2 §11 Stage 2 ("skills") and the HIR/1 note that Procedure/Memory are populated at Stage 2.
- **C1 / Stage 3–4:** `workflow_node` target and `B(P)`; parameters/outputs; `test_procedure` and `ProcedureConformance`; export to skill tree / MCP prompt; lexical selectors; promotion policy.
- **C2 / Stage 4–6:** `subagent_task` (needs F3), induction (I5), transfer tests, section-level bodies, `relations`, HIR/2 promotion.

Dependencies: consumes ADR-0016 (entity 5), ADR-0019/0020 (stages, rule kinds), ADR-0024/0025 (ladder, resolve/seal), ADR-0033/0034/0035 (authority, retrieval order, endorsements), ADR-0037 (supersession), WS-H5 (extension record), WS-D1 (candidates, slots). Produces inputs for WS-D3 (procedure tier), WS-F1 (hooks position), WS-F3 (delegate spec), WS-G1 (trace-predicate detector), WS-I5 (`induce` output shape), WS-C3 (`procedure_target` rule kind, `procedure_render`, `procedure_inline_budget`), WS-E2 (composite surfaces as procedures, OQ-066), WS-K3/E4 (MCP prompt export).

### 6.12 T-LCD statement and ADR-0024 rung

Satisfies T-LCD-01 (no model identity in the Procedure; target overrides live in profile rules), T-LCD-02 (bodies are `Text` leaves; `procedure_instruction` bucket), T-LCD-05 (`procedure_target` rules carry debt records), T-LCD-08 (`ProcedureSelector` receives profile + accounting), T-LCD-09 (target is a factor), T-LCD-10 (name/description are surfaces; `NameCollision` instead of last-wins), T-LCD-11 (skill-tree/MCP/A2A exports carry a loss report), T-LCD-12 (operations only), T-LCD-13 (deterministic detectors per target), T-LCD-14 (`test_procedure` refuses unbudgeted arms), T-LCD-15 (hosted `supplies.procedures` → `n/a` at component level). Does not by itself satisfy T-LCD-03/-04 — executable at Stage 3 via AC-D5-4 and the equivalence run. Rung: the design *is* the ladder's middle rungs (2→3→4) applied to procedural knowledge.

## 7. Open questions raised

Temporary ids in `WS-D5.additions.md`: OQ-WS-D5-01 (relation vocabulary for `relations` — Procedural-Graph triplets — closed at HIR/2 or ext-open forever; evidence provisional), OQ-WS-D5-02 (`procedure_inline_budget` and `procedure_render` as profile rule kinds vs parameters of `prompt_layout` — WS-C3), OQ-WS-D5-03 (should `workflow_node` be the default when `B(P)` holds, or should the default be `instruction` with `workflow_node` opt-in until Stage-3 interaction data exists — Lab/F1), OQ-WS-D5-04 (transfer-test scope before promotion: how many profiles/environments; who pays — I5/I2), OQ-WS-D5-05 (composition by inlining changes the caller's `semantic_id` on every callee edit — acceptable at C1, or does the Lab need call-by-reference identity earlier — L4/A3), OQ-WS-D5-06 (skill-tree export prefix keys inside `metadata` — WS-L6 with OQ-068). Answered here for neighbours: OQ-023 (yes — §6.1), OQ-H5-07 (yes — `parameters`/`outputs` + `validator` endorsement), OQ-049 procedure kind (§6.3 detectors), CF-038 (ext + HIR/2 list), OQ-066 partial (a composite surface's mapping is a `workflow_node`-target Procedure; if `B(P)` fails it is inadmissible at C0).

## 8. Conflicts flagged

CF-WS-D5-01 (ADR-0020 closed rule-kind enum vs the requested `procedure_target` kind), CF-WS-D5-02 (gemini/codex "activation widens scope / renders verified paths at a privileged role" vs ADR-0033/H5 — rejected precedent), CF-WS-D5-03 (OpenHands render-time exec vs ADR-0019 purity — rejected), CF-WS-D5-04 (scope register R-2.4.5 C2/Could vs the C0/Stage-2 slice doc 2 §11 places at Stage 2 — stage note requested, no tier change), CF-WS-D5-05 (last-wins name shadowing across locations vs ADR-0037 identity — `NameCollision`), CF-WS-D5-06 (WS-A3 §6.8 maps `Procedure(workflow_node) → Flow`; D5 says only `B(P)` procedures are `workflow_node`, so Agent Spec export of non-bindable procedures is `product`-granularity prose — consistent, recorded).

## 9. Ontology terms proposed

`procedure profile`, `skill` (packaging of a Procedure as an extension of kind `skill`), `checkable precondition` vs `applicability note`, `bindable (B)`, `procedure target` (+ the three values), `procedure validity`, `procedure test suite`, `procedure conformance`, `invocation policy`, `render purity / RenderTimeExecution`, `procedure selector`, `induced procedure`, `promotion policy`, `inline_at_resolve`. Table in `WS-D5.additions.md`.

## 10. Confidence

**High** for the lift mapping, the index/body split, deterministic activation/following detectors, render purity and claims-not-grants: every element is read in source at pinned commits in ≥3 systems and follows ratified ADRs. **Medium** for the three-target selection rule: the targets are precedented separately (instructions everywhere; workflow-as-code in AFlow/ADAS; delegation in F3's precedents) but the *total rule* is novel and the "deterministic when bindable" default is a design choice to be tested at Stage 3 (OQ-WS-D5-03). **Medium** for the `ext`-then-HIR/2 route (depends on WS-L6's prefix and synthesis's appetite for a dialect bump). **Low-to-medium** for anything resting on S-095, S-WS-D5-01…04 — all `provisional`, used only as motivation for the precondition split, transfer gate and selector factor, never as a C0 basis.

## 11. ADRs produced

- `research/decisions/proposed/WS-D5-adr-1.md` — Procedure/1 profile in `ext`, checkable preconditions vs applicability notes, tests via `validates`, model-free validity, composition by inlining, HIR/2 promotion list (resolves OQ-023, OQ-H5-07, CF-038).
- `research/decisions/proposed/WS-D5-adr-2.md` — three compilation targets, total selection rule, risk floor, feasibility errors, `procedure_target` rule kind, per-target compliance detectors, target as a Lab factor.
- `research/decisions/proposed/WS-D5-adr-3.md` — skills as lifted procedures: file-tree mapping both ways with loss report, render purity, claims → grants at seal, lifecycle/promotion policy, hooks-as-triggers position, C0/Stage-2 stage note on R-2.4.5.

## 12. Status

`dossier-written`
