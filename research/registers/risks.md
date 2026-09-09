# Program Risk Register

**Seeded:** 2026-09-09 (Preflight) from doc 3 §9. Owner = the workstream/synthesis pass accountable for the mitigation. Status: `open` / `mitigated` / `realized` / `closed`.

| id | risk | likelihood | impact | mitigation (doc 3 §9) | owner | status |
|---|---|---|---|---|---|---|
| RK-01 | **Integration risk of a single full-vision spec** (doc 2's core caution: maximal scaffolding) | high | high | Mandatory C0 Core / C1–C4 extension layering + staged build ladder *inside* the spec (§1.3); each subsystem behind a stable contract before its rich variant | Phase 5 synthesis; WS-A5, WS-L5 | open |
| RK-02 | **Lowest-common-denominator abstraction** in IR / ABI | high | high | "Stable semantic interface + model-conditioned compilation" stance (§1.2); WS-L7 novelty bar; profile/tool compilers (WS-C3/E2) preserve model-specific behavior; thin ABI only for black-box participants (ADR-0001) | WS-A3, WS-A4, WS-J6, WS-L7 | open |
| RK-03 | **Over-trusting fresh 2026 preprints** | high | high | Provisional tagging in `sources.md`; mechanism-vs-performance rule; matched-budget skepticism (§3.1); no C0 decision rests on unreplicated performance claims | every WS; synthesis passes | open |
| RK-04 | **Scope creep / boil-the-ocean** | medium | high | MoSCoW in `scope.md`; core-first build ladder; evolution cluster in Phase 4; the gate is spec-readiness not software | orchestrator; `scope.md` | open |
| RK-05 | **Novelty/differentiation failure** in a crowded orchestration-library market | medium | high | WS-A1 competitive audit + WS-L7 thesis; novelty confined to comparison-first ontology/IR, the lab instrument, and the doc 2 §12 mechanisms | WS-A1, WS-L7 | open |
| RK-06 | **Fast-moving model/evidence landscape invalidating decisions** | high | medium | Versioned Source Registry; conditionality per ADR; assumption-debt discipline (WS-I6) applied to design decisions with a revalidation cadence | WS-I6; every ADR | open |
| RK-07 | **Cross-subsystem contradiction** (provenance vs security vs context) | medium | high | Cross-track synthesis passes (§7.1); shared Ontology register; adversarial coherence review at the gate (§7.3) | Phase 2 synthesis (L3/H1/D1), Phase 5 | open |

## Program-execution risks (added at Preflight by the orchestrator)

| id | risk | likelihood | impact | mitigation | owner | status |
|---|---|---|---|---|---|---|
| RK-08 | Research subagents cannot reach some sources (paywall, repo size, rate limits) and silently fabricate | medium | high | Research-agent contract requires listing sources actually opened; unread sources stay `S` in registry; synthesis spot-checks dossier citations against `sources.md` | orchestrator | open |
| RK-09 | Language leakage: dossiers/ADRs assume a language/runtime before WS-L1 ratifies | medium | medium | Agent briefs forbid language commitments; Phase 0/1 synthesis greps dossiers for language-specific commitments and flags them in `conflicts.md` | orchestrator; WS-L1 | open |
| RK-10 | Session interruption mid-phase loses in-flight agent work | medium | medium | Agents persist dossier before returning; `LEDGER.md` status per WS; git commit at every phase boundary; Workflow `resumeFromRunId` | orchestrator | open |
| RK-11 | Synthesis pass ratifies ADRs that lack the §3.3 five-part evidence (mechanism / precedent-or-novel / disconfirming / conditionality / build-stage) | medium | high | ADR template enforces the five fields; readiness report audits every ratified ADR for them | synthesis agents; Phase 5 | open |
