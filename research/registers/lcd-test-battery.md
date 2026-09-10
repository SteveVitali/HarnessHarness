# LCD-Trap Test Battery — T-LCD-01…15 (binding acceptance criteria)

**Ratified:** ADR-0007 (Phase 0 synthesis, 2026-09-09). **Author:** WS-L7 (dossier §6.3, lifted verbatim). **Ontology pointer:** `registers/ontology.md` §7.
**Purpose:** convert the program's stance against the lowest-common-denominator (LCD) trap — "stable semantic interface + model-conditioned compilation" (doc 3 §1.2; CF-001, CF-003; RK-02) — into checkable criteria that every abstraction spanning models, targets or participants must satisfy. The three known failure signatures are **feature intersection**, **untyped escape hatches**, and **silent loss on lowering** (S-154 AWT/WORA); the known antidote is **capability declaration + verified probing + typed extension slots + per-target compilation** (S-152 LSP, S-035 ACP, S-153 MCP).

**Who inherits it.** Phase 1: WS-A3 (IR), WS-A4 (compiler), WS-A5 (composition), WS-B1 (event taxonomy), WS-I1/I2 (telemetry, eval), WS-L4 (identity), WS-L5 preview. Phase 2: WS-C3 (Model Profile), WS-E2 (tool-interface compiler), WS-E4 (protocol targets), WS-D1/D2 (context, compaction), WS-F1 (control), WS-I6 (assumption-debt record). Phase 3: WS-J3/J4/J5 (experiment, comparison, results), WS-J6 (Hosting ABI), WS-K3, WS-L5. Any future ADR introducing an abstraction spanning models, targets or participants must state which T-LCD tests it satisfies and which it does not; a "does not" without a mitigating test is grounds for rejection at synthesis (ADR-0007 decision 5).

**Conventions.** "Profile" = Model Profile (WS-C3); "target" = protocol/runtime lowering target (WS-A4); "surface" = anything a model or external system actually sees. Vocabulary per Ontology v0.1 (`registers/ontology.md` §5–§6).

## The battery

