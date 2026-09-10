# ADR-NNNN — Semantic-equivalence obligations E1–E7 for compiled tool surfaces; composite and synthesized surfaces are C2 and require executable differential evidence

**Status:** proposed
**Owner workstream(s):** WS-A4 (obligations) → WS-E2 (tool-interface compiler), WS-E1 (capability metadata), WS-H1 (reference monitor), WS-C3 (error/result rendering), WS-I2 (equivalence run) · **Phase:** 1 · **Related:** ADR-0002 (evolution may edit surfaces; must not widen authority), ADR-0007 (T-LCD-01 "semantic-equivalence validator", T-LCD-04, T-LCD-14/-15 `n/a` rule), ADR-0008, ADR-0011; WS-A3-adr-1/-2 (`ToolCapability.effects`, `Permission.grants`, `SurfaceRecord`); WS-A4-adr-2 (`tool_shape` rules ship this evidence); CF-WS-A4-04; OQ-WS-A4-04; R-2.1.3, R-2.5.2, R-2.5.3

## Context

Doc 2 §5.2 says a tool-interface compiler should compile a semantic capability "into the tool shape a particular model is most likely to use correctly" and that "interface synthesis should be evaluated for semantic equivalence and safety, not just lower token count". T-LCD-01's check method requires "a semantic-equivalence validator (same effect on the workspace)" to pass on both compiled surfaces of one `ToolCapability`; T-LCD-04 requires "permission/effect/budget invariants identical across targets". The audit found that no system verifies this: goose `validate_tool_schemas`, pi `makeStrictJsonSchema` and opencode `sanitizeOpenAISchema` check *provider acceptability* of a schema; DSPy adapters check that outputs *parse*; HEART (`provisional`) synthesizes natural-language tool primitives over a 25,519-function catalogue with no equivalence or safety check in its abstract; Cursor evaluates whole-harness variants by keep-rate. Codex's `apply_patch` (freeform) and `unified_exec` are two surfaces of what HIR calls `edit_file` and `execute`, and nothing in code asserts they share an effect class. Meanwhile ADR-0002 lets the evolution service edit surfaces, so an unchecked surface is an authority-widening vector.

## Options considered

1. **No equivalence obligation; rely on end-to-end keep-rate / benchmark evaluation** (Cursor, HEART). *Rejected:* cannot localise a regression to a surface, cannot detect authority widening statically, and gives T-LCD-01/-04 no executable check.
2. **Full behavioural equivalence proof for every surface.** *Rejected:* undecidable in general; open-world capabilities have no comparable end-state; rendering (truncation, concision) is lossy by design.
3. **Transport validity only** (schema accepted by the provider). *Rejected:* the audited status quo; says nothing about effects, authority or observation adequacy.
4. **A graded obligation set — static checks that are always required (effects, argument-map totality/soundness, precondition-domain inclusion, error-class surjectivity, result adequacy, accounting identity) plus an executable differential check for closed-world capabilities; `n/a` with reason where a check cannot apply; composite/synthesized surfaces admitted only with a plan-shaped mapping and mandatory differential evidence, at C2 (chosen).**

## Decision

For a `ToolCapability` C lowered under profile P to surface S, stage 3 attaches `EquivalenceEvidence{E1..E7: pass | fail | n/a(reason)}`:

| # | obligation | preserved | level | who |
|---|---|---|---|---|
| E1 | **Effect equality** | `effects(S) = effects(C)` (WS-A3 `EffectClass` set + `idempotent`/`reversible`) for every reachable invocation | static, from the argument map | compiler, C0 |
| E2 | **Authority soundness and totality** | every S argument maps through `SurfaceArgMap{surface_field → capability_param, transform, narrowing?}`; no unmapped S argument; no mapped value can reach a C parameter outside its declared domain; the reference monitor evaluates `Permission` on C's parameters only | static + property test over the argument domain | compiler, C0 |
| E3 | **Precondition-domain preservation** | accepted inputs of S's schema, after mapping, ⊆ C's precondition domain; narrowing allowed and declared `narrowed`; widening is an error | static (schema inclusion) | compiler, C0 |
| E4 | **Differential end-state equivalence** (closed-world capabilities only) | a scripted sequence of C invocations executed through S and through the reference surface yields identical workspace end-state and identical `Effect` ledger records on a fixture suite | executable, Stage 3 | WS-E2 / WS-I2 |
| E5 | **Error-class surjectivity** | every failure class of C has a rendering in S's error format; renderings are pairwise distinguishable | static + rendering test | WS-C3 |
| E6 | **Result-observation adequacy** | S's result renderer preserves every field a bound `Validator` reads; truncation/concision declared `truncated` with retained fields listed | static against validator bindings | compiler + WS-G track |
| E7 | **Accounting and identity** | cost/latency/approvals attributed to C's `semantic_id` regardless of surface; `trace_map(S) ∋ C.semantic_id`; rename changes the profile hash only | static | compiler, C0 |

