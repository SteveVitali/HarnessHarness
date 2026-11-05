<!--
  docs/build/reports/round5/SPEC_RECONCILIATION_PLAN.md — reconcile-build mode=spec deliverable
  (BM-RECON-02), round-5 scope. Written by REC5.2 (2026-11-05). Append-only. The round-1–4 plan
  stays at docs/build/SPEC_RECONCILIATION_PLAN.md (REC.2, 2026-10-06). Amendments apply ONLY where
  ticked; a tick is an operator/sponsor act — re-run `reconcile-build mode=spec
  apply_amendments=true` after ticking, with a manifest `## Spec amendments applied` line + ADR.
  The supplement `docs/evidence-and-adoption-spec.md` is frozen ratified text: no spec_src/ exists
  in-repo, so an applied amendment edits the named file directly per the row's target field.
-->
# SPEC RECONCILIATION PLAN — round 5: folding shipped reality back into the contracts

**Inputs.** `TICKET_VS_SPEC.md` (this directory — 63 landed round-5 rows tagged) ·
`docs/build/planning/2026-10-30_amendment-dossier.md` (7 prepared §9.3 rows + S.1–S.6) ·
`docs/build/readouts/GATE-G{9,18,11,12,13,14,15,16,17}.md` · `docs/adr/` ADR-0382…0420 (39
round-5 ADRs) vs `spec/CANONICAL_SPEC.md` Appendix B (ratified ADR-0001…0216 + applied A-3
by-reference paragraph naming ADR-0217–ADR-0330) · `docs/tickets/DEFERRALS.md` (3 OPEN) ·
`docs/build/reports/round5/COVERAGE_MATRIX.csv` + CAP5.3 closure packet.

**Tick state.** Every amendment row below is `[ ]` — **proposed, not applied**
(`apply_amendments=false`). Nothing in this run edits `spec/` or `docs/evidence-and-adoption-spec.md`
or the manifest's `## Spec amendments applied` section. Requirement ids are append-only — a
fold-back gets a new id in its family, never a renumber; **no new requirement ids are proposed**.

## Amendment rows (proposed)

| # | scope | target file | anchor | before → after | tick |
|---|---|---|---|---|---|
| R5A-1 | Canonical Appendix-B ADR-index range — the frozen mismatch extended by the landed set | `spec/CANONICAL_SPEC.md` (Appendix B has no section-file source) | `## Appendix B — ADR index`, the applied-A-3 paragraph (line ~9781) | **Before:** "Build-phase ADRs **ADR-0217–ADR-0330** … indexed here by reference" — the applied 2026-10-06 paragraph pre-dates rounds 4.15e–5 (90 further ADRs). **After:** the same paragraph re-reads "Build-phase ADRs **ADR-0217–ADR-0420** (per-ticket deviation/ruling records authored by the implement-spec chain through round 5) live in `docs/adr/` with a generated index (`docs/adr/README.md`); they amend no ratified contract and are indexed here by reference; the Sections column does not apply." This is the recorded BM-ADR-04 mismatch — preserved as dispositioned legacy until ticked, never silently rewritten. | [ ] |
| R5A-2 | Dispositioned — §9.3 register admission state | **no amendment proposed** | `docs/evidence-and-adoption-spec.md` §9.3 register table | The register is a *proposal* register by design ("O2/Q-7 review of proposed scope is not an already-landed amendment"). Its seven rows' admission+landing record lives in the GATE-G9 readout, the dossier, and ADR-0385–0420 — annotating the frozen register would rewrite a ratified source to record state the records already carry. | n/a — record stands |
| R5A-3 | Dispositioned — the §9.3 existing-contract fixes (M1 origin, R0 mapping, selected CLI request fields) | **no amendment proposed** | §9.3 closing paragraph ("Existing-contract fixes, not automatic amendments") | M1 `created`→`finished` origin (ADR-0389) and R0 open/finished mapping (ADR-0402) landed as **repairs conforming code to the contract** — the clause's own shape; the spec text never claimed the defect, so no text needs folding. The selected-CLI-fields fix is part of the R5.28 forwarding conformance (ADR-0406). | n/a — discharged by landing |
| R5A-4 | Dispositioned — DF-R5.17-1 (exploratory-prior arm defect) | **no amendment needed** | `docs/tickets/DEFERRALS.md` DF-R5.17-1 | The independent-verification battery found a real semantic defect; it routed to the owning admission and was repaired at ADR-0418 D1 with a fails-without proof — the row is **DONE**. This is the reconcile machinery working as designed: a defect became a record, then an ADR, never a silent patch or a papered claim. | n/a — discharged by ADR |
| R5A-5 | Dispositioned — gate scope decisions (G13 N=0 parity; G14 quote-only→withheld confirmation; G15/G16 voided stages; G17 acceptance) | **no amendment proposed** | LEDGER GATE DECISIONS + `docs/build/readouts/` | Operator scope decisions are decisions, not spec text: the spec's honesty rules require the voided/withheld/failed predicates to stay visible, which the packets + DEFERRALS rows (DF-H9-1, DF-CAP5.1-1, DF-R5.1-1) already do. Folding the decisions into spec text would hide owed work inside the contract. | n/a — decisions, not text |
| R5A-6 | Dispositioned — the dispositioned-record vocabulary (`unselected{stage_voided}`, `withheld{stage_voided}`, `no_publication`, `no_observation`, `failed`, `inconclusive` packet schemas `hh-*-unselected/1`, `hh-*-withheld/1`) | **no amendment proposed** | §8 optional-exercise + honesty rules | The typed honest-record schemas are the spec's "optional exercises retain explicit unselected/withheld scope" and "unexecuted/unadmitted work is unmet" discipline made concrete — an implementation mechanism, like the class table or codecs before it. Below spec granularity; lives correctly as landed records. | n/a — implementation detail |
| R5A-7 | Dispositioned — the owed predicate set itself | **no amendment proposed** | `docs/tickets/DEFERRALS.md` DF-R5.1-1 / DF-H9-1 / DF-CAP5.1-1 | The three OPEN rows name owner/trigger/verification for every owed predicate (GATE-G17 contributor evidence; withheld participant set; failed S1 residual). Documentation cannot retroactively authorize or discharge them — they close on their own triggers, never by this plan. | n/a — owed work, not text |