| id | criterion (must hold) | owner | check method | failure signature |
|---|---|---|---|---|
| **T-LCD-01 Model-specific tool shape without escape hatch** | The IR can express one `ToolCapability` (e.g., *edit file*) whose compiled surfaces differ by profile (patch-format for one family, string-replacement for another — S-049; `apply_patch` freeform vs absent — codex `models.json`, S-107) with **zero** per-model branches in the IR entity and all divergence in profile-owned fields. | A3, C3, E2 | Compile the same IR entity under ≥2 profiles; diff must touch only fields the profile schema owns; a semantic-equivalence validator (same effect on the workspace) passes on both. | An `if model == X` in IR, or a `raw_schema_override` field that bypasses the profile. |
| **T-LCD-02 Opacity budget** | Every model-facing surface is typed; free text is admitted only as a typed `Text` leaf carrying provenance, owner and authority class. Each harness definition reports an **opacity ratio** (tokens of untyped text ÷ total compiled surface tokens; exact definition OQ-036) and the lab can ablate opaque leaves individually. | A3, D1, I2 | Static check on the IR schema; opacity ratio computed at compile time and stored in the bundle. | "System prompt: string" as an IR entity; behaviour that cannot be ablated because it is one blob. |
| **T-LCD-03 Behavioural round-trip of a reference harness** | A minimal reference harness (Stage 0 baseline, mini-SWE-agent-class; richer anchor per OQ-039) expressed in the IR and compiled to the native runtime reproduces the original's behaviour within statistical equivalence on the Stage 3 suite (paired runs, fixed model/seed/budget). | A3, A4, I2 | Paired comparison; pre-registered equivalence margin; report distributions (doc 2 §8). | Systematic loss on any scorecard dimension attributable to a construct the IR could not express. |
| **T-LCD-04 Two-targets, two-profiles compile** | One IR definition compiles to ≥2 materially different profiles and ≥2 targets (e.g., native function calling and MCP) preserving policy and task semantics, verified by executable validators. | A4, C3, E4 | Same validators pass on all four compilations; permission/effect/budget invariants identical across targets. | Passing only on the "home" profile; validators skipped on a target. |
| **T-LCD-05 Every conditioned rule expires** | Every profile-owned rule carries hypothesis, evidence, owner, expiry condition and removal test (assumption-debt record). The compiler refuses a rule with none, warns on expired rules, and the lab can run a "delete this rule" experiment on demand. (Applies reflexively: the profile compiler itself and the Hosting ABI carry an expiry and a removal test.) | C3, I6, A4 | Schema-level requirement; compiler diagnostics; a lab recipe "retire rule R". | Rules with `evidence: none`; "dead weight" discovered by inspection (S-047) instead of by test. |
| **T-LCD-06 Hosted participants never weaken native contracts** | Adding a hosted participant requires no change to any native (IR/runtime) contract. Hosting is removable: with hosting disabled the native acceptance suite is unchanged. No dependency edge points from IR entity definitions to the Hosting ABI. | J6, A3, L5 | Dependency-graph check in the spec's DAG; build with hosting tier absent. | An IR entity gaining a field "so that hosted participants can fill it". |
| **T-LCD-07 Declare, verify, stratify — never intersect** | The Hosting ABI defines participant capabilities as a declared record (capability declaration) verified by probes into a capability vector (interrupt, streaming, resume, context-supply, tool-supply, permission-elicitation, cost-accounting, model-override…); undeclared = UNKNOWN, not unsupported; features are never removed from the ABI because some participant lacks them; comparisons stratify by capability. | J6, J4 | ABI schema mirrors ACP/LSP capability negotiation (S-035, S-152) and Omnigent's declared-vs-probed layers (S-118 `designs/harness-capabilities-bench-seam.md`). | A single "common subset" feature list; a probe result silently coerced to "unsupported". |
| **T-LCD-08 Component contracts see the profile** | Every component-class contract (compaction, context builder, control strategy, validator, router…) receives the model profile and resource accounting as inputs, so model-conditioned variants are expressible inside the contract rather than around it. | A5, J2, D2, F1 | Contract signature review; a model-conditioned variant (e.g., compaction that differs by context-window reminder policy, as in codex `models.json`) is implementable without touching Core. | A variant that must read a global or patch Core to learn which model it serves. |
| **T-LCD-09 Interaction effects are representable** | The experiment/results data model represents model × harness × environment × budget × seed as explicit factors and can report interaction effects and per-configuration distributions, never a single "harness score" or "model score". | I2, J3, J4, J5 | Schema check; a canned analysis "does compaction variant V help model M more than model N?" is executable. | Results keyed by harness alone, or by model alone. |
| **T-LCD-10 Names are surfaces, identities are semantic** | The identity of a capability/procedure/rule is content-addressed semantics; the model-facing name/description is a versioned, profile-owned surface. Renaming a compiled tool never changes IR identity; it is recorded as a profile edit with its own experiment (doc 2 §9). | A3, C3, L4 | Identity derivation excludes surface fields; rename produces a profile diff, not an IR diff. | Tool identity == tool name string. |
| **T-LCD-11 Lowering is lossless-or-declared-lossy** | Compiling to a protocol target (MCP/A2A/ACP/provider tool API) carries risk, permission, effect, cost and provenance metadata through typed extension slots (`_meta` prefixed keys, S-153) and the compiler emits a **lowering loss report** for anything the target cannot carry; lifting back recovers everything not declared lost. | A4, E4, K3 | Round-trip lower→lift test per target; loss report is part of the bundle. | Silent dropping of permission or effect metadata at the MCP boundary. |
| **T-LCD-12 Contracts are operations, not inheritance** | No Core contract requires a component to subclass or import a framework object; every contract is specified as operations/inputs/outputs/invariants/failure modes (language-neutral). | A5, L5, L1 | Spec review; the contract can be implemented out-of-process. | "Implement interface X from package Y". |
| **T-LCD-13 Compliance is observable separately from validity** | The event taxonomy records, for every artefact delivered to a model, *delivered / activated / followed* events so the lab can measure harness compliance independently of harness validity (S-088 mechanism; ontology `validity vs compliance`). | B1, I1, I2, A2 | Event schema includes artefact-delivery events with artefact ids; a compliance metric is computable per profile. | Only "prompt sent" is logged. |
| **T-LCD-14 No un-budgeted comparison** | The experiment engine refuses to run or report a comparison whose arms lack an explicit, matched resource budget (tokens, calls, wall-clock, approvals); search-time and artefact benefit are separated in the results schema. | J3, J4, I5 | Engine-level precondition; results schema has `search_budget` and `eval_budget` fields per arm. | "Evolved harness after N attempts vs baseline with one" (S-083 critique). |
| **T-LCD-15 Class-scoped metrics** | Every scorecard metric declares which participant classes (and observability levels) it applies to; component-level metrics on hosted participants are reported as N/A, never as 0 or as a configuration-level proxy. | J4, I2, A2 | Metric registry has `MetricDeclaration{requires_observability, applies_to_classes}`; report generator enforces. | A hosted participant "scoring 0" on attribution quality. |