Rules: (i) C0 admits a surface only with E1–E3 and E7 `pass`; E4 `pass` is required at Stage 3 for `edit_file`, `execute` and `read_file`; open-world capabilities (network, `openWorldHint`) record E4 `n/a(open-world)` — never 0 or fail (the T-LCD-15 rule). (ii) **Composite or synthesized surfaces** — one surface ↔ many capabilities, wrappers with logic, code-mode (codex `unified_exec`, HEART-style primitives) — must express their mapping as a small HIR `Procedure` plan so E1–E3 remain checkable; if they cannot, they are inadmissible at C0 and belong to the C2 tool-interface compiler (WS-E2) with E4 mandatory. (iii) The evidence is part of the bundle and of `lcd_report`; a `tool_shape` `ProfileRule` (WS-A4-adr-2) ships the evidence for the surface family it selects. (iv) An evolution-proposed surface edit (ADR-0002) whose E2 result changes from `pass` is `AuthorityWidening`-class and rejected.

## Evidence (doc 3 §3.3 synthesis contract — all five mandatory)

- (a) Mechanism evidence: surfaces differ per model and are edited by optimizers (S-049 patch vs string replacement; S-107 `apply_patch`/`unified_exec` per slug; S-037 description refinements with "dramatic" effects; S-WS-A4-02 renaming; ADR-0002 evolution edits surfaces); the field ships synthesized interfaces without equivalence checks (S-076 HEART abstract, `provisional`; S-111/S-112/S-113 check provider acceptability only); provider strict-schema subsets narrow input domains (S-113 `makeStrictJsonSchema` rejects `$ref`, `oneOf`, tuples, non-false `additionalProperties`; S-111 `sanitizeOpenAISchema` silently rewrites) — the E3 mechanism; MCP annotations are hints, so effects must be asserted from the HIR side (S-153).
- (b) Source-code precedent: `pi-mono/packages/ai/src/api/constrained-sampling.ts` L10–90 (typed rejection of unsupported schema constructs; S-113 @400d690); `opencode/packages/opencode/src/provider/transform.ts` L1479–1600 (silent-narrowing negative example; S-111 @9f8db11); `goose/crates/goose-provider-types/src/formats/openai.rs` L959 `validate_tool_schemas` (S-112 @fae91d0); `codex-rs/core/src/tools/spec_plan.rs` L1006–1115 (`unified_exec` shell-parameter composition; S-107 @0735c51); `dspy/adapters/base.py` L83–135 (fields deleted when handled natively — an argument-map transform in the wild; S-144 @ca54a85). Each individual check has ordinary software-engineering precedent (schema inclusion, property tests, differential testing); the integrated obligation set attached to compiled surfaces is `no-precedent / novel` (no audited system, and HEART explicitly not).
- (c) Disconfirming evidence considered: (i) equivalence is undecidable and open-world tools have no end-state — answered by grading: static obligations always, executable E4 only for closed-world workspace capabilities, `n/a` elsewhere. (ii) Composite surfaces map one to many and code-mode lets the model write the mapping — answered by the plan-shaped-mapping rule and the C2 boundary (OQ-WS-A4-04 for the plan language). (iii) HEART and Cursor report gains without any such check, suggesting the obligations are unnecessary cost — answered: their evidence is unmatched-budget and whole-harness (S-083 discipline); the obligations are what make a surface regression attributable and an authority widening detectable, which ADR-0002's governance requires regardless of benchmark gains. (iv) Rendering loss (truncation, `concise`) is by construction non-equivalent — answered: E6 reports it against validator needs rather than proving it absent. None overturns.
- (d) Conditionality: E4 assumes a deterministic fixture environment (workspace + shell) exists at Stage 3 (WS-B5/E5); E5/E6 assume WS-C3 error/result renderers are data; the obligations are independent of model tier; no `provisional` result is load-bearing (HEART is cited only as the system that omits the check).
- (e) Build-stage assignment: **C0 / Stage 1** (schema for `SurfaceArgMap` and `EquivalenceEvidence`; E1–E3, E7 static checks); **C0 / Stage 3** (E4 fixture suite for `edit_file`/`execute`/`read_file`; E5/E6 with the first two profiles); **C2** (WS-E2 composite/synthesized surfaces, code-mode, E4 mandatory).

**T-LCD statement (ADR-0007 decision 5).** Satisfies: T-LCD-01 (supplies the "semantic-equivalence validator" the check method names), T-LCD-04 (policy invariants identical across targets is E1/E2 per target), T-LCD-10 (E7), T-LCD-15 (`n/a` never 0). Does not by itself satisfy T-LCD-03 (whole-harness round-trip) — that is WS-A4-adr-1/WS-A3 AC-8.

## Consequences

- WS-E2's C2 tool-interface compiler inherits E1–E7 as admission criteria; wrapper/composite synthesis must emit a `Procedure` plan.
- WS-H1's reference monitor evaluates `Permission` on capability parameters through `SurfaceArgMap`, never on surface fields.
- WS-E1's capability metadata must carry `preconditions` as a checkable domain, not prose, for E3 to run.
- WS-I5's evolution pipeline adds "E2 unchanged" to its security-invariant check.
- WS-I2 owns the E4 fixture suite and its budget accounting (T-LCD-14).

## Reversibility

**High for the executable checks** (E4 fixtures can be extended or narrowed by ADR); **medium for E1–E3/E7** (dropping them after Stage 1 removes the static guard ADR-0002's governance relies on and would require a compensating runtime check in WS-H1).
