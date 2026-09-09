# Phase 0 synthesis memo — program setup (2026-09-09)

**Scope of this pass** (doc 3 §11.5 steps 3–6, §7.1): fold the WS-A1, WS-L7, WS-L1 sidecars into the shared registers; reconcile the three dossiers with each other, with ADR-0001/0002 and with the ontology; disposition the seven proposed ADRs; enforce the Phase 0 gate. **Gate result: passed.** Nothing here implements code; nothing here chooses a language.

## 1. Decisions ratified

| ADR | Decision (one line) | Disposition |
|---|---|---|
| ADR-0003 | MetaHarness is a **research instrument + reference runtime**, not a general orchestration framework; explicit non-goals; every ⊕ NEW / ↑ EXT scope item gets a `precedent \| renamed \| novel` annotation at Phase 5; WS-K4 = "embed the instrument/runtime". **Resolves CF-009.** | ratified, amended (non-goal numbering unified with ADR-0006, CF-019) |
| ADR-0004 | **ADR-0001's `novel` flag on the two-participant-class comparison plane is refuted**: the plane is precedented (Harbor S-132/S-133, HAL S-134/S-135 [ICLR 2026], Inspect AI S-136/S-137, Omnigent S-118, Harness-Bench S-062). Novelty narrowed to (i) the component-decomposable white-box axis over the Harness IR, (ii) `MetricDeclaration{requires_observability, applies_to_classes}` (`n/a`, never 0), (iii) declared-vs-observed conformance (DRIFT) as experimental data. ADR-0001 (b) amended; decision unchanged. | ratified; applied to ADR-0001 |
| ADR-0005 | **Interoperate, don't duplicate**: native typed ledger + interchange export/import (ATIF-compatible or documented mapping); Hosting ABI = *minimum observable event set + capability declaration*, neutral across session-ABI / model-boundary interception / container-installed (session-ABI first); MetaHarness must be able to *act as a participant* in external planes (C2/Stage 4, resolves OQ-032); WS-L2 defines per-mechanism cost attribution with provenance + confidence. | ratified |
| ADR-0006 | **Differentiation thesis** (WS-L7 §6.1 paragraph, verbatim into Spec §1); novelty confined to three narrowed, falsifiable statements; **canonical non-goals N1–N13**; thesis fails if T-LCD-03/-04 fail at Stage 3. | ratified, amended (N13 added; WS-A1 confirmation + Open Agent Spec recorded) |
| ADR-0007 | **LCD-trap test battery T-LCD-01…15 are binding acceptance criteria**, published as `research/registers/lcd-test-battery.md`; `lcd_report` / `UnexpressibleSurface` (first-class error); experiment engine refuses un-budgeted arms; metric registry carries `applies_to`; assumption-debt *record schema* is C0/Stage 1. | ratified |
| ADR-0008 | **Canonical names** (Ontology v0.1): Harness IR (HIR), Harness Definition, native/hosted participant, comparison plane, Harness Lab, Hosting ABI, Model Profile / Profile Compiler, target / lowering / lifting / lowering loss report, configuration / arm, component class / variant, conditioned rule / assumption-debt record, class-scoped metric, capability declaration / capability vector, LCD trap. Product name retained with disclosed collision. | ratified, amended (effective at v0.1 now; CF-020/021 flagged to WS-A2) |
| ADR-0009 | WS-L1 **decision criteria, candidate classes E1–E5, ratification procedure** — no language chosen. | **proposed — ratify at end of Phase 1** together with the decision; not before |
| ADR-0010 | **R-2.10.6 hosting MoSCoW Could → Should** (tier C2 unchanged; secondary per ADR-0001); Spec §1 states two-step delivery (native-vs-native Stage 3, hosted Stage 4). Resolves CF-014. | ratified (synthesis-authored scope change) |