## Stage placement (ADR-0007 decision 2)

| class | tests | tier / stage | enforcement |
|---|---|---|---|
| schema / static | T-01, T-02, T-06, T-10, T-12 | C0 · Stage 1 | IR schema validation; spec DAG check |
| contract | T-05, T-08, T-09, T-14, T-15 | C0 · Stages 1–3 | compiler / engine preconditions |
| executable | T-03, T-04, T-11, T-13 | C0 · Stage 3 (evaluation-first) — the build ladder must place these **before** Stage 5 (profiles) and Stage 6 (evolution) | paired runs; lower→lift round-trips; compliance metrics |
| hosting | T-07 | C2 · Phase 3 design | Hosting ABI schema + probe suite |

Which tests become *automated gates* vs *review checklists* is OQ-037 (Phase 1 synthesis with WS-I2).

## The LCD gate contract (language-agnostic; WS-A4/J3 own the full versions)

- `lcd_report(harness_def, profiles[], targets[]) → LcdReport{opacity_ratio, per_profile_diff_fields[], lowering_loss[target][], conditioned_rules[{id, evidence?, expiry?, removal_test?}], identity_stability: bool, hosting_edges: []}` — invariants: `per_profile_diff_fields ⊆ profile_schema.owned_fields` (T-01); every conditioned rule has all evidence fields or the report is `FAIL` (T-05); `hosting_edges` is empty (T-06); `identity_stability` true under rename (T-10). Failure mode: `UnexpressibleSurface(entity, profile)` — a first-class **error**, never a warning (the LCD signal).
- `equivalence_run(reference_participant, native_harness_def, suite, budget, seeds[]) → EquivalenceReport` — paired runs; per-dimension distributions; verdict against a pre-registered margin (T-03/T-04); refuses unequal budgets (T-14).
- `capability_vector(participant) → {declared: map, probed: map, unknown: set}` — for hosted participants (T-07); `unknown` is never coerced.

Data model: `LcdReport` is stored in the reproducible harness bundle (WS-I3). `AssumptionDebtRecord{rule_id, hypothesis, evidence_refs[], owner, expiry_condition, removal_test_ref, status}` is a C0/Stage-1 schema constraint on every conditioned rule (manager WS-I6 is C4). `MetricDeclaration.applies_to_classes ⊆ {native, hosted}` and `requires_observability ⊆ {events, model_io, end_state, ledger}` (T-15).

## Readiness-report matrix (to be filled at Phase 5)

