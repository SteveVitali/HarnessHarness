# Conflicts Register

**Seeded:** 2026-09-09 (Preflight). Records contradictions between sources, between workstream dossiers/ADRs, or between a dossier and the ontology — and how each was resolved. Synthesis passes (doc 3 §7.1, §11.5 step 4) own resolution. The Ontology register is the tie-breaker for vocabulary; WS-A3 arbitrates typed IR vocabulary.

**Columns:** `id` · parties (sources / WS / ADRs) · the contradiction · resolution · resolved-by (synthesis pass / ADR) · status (`open` / `resolved` / `accepted-tension`).

## Known tensions carried in from docs 2–3 (pre-registered so synthesis must address them)

| id | parties | contradiction | resolution | resolved-by | status |
|---|---|---|---|---|---|
| CF-001 | doc 3 §1.2 (generic vs performant) — WS-A3/A4 vs WS-C3/E2 | Maximal IR genericity hides model-conditioned features that drive performance (LCD trap) | Stance: stable semantic interface + model-conditioned compilation; to be validated by A3/A4 and the profile/tool compilers | Phase 1 synthesis (stance) → Phase 2 (validation) | open |
| CF-002 | doc 3 §1.2 (full-vision vs rigor) — sponsor vs doc 2 §11 | Doc 2 recommends a small rigorous core first; sponsor chose one full-vision spec | Full-vision spec with internal C0/C1–C4 layering + staged build ladder (doc 3 §1.3) | pre-resolved (doc 3); enforced at gate | accepted-tension |
| CF-003 | ADR-0001 — WS-J6 vs WS-A3 | Hosting external harnesses through a common ABI risks the LCD trap that the IR avoids | Two participant classes; thin *observational* ABI only for black-box participants; deep interop only on white-box path | ADR-0001 (ratified) | resolved |
| CF-004 | Evolution literature (S-061/063/080/081/071) vs counter-evidence (S-083/075/091/092) | "Evolution beats baseline" vs "not under matched budget; weak transfer" | Matched-budget skepticism (§3.1); evolution fully in scope (ADR-0002) but every C4 decision carries matched-budget conditionality and separates search-time benefit from artifact benefit | ADR-0002 + WS-I5 | resolved-in-principle; WS-I5 must apply |
| CF-005 | Deterministic-control pole (S-065, S-070) vs model-owned control (S-006, S-032) | Where should control live? | Pluggable control-strategy family (WS-F1) + control envelope of hard invariants (WS-F2); boundary is a lab comparison target, not a fixed decision | WS-F1/F2 (Phase 2) | open |
| CF-006 | Provenance (WS-L3) × security kernel (WS-H1) × context builder (WS-D1) × IFC (WS-H2) | Each may propose its own authority/label/taint scheme | Must converge on ONE authority/label scheme before any of their ADRs ratifies (doc 3 §7.1, §11.5.4) | Phase 2 synthesis | open (pre-registered) |
| CF-007 | Event sourcing "selectively not dogmatically" (doc 2 §11) — WS-B1 vs WS-H6 vs WS-J5 | Authoritative append-only log vs materialized views vs content-addressed artifacts; audit wants everything, context wants compact views | WS-B1 decides authoritative-log scope (OQ-003); H6 and J5 consume it | Phase 1 synthesis (B1) → Phase 2 (H6) → Phase 3 (J5) | open |
| CF-008 | Memory research (S-016/017 retrieval-centric) vs lifecycle research (S-077, S-106) | Similarity retrieval vs validity/authority-first retrieval | Retrieval filters by authority/validity before relevance (doc 2 §5.1); D3 hierarchy must adopt D4 lifecycle semantics | Phase 2 synthesis (D3/D4/L3) | open |
| CF-009 | Doc 2 §7 source-code study (no vector embeddings, hand-rolled loops in 11 coding harnesses) vs generic-framework aspirations | Evidence that mature harnesses avoid general frameworks for their core loop cuts against "framework" positioning | WS-A1 must address: MetaHarness is an instrument/reference runtime, not a general orchestration framework (doc 3 §1.1 thesis) | WS-A1 / WS-L7 (Phase 0) | open |

## Workstream-raised conflicts (append-only; next id **CF-010**)

| id | parties | contradiction | resolution | resolved-by | status |
|---|---|---|---|---|---|