ADR-0001: status still ratified; evidence (b) amended per ADR-0004 (and the unreachable S-055 citation replaced by S-107 source paths, CF-024); consequences bullet 1 extended with the v0.1 terms. ADR-0002: untouched; WS-A1 F8 corroborates its (b) "no integrated governed-pipeline precedent".

Every ratified ADR was checked for the five §3.3 fields and for `provisional` reliance: all five present; no C0 element rests on a 2026 preprint — the preprints (S-062, S-072, S-088, S-148, S-133, S-074, S-140) supply mechanism only and are corroborated by one Tier-A paper (S-135) and Tier-B source audited at pinned commits.

## 2. Contract convergence achieved (what the three dossiers agreed on)

- **Positioning and non-goals converged** independently (WS-A1 §6.1 ↔ WS-L7 §6.1/§6.2); the only divergence was numbering, fixed by one canonical list N1–N13 (CF-019).
- **Novelty narrowed identically** by both audits (CF-010/011 ↔ CF-015): the comparison plane is not the differentiator; the white-box participant class over a seven-plane, provenance-bearing, profile-compiled IR is — and only if WS-A3 and WS-C3/E2 deliver it (conditional, re-checked at Phase 1/2 synthesis).
- **Hosting boundary shape converged** across three angles: WS-A1's mechanism-neutral event set + capability declaration (ADR-0005), WS-L7's declare/verify/stratify + no upward edges (T-LCD-06/-07), WS-L1's ecosystem-boundary verb set (§6.5). Layering fixed in CF-023: session-ABI mechanism = the verb set; Hosting ABI = event set + declaration; ecosystem boundary = the same verb set + event vocabulary applied internally.
- **Vocabulary converged** except two flagged items (CF-020 capability declaration vs vector — both ratified with distinct meanings; CF-021 `configuration` as enum value vs factorial point — consistent, WS-A2 may rename the enum value).
- **Evidence posture converged**: all three dossiers rest recommendations on source audit at pinned commits and Tier-A/B specs; provisional numbers appear only as illustrations.

## 3. Register state after folding

- **Sources:** S-132…S-166 added (35 unique after de-duplicating the three sidecars; dedup map in `sources.md` synthesis note); 31 rows promoted S→P; S-055 remains `S` (HTTP 403, substituted by S-107 source). Next **S-167**.
- **Open questions:** OQ-030…047 added; OQ-004 reframed; OQ-032 resolved (ADR-0005). Next **OQ-048**.
- **Conflicts:** CF-010…025 added; CF-009 resolved. Open/accepted: CF-013 (name collision — sponsor), CF-017 (Phase 2 runtime-assumption exposure — pre-registered), CF-018 (S-074 count inconsistency — accepted), CF-011 (conditional on WS-A3/C3/E2). Next **CF-026**.
- **Ontology:** **v0.1** — §5 terms folded with owner + status; §6 canonical-name table; §7 pointer to the LCD battery. WS-L1's eight program-vocabulary terms are `proposed` (ratify with ADR-0009).
- **Scope:** R-2.10.6 → Should (ADR-0010); R-2.12.5 → specified; R-2.12.3 → framed; stage/novelty notes on R-2.1.1/2/3, R-2.3.3, R-2.9.2/4/6, R-2.10.4, R-2.11.4; scope-change log updated. 60 items, no additions or removals.
- **Risks:** RK-02 mitigation in place (battery), RK-05 mitigated-conditional, RK-09 Phase 0 clean.
- **Language-leak audit (RK-09):** clean — every language/runtime token in WS-A1/WS-L7/ADRs is a precedent path or repository name; WS-L1 names languages only as ecosystem-class exemplars and decides nothing (CF-025).

## 4. Open items carried forward

