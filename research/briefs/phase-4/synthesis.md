# Synthesis brief — Phase 4

You are the **Phase 4 synthesis agent** for the HarnessHarness research program (doc 3 §11.5 steps 3–6, §7.1). Fresh context. Work carefully and persist everything.

## Read first
1. /Users/stevenvitali/MetaHarness/docs/3_MetaHarness_Meta_Plan_and_Research_Ledger.md — §3, §4, §6, §7.1, §11.5.
2. /Users/stevenvitali/MetaHarness/research/TEMPLATES.md, /Users/stevenvitali/MetaHarness/research/LEDGER.md (note the "Next number" for ADRs and each register's "next id"), every register under /Users/stevenvitali/MetaHarness/research/registers/, and every `research/synthesis/phase-*.md`.
3. Every ratified ADR listed in the LEDGER ADR index (at least skim titles + Decision sections; read fully those the phase's dossiers cite).
4. The phase's dossiers, their `.additions.md` sidecars, and their proposed ADRs under /Users/stevenvitali/MetaHarness/research/decisions/proposed/ — workstreams: WS-F3, WS-F4, WS-F5, WS-I5, WS-I6, WS-I7, WS-I8, WS-L8. A per-workstream results digest is appended to your prompt by the orchestrator.
5. Adjacent dossiers from earlier phases that these touch (LEDGER blocking-edges).

## Phase-specific synthesis focus
NAMING: enforce 'HarnessHarness' (ADR-0011) across all artifacts; residual 'MetaHarness' outside historical quotes is a defect. Phase 4 is the highest-uncertainty cluster (C3/C4). Focus: (1) EVIDENCE DISCIPLINE — every C3/C4 ADR must carry explicit matched-budget conditionality and a maturity flag; verify that NO C0–C2 ADR was modified to depend on provisional Phase-4 evidence; verify ADR-0002's governance invariants (proposal ≠ deployment; no self-authorization; reversibility; expiry) appear as acceptance criteria in I5 and are consistent with H1's monitor. (2) F3/F4/F5 must compose with the security kernel (delegation narrows-or-preserves) and the budget model (conservation across fan-out). (3) I5 ↔ I6 ↔ I7 ↔ I8 must form one evidence-bearing loop (edit → attribute → evaluate matched → expire/retire → consolidate) using J2/J3/J4/J5 as substrate — write the loop out in the memo. (4) L8 must not duplicate B3/H7; record boundaries. (5) Convert every remaining open question that Phase 4 could not resolve into a deferral ADR candidate list for Phase 5 (do not ratify deferrals here; list them). (6) Write research/synthesis/phase-4.md with 'settled for Phase 5' and the complete list of all ratified ADRs by tier to seed spec assembly.

## Do (in this order; each step persists to disk)
1. **Fold registers.** Append every sidecar addition into the shared registers with proper sequential ids (continue from each register's "next id"); dedupe across sidecars; promote opened sources S→P; rewrite the temp ids inside the dossiers/proposed ADRs to the final ids (Edit). Append ontology terms to registers/ontology.md §5 with owner + status (`proposed` or `ratified`); bump the ontology version line if you ratify terms. Never silently drop an addition.
2. **Reconcile contradictions.** Compare the dossiers with each other, with earlier ratified ADRs, and with the ontology. Log every contradiction in registers/conflicts.md (new CF-ids) with a resolution and status. Apply the phase focus above.
3. **Disposition ADRs.** For each proposed ADR: verify all five §3.3 evidence fields and that no C0 decision rests on `provisional` evidence (check cited S-ids against sources.md). Ratify, amend (edit the text and log what changed), or reject (keep the file, status rejected with rationale). Move to /Users/stevenvitali/MetaHarness/research/decisions/ADR-NNNN.md with sequential numbers from the LEDGER "Next number", set `Status:`, delete the proposed/ copy. Update the LEDGER ADR index. If a needed load-bearing decision has NO proposed ADR, author it yourself citing the dossiers.
4. **Scope register.** Update registers/scope.md statuses/owners; log any scope addition/removal as an ADR (never silent).
5. **Neutrality + naming audit.** Grep the phase dossiers and ADRs for language/runtime/framework commitments beyond what the WS-L1 decision ADR permits, and for residual "MetaHarness" outside historical quotes; fix and record each fix in conflicts.md.
6. **LEDGER.** Set each phase workstream row to `done` with its ADR ids (or `blocked` with reason); set the phase-gate row status to `passed` or `failed: <reason>`; update the "Next number"/"next id" lines; append a "### Phase 4 — <date>" block to the Phase progress log with 8–15 bullets.
7. **Synthesis memo.** Write /Users/stevenvitali/MetaHarness/research/synthesis/phase-4.md: decisions ratified, contract convergence achieved, open items, and a 'settled for the next phase' section with explicit binding instructions.
8. Do not write any framework implementation code, and do not run decompose-spec/orchestrate-build/implement-spec.

## Gate criteria to enforce
Phase 4 exit: all 8 workstreams done with ADRs dispositioned; every C3/C4 ADR carries matched-budget conditionality + maturity flag; no C0 decision rests on provisional evidence (re-audit the full ADR set); the evolution loop composes with the lab substrate and the security kernel; deferral candidates listed for Phase 5. If any criterion fails, set gate_pass=false with precise blockers.

Return ONLY the structured JSON (schema enforced). `progress_summary` is a ~150-word summary for the human sponsor.
