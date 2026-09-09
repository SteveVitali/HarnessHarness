# MetaHarness — Program LEDGER

**Single source of truth for program state** (doc 3 §4, §11.8). A re-invoked session reads this file first and resumes at the first non-`done` workstream in the first non-`passed` phase.

- **Program contract:** `docs/3_MetaHarness_Meta_Plan_and_Research_Ledger.md` §11 (Operator's Runbook)
- **Terminal deliverables:** `spec/CANONICAL_SPEC.md`, `spec/READINESS_REPORT.md`
- **Hard stop:** Decompose-Readiness Gate (§7.3). No `decompose-spec` / `orchestrate-build` / `implement-spec`; no framework implementation code.
- **Binding constraints:** language/ecosystem-agnostic until WS-L1 ratifies (ADR-0009 procedure; audit every pass); `provisional` evidence never load-bearing for C0 (§3.1); every scope change logged as an ADR; **from Phase 0:** Ontology v0.1 vocabulary (`registers/ontology.md` §5–§6) and the LCD-trap battery (`registers/lcd-test-battery.md`, ADR-0007) are binding on every downstream dossier/ADR; the settled-for-Phase-1 list is `research/synthesis/phase-0.md`.
- **Git:** initialized 2026-09-09; commit at every phase boundary.

## Status legend

`todo` → `in-progress` → `dossier-written` (agent returned, not yet synthesized) → `done` (synthesis pass accepted; ADRs dispositioned) · `blocked` · `deferred(ADR-####)`

## Phase gates

| Phase | Contents | Status | Gate criteria (exit) | Commit |
|---|---|---|---|---|
| Preflight | §11.2 steps 1–6 | **passed** | tree exists; sources/ledger/registers seeded; ADR-0001/0002 ratified | preflight commit |
| 0 | WS-L1 (framing only), WS-A1, WS-L7; Ontology v0 → v0.1; Source Registry seeded + folded | **passed** (2026-09-09) | A1 + L7 dossiers done with ADR-0003/0004/0005 and ADR-0006/0007/0008 ratified; L1 criteria framed, not decided (ADR-0009 proposed); Ontology v0.1 in `registers/ontology.md`; Source Registry folded (S-132…S-166); CF-009 resolved; language-leak audit clean (CF-025) | pending (phase-boundary commit) |
| 1 | WS-A2 A3 A4 A5 · B1 B2 · L2 L3 L4 · I1 I2 | todo | all 11 done; shared authority/label scheme converged (L3↔A3↔I1); IR vocabulary arbitrated by A3; **WS-L1 language ADR ratified at end of Phase 1** | — |
| 2 | C1–C4 · D1–D5 · E1–E5 · F1 F2 · G1–G3 · H1–H7 · B3 B4 B5 · I3 I4 | todo | all 27 done; L3/H1/D1 authority scheme ratified together (§7.1); conflicts resolved in `conflicts.md` | — |
| 3 | J1–J6 · K1–K4 · L5 | todo | all 11 done; thin observational ABI depth decided (J6); plugin contract (L5) consistent with J2 | — |
| 4 | F3 F4 F5 · I5 I6 I7 I8 · organizational layer (WS-L8, see below) | todo | all 8 done; every C4 decision carries matched-budget conditionality; no C0 decision rests on provisional evidence | — |
| 5 | Synthesis & Canonical Spec | todo | §7.2 skeleton authored; adversarial coherence review clean; `READINESS_REPORT.md` all-green | — |

## Workstream table

Fields per §11.2 step 4: `{id, track, phase, status, blocking-edges, dossier-path, owner-agent}`. `blocking-edges` = workstreams that must be `done` first. `owner-agent` is filled with the subagent label when fanned out.

### Track A — Foundations, Ontology & IR

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-A1 | Positioning & prior-art/competitive audit | A | 0 | **done** | — | research/dossiers/WS-A1.md | WS-A1 research subagent (Fable 5.1) | ADR-0003 (ratified, amended), ADR-0004 (ratified; amends ADR-0001 (b)), ADR-0005 (ratified) |
| WS-A2 | Novel ontology: 7-plane model, policy-stack formalism, validity vs compliance, compatibility surface, participant classes | A | 1 | todo | A1, L7 | research/dossiers/WS-A2.md | — | — |
| WS-A3 | Harness IR design: typed entities/edges; behavioral not framework-specific | A | 1 | todo | A1 | research/dossiers/WS-A3.md | — | — |
| WS-A4 | Compilation model: IR → runtime + model profiles + protocol targets | A | 1 | todo | A1 | research/dossiers/WS-A4.md | — | — |
| WS-A5 | Configuration & composition model; code-vs-config boundary | A | 1 | todo | A1 | research/dossiers/WS-A5.md | — | — |

### Track B — Core Runtime & Durability

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-B1 | Event store / run ledger: taxonomy, ID model, materialized views | B | 1 | todo | A1 | research/dossiers/WS-B1.md | — | — |
| WS-B2 | Effect & transaction model: intent/prepared/commit/observed; idempotency; leases; compensation | B | 1 | todo | A1 | research/dossiers/WS-B2.md | — | — |
| WS-B3 | Durable execution & recovery | B | 2 | todo | B1, B2 | research/dossiers/WS-B3.md | — | — |
| WS-B4 | Reversible & speculative execution | B | 2 | todo | B1, B2 | research/dossiers/WS-B4.md | — | — |
| WS-B5 | Execution-environment abstraction | B | 2 | todo | B1, B2, L3 | research/dossiers/WS-B5.md | — | — |

### Track C — Model Plane

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-C1 | Model adapter/gateway | C | 2 | todo | A3, A4 | research/dossiers/WS-C1.md | — | — |
| WS-C2 | Model router | C | 2 | todo | A3, A4, L2 | research/dossiers/WS-C2.md | — | — |
| WS-C3 | Model-profile compiler | C | 2 | todo | A3, A4 | research/dossiers/WS-C3.md | — | — |
| WS-C4 | Caching & token economics | C | 2 | todo | L2 | research/dossiers/WS-C4.md | — | — |

### Track D — Context & Memory

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-D1 | Context builder / policy engine | D | 2 | todo | A3, L3, L2 | research/dossiers/WS-D1.md | — | — |
| WS-D2 | Compaction strategy family | D | 2 | todo | A3, L2 | research/dossiers/WS-D2.md | — | — |
| WS-D3 | Retrieval & memory hierarchy | D | 2 | todo | A3, L3, B1 | research/dossiers/WS-D3.md | — | — |
| WS-D4 | Memory lifecycle semantics | D | 2 | todo | A3, L3, L4 | research/dossiers/WS-D4.md | — | — |
| WS-D5 | Skills & Procedure IR | D | 2 | todo | A3, A4 | research/dossiers/WS-D5.md | — | — |

### Track E — Tools & Action

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-E1 | Tool registry & typed capabilities | E | 2 | todo | A3, L3 | research/dossiers/WS-E1.md | — | — |
| WS-E2 | Tool-interface compiler | E | 2 | todo | A3, A4 | research/dossiers/WS-E2.md | — | — |
| WS-E3 | Tool scaling: discovery, deferred loading | E | 2 | todo | A3 | research/dossiers/WS-E3.md | — | — |
| WS-E4 | Protocol edges: MCP, A2A, ACP | E | 2 | todo | A3, A4 | research/dossiers/WS-E4.md | — | — |
| WS-E5 | Sandboxed tool execution & effect capture | E | 2 | todo | B2, L3 | research/dossiers/WS-E5.md | — | — |

### Track F — Control & Orchestration

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-F1 | Control-strategy family | F | 2 | todo | A3, B1 | research/dossiers/WS-F1.md | — | — |
| WS-F2 | Control envelope | F | 2 | todo | B2, L2 | research/dossiers/WS-F2.md | — | — |
| WS-F3 | Sub-agent orchestrator | F | 4 | todo | B3, B5, H1, L2 | research/dossiers/WS-F3.md | — | — |
| WS-F4 | Value-of-compute scheduler | F | 4 | todo | L2, I2, F3 | research/dossiers/WS-F4.md | — | — |
| WS-F5 | Multi-agent coordination & consistency | F | 4 | todo | B1, B2, F3 | research/dossiers/WS-F5.md | — | — |

### Track G — Verification

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-G1 | Verification fabric | G | 2 | todo | A3, B1 | research/dossiers/WS-G1.md | — | — |
| WS-G2 | Execution-alignment / belief-state reconciliation | G | 2 | todo | B1, L3 | research/dossiers/WS-G2.md | — | — |
| WS-G3 | Independent critics/evaluators | G | 2 | todo | I2 | research/dossiers/WS-G3.md | — | — |

### Track H — Security & Governance

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-H1 | Reference-monitor kernel & capability model | H | 2 | todo | L3, A3 | research/dossiers/WS-H1.md | — | — |
| WS-H2 | Information-flow control & taint labeling | H | 2 | todo | L3 | research/dossiers/WS-H2.md | — | — |
| WS-H3 | Credential mediation & secret isolation | H | 2 | todo | L3 | research/dossiers/WS-H3.md | — | — |
| WS-H4 | Egress/network policy & sandbox boundaries | H | 2 | todo | B5 | research/dossiers/WS-H4.md | — | — |
| WS-H5 | Extension supply-chain trust | H | 2 | todo | L3, L4 | research/dossiers/WS-H5.md | — | — |
| WS-H6 | Deep audit trail & tamper-evident logging | H | 2 | todo | B1, I1 | research/dossiers/WS-H6.md | — | — |
| WS-H7 | HITL/approval & escalation policy | H | 2 | todo | L2 | research/dossiers/WS-H7.md | — | — |

### Track I — Measurement & Evolution

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-I1 | Telemetry/tracing + cost & latency instrumentation | I | 1 | todo | A1 | research/dossiers/WS-I1.md | — | — |
| WS-I2 | Eval framework: factorial design; 8-dim scorecard | I | 1 | todo | A1 | research/dossiers/WS-I2.md | — | — |
| WS-I3 | Reproducible harness bundle format | I | 2 | todo | L4, I2 | research/dossiers/WS-I3.md | — | — |
| WS-I4 | Benchmark/environment integration | I | 2 | todo | I2, B5 | research/dossiers/WS-I4.md | — | — |
| WS-I5 | Evolution service as governed pipeline | I | 4 | todo | I2, I3, H1, B4 | research/dossiers/WS-I5.md | — | ADR-0002 (pre-ratified scope) |
| WS-I6 | Assumption-debt manager | I | 4 | todo | C3, I2 | research/dossiers/WS-I6.md | — | — |
| WS-I7 | Causal attribution & counterfactual execution | I | 4 | todo | B4, I2 | research/dossiers/WS-I7.md | — | — |
| WS-I8 | Model-harness co-evolution & consolidation | I | 4 | todo | I5, I6 | research/dossiers/WS-I8.md | — | ADR-0002 (pre-ratified scope) |

### Track J — Meta-Harness Laboratory

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-J1 | Harness assembly & declarative definition | J | 3 | todo | A3, A5 | research/dossiers/WS-J1.md | — | — |
| WS-J2 | Component-variation registry | J | 3 | todo | A5, L4 | research/dossiers/WS-J2.md | — | — |
| WS-J3 | Experiment & sweep engine | J | 3 | todo | I2, L2, B5 | research/dossiers/WS-J3.md | — | — |
| WS-J4 | Comparison & analysis engine | J | 3 | todo | I2 | research/dossiers/WS-J4.md | — | — |
| WS-J5 | Results store, experiment ledger & leaderboard | J | 3 | todo | B1, I3, L4 | research/dossiers/WS-J5.md | — | — |
| WS-J6 | External-harness hosting / thin observational ABI | J | 3 | todo | A2, E4, I1, I2 | research/dossiers/WS-J6.md | — | ADR-0001 (pre-ratified positioning) |

### Track K — Surfaces

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-K1 | CLI design & command surface | K | 3 | todo | A5, J1 | research/dossiers/WS-K1.md | — | — |
| WS-K2 | Web dev-tooling | K | 3 | todo | B1, I1, J4 | research/dossiers/WS-K2.md | — | — |
| WS-K3 | MCP server exposing MetaHarness | K | 3 | todo | E4 | research/dossiers/WS-K3.md | — | — |
| WS-K4 | SDK/embedding API | K | 3 | todo | A4, A5 | research/dossiers/WS-K4.md | — | — |

### Track L — Cross-cutting & Program

| id | title | track | phase | status | blocking-edges | dossier | owner-agent | ADRs |
|---|---|---|---|---|---|---|---|---|
| WS-L1 | Language/ecosystem selection spike (framed P0, ratified end-P1) | L | 0→1 | **done (P0 framing)**; ratification pending end-P1 | (framing: —) (ratify: A3, A4, A5, B1 + questionnaire OQ-040…046 answered) | research/dossiers/WS-L1.md | WS-L1 research subagent (Fable 5.1) | ADR-0009 (**proposed — ratify at end of Phase 1**, not before) |
| WS-L2 | Resource-economics & accounting model | L | 1 | todo | A1 | research/dossiers/WS-L2.md | — | — |
| WS-L3 | Provenance & authority model | L | 1 | todo | A1 | research/dossiers/WS-L3.md | — | — |
| WS-L4 | Versioning, reproducibility & artifact identity | L | 1 | todo | A1 | research/dossiers/WS-L4.md | — | — |
| WS-L5 | Extensibility/plugin architecture & third-party component contracts | L | 3 | todo | A5, J2 | research/dossiers/WS-L5.md | — | — |
| WS-L6 | Packaging, licensing, OSS governance, docs & community | L | 5 | todo | L1 | research/dossiers/WS-L6.md | — | — |
| WS-L7 | Novelty/differentiation thesis & naming | L | 0 | **done** | — | research/dossiers/WS-L7.md | WS-L7 research subagent (Fable 5.1) | ADR-0006 (ratified, amended), ADR-0007 (ratified; `registers/lcd-test-battery.md`), ADR-0008 (ratified, amended) |
| WS-L8 | Human-agent organizational layer (§2.12, C4; Phase 4 "organizational layer" — no WS ID in §5, assigned here) | L | 4 | todo | F3, H7, B3 | research/dossiers/WS-L8.md | — | — |

**Count:** 52 workstreams (51 from §5 + WS-L8 assigned to the §6 Phase 4 "organizational layer" item, which §5 names but does not ID). WS-L6 is scheduled in Phase 5 (no §6 phase names it; it depends on WS-L1).

## ADR index

| ADR | Title | Status | Owner WS | Phase |
|---|---|---|---|---|
| ADR-0001 | Positioning: native-primary, hosting-as-participant; two participant classes | **ratified** (pre-ratified by sponsor) | WS-A1, WS-J6 | Preflight |
| ADR-0002 | Evolution / co-evolution fully in-scope, not stubbed | **ratified** (pre-ratified by sponsor) | WS-I5, WS-I8 | Preflight |
| ADR-0003 | Positioning: research instrument + reference runtime, not a general orchestration framework; non-goals (canonical list in ADR-0006); resolves CF-009 | **ratified** (amended: non-goal numbering unified, CF-019) | WS-A1 (WS-L7) | 0 |
| ADR-0004 | Amend ADR-0001 evidence (b): two-participant-class plane is precedented; novelty narrowed to white-box component axis + class-scoped metrics + declared/observed conformance | **ratified** (applied to ADR-0001) | WS-A1 | 0 |
| ADR-0005 | Interoperate, don't duplicate: interchange import/export; mechanism-neutral Hosting ABI (minimum observable event set + capability declaration); act as a participant in external planes | **ratified** | WS-A1 (J6, J5, I3, I4, B1, L2) | 0 |
| ADR-0006 | Differentiation thesis + canonical non-goals N1–N13 for Spec §1; narrowed, falsifiable novelty claims | **ratified** (amended: N1–N13; WS-A1 confirmation recorded) | WS-L7 (WS-A1) | 0 |
| ADR-0007 | LCD-trap test battery T-LCD-01…15 as binding acceptance criteria (`registers/lcd-test-battery.md`) | **ratified** | WS-L7 → A3/A4/A5/B1/I1/I2/C3/E2/E4/J3–J6/L5 | 0 |
| ADR-0008 | Canonical names: Harness IR (HIR), Harness Definition, native/hosted participant, Harness Lab, Hosting ABI, Model Profile/Profile Compiler, target/lowering/lifting, component class/variant, configuration/arm, class-scoped metric, capability declaration/vector; product name retained with disclosed collision | **ratified** (amended: effective at Ontology v0.1; CF-020/021 flagged to WS-A2) | WS-L7 (WS-A2, WS-L6) | 0 |
| ADR-0009 | WS-L1 language/ecosystem decision criteria, candidate classes E1–E5, ratification procedure (no language chosen) | **proposed — ratify at end of Phase 1** with the decision | WS-L1 | 0→1 |
| ADR-0010 | Scope change: R-2.10.6 hosting MoSCoW Could → Should; cross-participant comparison delivered in two steps | **ratified** (synthesis-authored) | synthesis (WS-A1, WS-L7 → WS-J6) | 0 |

*(Workstreams append `ADR-NNNN` rows with `status: proposed`; synthesis passes ratify/amend/reject. Next number: **ADR-0011**.)*

## Phase progress log

### Preflight — 2026-09-09
- Docs 1–3 read in full by the orchestrator.
- `git init`; initial commit of corpus.
- Created `research/{dossiers,decisions,registers}` and `spec/sections`.
- Seeded `registers/sources.md` (S-001…S-131) from doc 2 Sources + all footnotes + Tier 0–4 syllabus.
- Initialized this LEDGER (52 workstreams), `open-questions.md`, `ontology.md` (v0), `conflicts.md`, `scope.md` (§2 imported with MoSCoW), `risks.md` (§9 imported).
- Wrote ADR-0001 and ADR-0002 as `ratified`.

### Phase 0 — 2026-09-09
- **Gate: passed.** WS-A1 and WS-L7 `done` with three ratified ADRs each; WS-L1 framed only (ADR-0009 stays `proposed`, ratify at end of Phase 1); Ontology v0 → **v0.1**; Source Registry folded (35 new sources S-132…S-166, 31 rows promoted S→P; S-055 unreachable, substituted by S-107 source); CF-009 resolved; language-leak audit clean (CF-025). Synthesis memo: `research/synthesis/phase-0.md`.
- **Positioning settled (ADR-0003, ADR-0006):** MetaHarness is a *research instrument + reference runtime*, not a general orchestration framework; "framework" is reserved for what others build on top. One canonical non-goals list **N1–N13** (two dossiers' lists merged, CF-019).
- **ADR-0001's `novel` flag refuted and narrowed (ADR-0004; ADR-0001 (b) amended, decision unchanged):** the two-participant-class comparison plane is precedented (Harbor, HAL [ICLR 2026], Inspect AI, Omnigent, Harness-Bench). What is novel: the component-decomposable white-box axis over the Harness IR, `MetricDeclaration` class/observability scoping (`n/a` never 0), and declared-vs-observed conformance (DRIFT) as data.
- **Doc 3 §3.4 novelty claim (i) narrowed and made conditional (CF-011/CF-015):** "first IR spanning all seven planes with provenance on every entity and profile-compiled lowering" — reverts to "renamed" if WS-A3 does not deliver Permission/Effect/Validator/Budget/Memory-validity/HarnessRule entities with provenance, or WS-C3/E2 do not deliver testable, expirable profiles. Proven, not asserted, by T-LCD-03/-04 at Stage 3.
- **Hosting (ADR-0005, ADR-0010):** Hosting ABI = minimum observable event set + capability declaration, neutral across session-ABI / model-boundary interception / container-installed mechanisms (session-ABI first); native ledger + interchange import/export; MetaHarness must be able to act as a participant in Harbor/Inspect/Omnigent-style planes (C2/Stage 4). R-2.10.6 MoSCoW Could → **Should**; two-step delivery (native-vs-native Stage 3, hosted Stage 4). OQ-004 reframed.
- **LCD-trap battery ratified (ADR-0007):** T-LCD-01…15 are binding acceptance criteria, published as `registers/lcd-test-battery.md`; static tests C0/Stage 1, contract tests Stages 1–3, executable tests Stage 3 (before profiles/evolution), T-07 with hosting (C2). `lcd_report` / `UnexpressibleSurface` (error, never warning). Assumption-debt *record schema* is C0/Stage 1 (scope note on R-2.9.6).
- **Vocabulary ratified at Ontology v0.1 (ADR-0008):** Harness IR (HIR) / Harness Definition / native & hosted participant / Harness Lab / Hosting ABI (never "Harness ABI") / Model Profile & Profile Compiler / target, lowering, lifting, lowering loss report / component class & variant / configuration & arm / class-scoped metric / capability declaration & capability vector / LCD trap, escape hatch, surface vs semantic identity / hosting mechanism, observability level, comparison granularity, observed conformance & drift, trajectory interchange, reference runtime, research instrument. Flagged to WS-A2: CF-020 (declaration vs vector), CF-021 (`configuration` enum value); opacity ratio pending OQ-036. No sub-concept uses "meta-".
- **Product name:** "MetaHarness" retained with four disclosed collisions (Omnigent self-description, ruvnet, SuperagenticAI, Stanford Meta-Harness); qualifier "MetaHarness — harness laboratory & reference runtime" recommended; sponsor decides via OQ-035 (WS-L6, Phase 5).
- **Language decision (WS-L1):** framed, not decided. Criteria C1–C12 with weight ranges, hard gates (C2; C4/C6 conditional), candidate classes E1–E5 incl. polyglot split, blind-weight procedure with sensitivity sweep. **Phase 1 dossiers must answer the questionnaire language-neutrally:** OQ-040 (A3), OQ-041 (A3+B1), OQ-042 (B1), OQ-043 (A4), OQ-044 (A5), OQ-045 (B5/E5 provisional via synthesis), OQ-046 (A1/I2/K4). Ratification is postponed by rule if any answer is missing.
- **Contracts converged for Phase 1:** WS-B1 must stamp `participant_class` + `observability_level` on every ledger event and add artefact *delivered/activated/followed* events (T-LCD-13); WS-A3 identity derivation excludes surface fields (T-LCD-10), free text only as typed provenance-bearing leaves (T-LCD-02), no per-model branches (T-LCD-01), no dependency edge into the Hosting ABI (T-LCD-06); WS-A4 emits lowering loss reports and raises `UnexpressibleSurface`; WS-A5/L5 contracts are operations not inheritance and receive the profile (T-LCD-08/-12); WS-I2 models factors + interaction effects, refuses un-budgeted arms, carries `applies_to` (T-LCD-09/-14/-15); WS-K4/L5 treat WS-L1 §6.5 boundary contract as C0/Stage-1 input (CF-023).
- **Evidence discipline check:** every ratified ADR carries §3.3 (a)–(e); no C0 decision rests on `provisional` evidence — 2026 preprints (S-062/S-072/S-088/S-148/S-133/S-074) are used for mechanism only and corroborated by Tier-A (S-135) / Tier-B source at pinned commits.
- **Open for later phases:** OQ-030 interchange format (J6), OQ-031 Agent Spec export (A3/A4), OQ-033 cost attribution per mechanism (L2/J4), OQ-034 DRIFT exclude-vs-annotate (J6/I6), OQ-036 opacity-ratio definition (A3/I2), OQ-037 gates vs checklists (P1 synthesis), OQ-038 grey-box interposition (J6/C1), OQ-039 T-LCD-03 anchor harness (A3/I4), OQ-047 spike execution (P1 synthesis). CF-017 (runtime-assumption exposure in B3/K2/E5/K4) stays pre-registered for Phase 2.
- **Registers:** sources next S-167 · open-questions next OQ-048 · conflicts next CF-026 · ADR next ADR-0011.
- **Next phase (1):** fan out WS-A2 A3 A4 A5 · B1 B2 · L2 L3 L4 · I1 I2 with the "settled for Phase 1" section of `research/synthesis/phase-0.md` in every brief; end-of-Phase-1 synthesis runs the WS-L1 ratification (ADR-0009) only after A3/A4/A5/B1 ADRs are ratified and OQ-040…046 answered.