| id | item | owner | due |
|---|---|---|---|
| OQ-030 | Interchange format (ATIF vs documented superset); compaction/subagent markers | WS-J6 (B1, I3) | Phase 3 |
| OQ-031 | Harness IR ↔ Open Agent Spec export/import as an interop target; what is lost | WS-A3 / WS-A4 | Phase 1 |
| OQ-033 | Cost-attribution confidence per hosting mechanism; mixed-confidence Pareto rows | WS-L2, WS-J4 | Phase 1–3 |
| OQ-034 | DRIFT: exclude vs annotate; re-probe cadence | WS-J6, WS-I6 | Phase 3 |
| OQ-035 | Product-name qualifier or rename (four collisions) | sponsor via WS-L6 | Phase 5 |
| OQ-036 | Opacity-ratio definition (T-LCD-02) | WS-A3 with WS-I2 | Phase 1 |
| OQ-037 | Which T-LCD tests are automated gates vs review checklists | Phase 1 synthesis, WS-I2 | Phase 1 / 5 |
| OQ-038 | Grey-box model-call interposition as an optional hosted capability | WS-J6 with WS-C1 | Phase 3 |
| OQ-039 | Reference harness anchoring T-LCD-03 | WS-A3, WS-I4 | Phase 1 / 3 |
| OQ-040…046 | WS-L1 ratification questionnaire (see §5.6) | A3, B1, A4, A5, B5/E5, A1/I2/K4 | Phase 1 |
| OQ-047 | Spike execution and bias review for language ratification | Phase 1 synthesis | end Phase 1 |
| CF-011 | Novelty claim conditional on WS-A3 (seven-plane entities + provenance) and WS-C3/E2 (testable profiles) | Phase 1/2 synthesis | re-check |
| CF-017 | Runtime-assumption exposure in B3/K2/E5/K4 | Phase 2 synthesis | Phase 2 |

## 5. Settled for Phase 1 — binding on WS-A2, A3, A4, A5, B1, B2, L2, L3, L4, I1, I2

Treat the following as decided. Cite the ADR; do not re-argue it; if you must deviate, propose an ADR that supersedes.

### 5.1 Positioning (ADR-0003, ADR-0006)
- MetaHarness is a **research instrument and reference runtime for harness engineering**, not a general agent framework. The word "framework" never describes MetaHarness; it names what others build on top. Adoption pressure sits on the IR's external edges (ACP/MCP/interchange/export), never on the loop.
- The reference runtime is opinionated, minimal, event-sourced; its purpose is instrument validity (ablation/attribution), not hosting third-party core loops.
- The canonical non-goals are **N1–N13** (ADR-0006 decision 3). Any dossier that would widen scope into N1 (general framework), N2 (universal Harness ABI), N5 (new protocol), N6 (behavioural interchangeability) or N12 (language commitment) is out of bounds.

### 5.2 Novelty claims (ADR-0004, ADR-0006; CF-011/CF-015)
- Do **not** claim the two-participant-class comparison plane as novel; cite Harbor/HAL/Inspect/Omnigent.
- The three permitted novelty statements: (a) first IR spanning all seven planes with provenance/authority on every entity and profile-compiled lowering, designed for comparison; (b) a lab that varies one component under matched budget across native participants and admits hosted participants on the same plane without weakening native contracts; (c) the doc 2 §12 mechanisms as integrated contracts with provenance and expiry, each cited against partial prior art. Any ADR using "novel" names the prior art it is novel relative to (WS-A1 §4c/§6.2/§6.3 lists; WS-L7 §5).
- **WS-A3 is load-bearing for claim (a):** the entity set must include `Permission, Effect, Validator, Budget, Memory` (with validity/authority), `HarnessRule` (with assumption-debt fields) alongside `Goal, Observation, ContextItem, Procedure, ToolCapability, Artifact, AgentProcess`, every entity versioned with provenance, edges `depends-on, supersedes, authorizes, produced-by, validates, delegated-to`. Open Agent Spec (S-138/S-139) is the nearest prior art and stops at agent/tool/flow/LLM-config — answer OQ-031 (export/import to it, and what is lost).