| test | owning subsystem section (Spec §7.2 #5) | acceptance-criterion line | build stage | status |
|---|---|---|---|---|
| T-LCD-01 | | | C0/S1 | not encoded |
| T-LCD-02 | | | C0/S1 | not encoded |
| T-LCD-03 | | | C0/S3 | not encoded |
| T-LCD-04 | | | C0/S3 | not encoded |
| T-LCD-05 | | | C0/S1–3 | not encoded |
| T-LCD-06 | | | C0/S1 | not encoded |
| T-LCD-07 | | | C2 | not encoded |
| T-LCD-08 | | | C0/S1–3 | not encoded |
| T-LCD-09 | | | C0/S1–3 | not encoded |
| T-LCD-10 | | | C0/S1 | not encoded |
| T-LCD-11 | | | C0/S3 | not encoded |
| T-LCD-12 | | | C0/S1 | not encoded |
| T-LCD-13 | | | C0/S3 | not encoded |
| T-LCD-14 | | | C0/S1–3 | not encoded |
| T-LCD-15 | | | C0/S1–3 | not encoded |

Phase 5 readiness cannot pass with any row marked "not encoded" (ADR-0007 consequences).

## Evidence summary (ADR-0007 §Evidence)

Mechanism: S-154 (AWT peer model limited to intersection; WORA decay), S-152 (LSP capabilities; unknown properties ignored; `experimental`; `$/`), S-035 (ACP v2: all capabilities OPTIONAL, omitted = UNSUPPORTED, `_meta`, `_` extension methods), S-153 (MCP prefixed `_meta`); model-conditioned surfaces exist and matter — S-049, S-047 (Tier B), S-062/S-072/S-088/S-148 (`provisional`, corroborating one mechanism); validity vs compliance separable — S-088 (`provisional (mech)`), doc 2 §1; matched-budget necessity — S-083, doc 2 §8. Source-code precedents: codex `codex-rs/models-manager/models.json` + `core/src/tools/spec_plan.rs` (T-01/T-08); OpenHands `context/prompts/sections/static.py` (T-10); omnigent `harness_capabilities.py` + `designs/harness-capabilities-bench-seam.md` (T-07); DSPy `adapters/base.py` (T-04, one plane); AutoGen `autogen_core/models/_model_client.py` `ModelInfo` (T-05 shape). The battery as a whole and `LcdReport`/`UnexpressibleSurface` are `no-precedent / novel`. No `provisional` performance number is load-bearing: the tests check expressibility, declaration and reporting, not performance.

## Phase 1 application record (ADR-0007 decision 5; Phase 1 synthesis, 2026-09-10)

Every Phase 1 ADR that introduces an abstraction spanning models, targets or participants carries a T-LCD statement; the synthesis checked each statement against the ADR's decision text and found no "does not" without a mitigating owner. Static/contract tests are satisfied *on paper* (schema and contract rules stated); executable tests (T-03/-04/-11/-13) are Stage 3 obligations whose operations and acceptance criteria are now specified.

| ADR | owner | satisfies (on paper) | executable at Stage 3 | verdict |
|---|---|---|---|---|
| ADR-0015 (A3 type discipline) | WS-A3 | T-01 (no model-identity field outside `ProfileRef`/`surface`; `ext` never decides authority/budget/validity), T-02 (two opaque leaf kinds; opacity ratio), T-06, T-10 (identity excludes surface), T-12 | T-03 via `equivalence_run` (AC-8) | **pass** |
| ADR-0016 (HIR/1 catalogue) | WS-A3 | T-01/-02/-06/-10/-12 per kind; supplies T-05 (`assumption_debt`), T-13 (`delivery_id`), T-14 (Budget on every Goal/AgentProcess), T-15 (`hosted` has no components) | T-03/-04 with WS-A4/I2 | **pass** |
| ADR-0017 (HirDiff) | WS-A3 | T-10 (rename ⇒ `SurfaceEdit` only), T-05 (conditioned-rule edits carry records) | — | **pass** |
| ADR-0018 (OpaqueProcess; Agent Spec) | WS-A3 | T-06, T-07 (tri-state, never coerced), T-15, T-11 (Agent Spec loss report) | T-11 round-trip (C2) | **pass** |
| ADR-0019 (compilation pipeline) | WS-A4 | T-01, T-02 (opacity report in bundle), T-05 (link refuses undocumented rules), T-10 (bundle identity content-derived), T-11 (loss report per target), T-12, T-13 (closed-world rule) | T-03/-04 (AC-7/AC-8) | **pass** |
| ADR-0020 (Model Profile contract) | WS-A4 | T-01 (owned-field diff), T-05 (refuse/warn/retire), T-08, T-10, T-13 (behavioural suite) | T-04 (two minimal profiles) | **pass** |
| ADR-0021 (protocol targets) | WS-A4 | T-04 (construction from one `RuntimePlan`), T-06, T-07-adjacent, T-11 (typed loss report + round trip), T-15 (granularity ceiling) | T-04/-11 (MCP export, Stage 3) | **pass** |
| ADR-0022 (equivalence E1–E7) | WS-A4 | T-01 (supplies the semantic-equivalence validator), T-04, T-10 (E7), T-15 (`n/a` never 0) | E4 fixture suite | **pass** |
| ADR-0023 (definition is data) | WS-A5 | T-08 (`required_inputs ⊇ {ModelProfile, ResourceAccount}`), T-12 (operation records; out-of-process binding), T-02, T-06, T-10 | T-03 input | **pass** |
| ADR-0024 (code-vs-config) | WS-A5 | T-02 (ladder metric), T-01 (per-model branches are defects), T-05, T-14 (`authority_cap`), T-10 | — | **pass** |
| ADR-0025 (assembly validation) | WS-A5 | T-10 (AC-6), T-12, T-08, T-02, T-06 (`hosting_edges = []`), T-05, T-14 (budget placeholders) | — | **pass** |

Other Phase 1 ADRs (WS-A2, B1, B2, L2, L3, L4, I1, I2) carry T-LCD statements checked the same way; all pass on paper. OQ-037 (gate vs checklist) is resolved by ADR-0045: T-02/-09/-14/-15 and the `n/a`/outcome-class rules are automated gates; T-03/-13 are executable tests with review items. The readiness-report matrix above is filled at Phase 5.
