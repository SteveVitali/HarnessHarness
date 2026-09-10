# Synthesis brief — Phase 3

You are the **Phase 3 synthesis agent** for the HarnessHarness research program (doc 3 §11.5 steps 3–6, §7.1). Fresh context. Work carefully and persist everything.

## Read first
1. /Users/stevenvitali/MetaHarness/docs/3_MetaHarness_Meta_Plan_and_Research_Ledger.md — §3, §4, §6, §7.1, §11.5.
2. /Users/stevenvitali/MetaHarness/research/TEMPLATES.md, /Users/stevenvitali/MetaHarness/research/LEDGER.md (note the "Next number" for ADRs and each register's "next id"), every register under /Users/stevenvitali/MetaHarness/research/registers/, and every `research/synthesis/phase-*.md`.
3. Every ratified ADR listed in the LEDGER ADR index (at least skim titles + Decision sections; read fully those the phase's dossiers cite).
4. The phase's dossiers, their `.additions.md` sidecars, and their proposed ADRs under /Users/stevenvitali/MetaHarness/research/decisions/proposed/ — workstreams: WS-J1, WS-J2, WS-J3, WS-J4, WS-J5, WS-J6, WS-K1, WS-K2, WS-K3, WS-K4, WS-L5. A per-workstream results digest is appended to your prompt by the orchestrator.
5. Adjacent dossiers from earlier phases that these touch (LEDGER blocking-edges).

## Phase-specific synthesis focus
NAMING: enforce 'HarnessHarness' (ADR-0011) across all artifacts; residual 'MetaHarness' outside historical quotes is a defect. Phase 3 builds the laboratory and surfaces on Phase 1–2 contracts. Focus: (1) OQ-004 — ratify WS-J6's ABI depth decision; verify the LCD-trap battery result and that hosting remains strictly secondary (ADR-0001); ensure the ABI verbs map cleanly onto E4's ACP mapping and onto B1/I1 events. (2) LAB CHAIN — J1 assembly ↔ J2 registry ↔ J3 experiments ↔ J5 results ↔ J4 analysis must compose; every experiment result is linked to a bundle (I3) and every score is class-labelled. (3) L5 plugin contract must be consistent with J2 registry semantics and H5 trust; the extension-DAG rule is stated as spec invariants. (4) SURFACES — K1/K2/K3 must all be clients of K4's embedding contract (no surface bypasses it); record any capability a surface needs that K4 lacks as a conflict and resolve. (5) Verify no surface or lab ADR commits to a language/UI technology. (6) Write research/synthesis/phase-3.md with 'settled for Phase 4' (the measurement + lab contracts that F3/F4/F5/I5–I8/L8 must use; the security kernel constraints from Phase 2 that the evolution service must respect).

## Do (in this order; each step persists to disk)
1. **Fold registers.** Append every sidecar addition into the shared registers with proper sequential ids (continue from each register's "next id"); dedupe across sidecars; promote opened sources S→P; rewrite the temp ids inside the dossiers/proposed ADRs to the final ids (Edit). Append ontology terms to registers/ontology.md §5 with owner + status (`proposed` or `ratified`); bump the ontology version line if you ratify terms. Never silently drop an addition.
2. **Reconcile contradictions.** Compare the dossiers with each other, with earlier ratified ADRs, and with the ontology. Log every contradiction in registers/conflicts.md (new CF-ids) with a resolution and status. Apply the phase focus above.
3. **Disposition ADRs.** For each proposed ADR: verify all five §3.3 evidence fields and that no C0 decision rests on `provisional` evidence (check cited S-ids against sources.md). Ratify, amend (edit the text and log what changed), or reject (keep the file, status rejected with rationale). Move to /Users/stevenvitali/MetaHarness/research/decisions/ADR-NNNN.md with sequential numbers from the LEDGER "Next number", set `Status:`, delete the proposed/ copy. Update the LEDGER ADR index. If a needed load-bearing decision has NO proposed ADR, author it yourself citing the dossiers.
4. **Scope register.** Update registers/scope.md statuses/owners; log any scope addition/removal as an ADR (never silent).
5. **Neutrality + naming audit.** Grep the phase dossiers and ADRs for language/runtime/framework commitments beyond what the WS-L1 decision ADR permits, and for residual "MetaHarness" outside historical quotes; fix and record each fix in conflicts.md.
6. **LEDGER.** Set each phase workstream row to `done` with its ADR ids (or `blocked` with reason); set the phase-gate row status to `passed` or `failed: <reason>`; update the "Next number"/"next id" lines; append a "### Phase 3 — <date>" block to the Phase progress log with 8–15 bullets.
7. **Synthesis memo.** Write /Users/stevenvitali/MetaHarness/research/synthesis/phase-3.md: decisions ratified, contract convergence achieved, open items, and a 'settled for the next phase' section with explicit binding instructions.
8. Do not write any framework implementation code, and do not run decompose-spec/orchestrate-build/implement-spec.

## Gate criteria to enforce
Phase 3 exit: all 11 workstreams done with ADRs dispositioned; OQ-004 resolved by a ratified ADR with LCD-battery result; lab chain composes end-to-end; L5 consistent with J2/H5; all surfaces build on K4; no technology commitments; conflicts dispositioned. If any criterion fails, set gate_pass=false with precise blockers.

Return ONLY the structured JSON (schema enforced). `progress_summary` is a ~150-word summary for the human sponsor.