### 5.3 LCD-trap battery (ADR-0007; `registers/lcd-test-battery.md`)
- Binding acceptance criteria. Phase 1 owners and tests: **WS-A3** T-01, T-02, T-03, T-06, T-10, T-12 (identity derivation excludes surface fields; free text only as typed `Text` leaves with provenance/owner/authority; no per-model branches; no dependency edge from IR entities into the Hosting ABI); **WS-A4** T-03, T-04, T-05, T-11 (`lcd_report`, lowering loss report, `UnexpressibleSurface` as *error*); **WS-A5** T-08, T-12 (every component-class contract receives the Model Profile and resource accounting; contracts are operations, never inheritance; implementable out-of-process); **WS-B1/WS-I1** T-13 (artefact *delivered/activated/followed* events with artefact ids); **WS-I2** T-02 (opacity ratio metric, OQ-036), T-09, T-14, T-15 (factors + interaction effects; `search_budget`/`eval_budget` per arm; `MetricDeclaration.applies_to`); **WS-L4** T-10 (content-addressed semantic identity). WS-A2 owns the vocabulary behind T-13/T-15.
- Each Phase 1 ADR that introduces an abstraction spanning models, targets or participants states which T-LCD tests it satisfies and which it does not (a "does not" without a mitigating test is grounds for rejection).
- Stage placement is fixed: static tests C0/Stage 1; contract tests Stages 1–3; executable tests Stage 3, before profiles (Stage 5) and evolution (Stage 6). WS-I2 and the Phase 1 synthesis decide gate-vs-checklist (OQ-037).

### 5.4 Vocabulary (ADR-0008; `registers/ontology.md` v0.1 §5–§6)
- Use the canonical names: **Harness IR (HIR)**, **Harness Definition**, **native participant** / **hosted participant** (expand to the ADR-0001 long forms on first use), **comparison plane**, **Harness Lab**, **Hosting ABI** (never "Harness ABI"; never "HTIR"), **Model Profile** / **Profile Compiler** ("model adapter" = gateway, WS-C1), **target / lowering / lifting / lowering loss report**, **configuration / arm**, **component class / component variant** ("plugin" = third-party packaging), **conditioned rule / assumption-debt record**, **class-scoped metric**, **capability declaration** (claimed record) / **capability vector** (derived declared-probed-unknown triple), **hosting mechanism**, **observability level** `{events, model_io, end_state, ledger}`, **comparison granularity** `{component, configuration, product}`, **observed conformance / DRIFT**, **trajectory interchange**, **LCD trap / escape hatch / opacity ratio / surface vs semantic identity**. No sub-concept name uses "meta-".
- **WS-A2** produces Ontology v1: confirm or collapse CF-020 (declaration vs vector), settle the CF-021 enum value, formalize the two participant classes with `comparison granularity` and `observability level` (ADR-0001 consequences as amended), define `validity vs compliance` events (T-LCD-13) and class-scoped metrics (T-LCD-15). Amend ratified terms only by ADR.

### 5.5 Hosting-related obligations that land in Phase 1 (ADR-0004, ADR-0005, ADR-0010)
- **WS-B1:** every ledger event carries `participant_class` and `observability_level`; the ledger is native and typed (never ATIF-shaped); a documented bidirectional mapping to an interchange trajectory is a later (I3/J6) obligation, but the ID model must make export/import rows identifiable by provenance.
- **WS-I2:** `MetricDeclaration{name, requires_observability, applies_to_classes}` is C0/Stage 1; inapplicable metric × class pairs render `n/a`, never 0; factors are model × harness (component/configuration/product) × environment × budget × seed.
- **WS-L2:** define cost attribution per hosting mechanism (measured model I/O; participant-reported usage; native-log reconstruction) with provenance and confidence (OQ-033).
- **WS-A4:** ACP, MCP, A2A and provider tool APIs are *lowering targets*; the compiler must be able to expose a native harness through an ACP-style session interface (ADR-0005 "act as a participant"); answer OQ-043 (interpreted data vs emitted code; protocol-target artefacts).
- Hosting is **Should**, C2, secondary; Phase 1 does not design it but must not make it impossible (T-LCD-06).