## Fold-back id accounting

- **New requirement ids proposed: none.** R5A-1 amends appendix header prose only — it names the
  ADR set by reference and creates no requirement. Every `ticket-added` group in
  `TICKET_VS_SPEC.md` is build/evidence infrastructure (guard, driver, checkers, capstone tools,
  packets, run ledgers) — outside the product spec's scope; none earns a requirement id.
- Families touched if ticked: Appendix B header prose only (R5A-1). No §-numbered section,
  requirement row, or acceptance cell changes.

## ADR set vs spec Appendix B — parity result (round 5)

| set | membership | home |
|---|---|---|
| Spec Appendix B (ratified) | ADR-0001…ADR-0216 (216 rows, Sections column) | `research/decisions/ADR-*.md` |
| Spec Appendix B (by-reference paragraph, applied A-3 2026-10-06) | names ADR-0217–ADR-0330 | `spec/CANONICAL_SPEC.md` prose |
| `docs/adr/` (actual build set) | ADR-0217…ADR-0420 (204 files) + generated `README.md` | `docs/adr/` |

**Delta:** ADR-0331…ADR-0420 are landed but outside the applied paragraph's named range — the
BM-ADR-04 warning persists by design and is dispositioned by row R5A-1 (a ticked apply extends
the by-reference range; until then the frozen text is preserved, per this ticket's contract:
"preserve frozen appendix mismatch as explicitly dispositioned legacy, not a failed requirement
hidden by rewriting it").

## Reconciliation findings the operator may want

1. **The round is honest to a machine-checkable depth.** Every landed semantic traces to an
   admitted packet, a direct adopted requirement, a recorded fix, or a verbatim gate decision —
   V-1…V-15 in `TICKET_VS_SPEC.md`. No missing amendment, reverse edge, hand-edited generated
   artifact, unadmitted dependency, or silent deviation was found.
2. **The only owed spec text is R5A-1** (the appendix range), and it is optional by design —
   the generated `docs/adr/README.md` is the live index; the appendix paragraph is a pointer.
3. **Three predicates remain owed** (DF-R5.1-1, DF-H9-1, DF-CAP5.1-1) — all OPEN with
   owner/trigger/verification; none is satisfiable by documentation. The build is not DONE:
   layer verdicts stand at the CAP5.3 recompute (engineering `executed`; research/public/
   adoption `withheld`/`reported`), and this close adds no predicate.