### 5.6 Inputs WS-L1 needs from Phase 1 (ADR-0009 proposed; registered as OQ-040…046)
Answer **inside your dossier, language-neutrally** (name no ecosystem). The language ratification is postponed by rule if any answer is missing.
- **WS-A3:** OQ-040 — closed sum types with exhaustiveness checking required, or open schema-validated records sufficient? Effect/capability typing *checked* at IR validation or only *recorded* for the reference monitor? Structural vs textual diffs; IR ↔ source round-trip? (flips hard gate C6). OQ-041 (with WS-B1) — canonical deterministic byte encoding required for content addressing and tamper evidence, and which class? Also OQ-036 (opacity ratio) and OQ-039 (T-LCD-03 anchor harness).
- **WS-B1:** OQ-042 — is the event log authoritative with derived views (OQ-003)? Write-path consistency (single writer, leases, fencing)? Storage class at Stage 0–1? Deterministic control-loop replay or event reconstruction only? Plus OQ-041 jointly with WS-A3.
- **WS-A4:** OQ-043 — are compiled control structures runtime-interpreted data or emitted host-language code? Must the compiler emit MCP schemas / A2A AgentCards / ACP session config?
- **WS-A5:** OQ-044 — is a Harness Definition a data document any runtime can load, or does composition require host-language code? Must component variants load in-process or may they be out-of-process participants?
- **WS-I2:** OQ-046 (Q-L1-13) — external scorers/judges/loaders called in-process per sample, or per run over a boundary?
- **Phase 1 synthesis:** OQ-045 provisional answer (sandbox helper in-process vs helper binary — consult the doc 2 §5/H-track evidence; final answer is WS-B5/E5 in Phase 2); OQ-046 Q-L1-11 tally from S-166 + WS-A1 §3; OQ-047 spike plan; then run ADR-0009's procedure only after the A3/A4/A5/B1 ADRs are ratified.
- **WS-K4/WS-L5 (later):** treat WS-L1 §6.5 (boundary contract: `hello/open_session/submit/stream_events/request_permission/cancel/account/close`; single source of truth + generated bindings + CI check; version handshake; exact pinning; byte-identical events; no authority widening) as C0/Stage-1 input (CF-023).

### 5.7 Evidence and neutrality rules restated
- Tag every 2026 single-team/preprint result `provisional`; use it for mechanism only; corroborate any C0 claim with Tier-A/B evidence or an independent re-derivation. Cite S-ids from `registers/sources.md` (next S-167); the Phase 0 audit corpus (S-132…S-166, promoted rows) is available and already opened.
- No language, runtime, package-ecosystem or framework commitment anywhere (RK-09; ADR-0009 AC8). Precedent file paths are fine; "we will use X" is a leak and will be logged in `conflicts.md`.
- Every proposed ADR carries §3.3 (a)–(e); ADRs ratify only in synthesis passes; the ontology register is the vocabulary tie-breaker and WS-A3 arbitrates typed IR vocabulary.
- Sidecar convention stands: write new S/OQ/CF/ontology rows to `dossiers/WS-XX.additions.md` with temp ids; synthesis renumbers.

## 6. Gate check (doc 3 §6; LEDGER)

| criterion | status |
|---|---|
| WS-A1 and WS-L7 dossiers done with ≥1 ADR each dispositioned | pass — 3 + 3 ratified |
| WS-L1 criteria framed but NOT decided; ADR remains proposed | pass — ADR-0009 proposed, "ratify at end of Phase 1" |
| Ontology v0 → v0.1 recorded | pass — `registers/ontology.md` |
| Source Registry folded | pass — S-132…S-166; promotions; dedup map |
| No language commitments except WS-L1's candidate enumeration | pass — CF-025 |
| CF-009 dispositioned | pass — resolved (ADR-0003, ADR-0006) |

**Phase 0: passed.** Commit at the phase boundary per §11.8.
