<!--
  docs/build/SPEC_RECONCILIATION_PLAN.md — reconcile-build mode=spec deliverable (BM-RECON-02).
  Written by REC.2 (2026-10-06). Append-only. Amendments apply ONLY where ticked; a tick is an
  operator/sponsor act — re-run `reconcile-build mode=spec apply_amendments=true` after ticking.
  This spec is not generated in-repo: there is no spec_src/ or BUILD.sh, so an applied amendment
  edits `spec/sections/<file>` AND the inlined copy inside `spec/CANONICAL_SPEC.md`, keeping the
  two verbatim (the assembled document inlines the section files).
-->
# SPEC RECONCILIATION PLAN — folding shipped reality back into `spec/CANONICAL_SPEC.md`

**Inputs.** `TICKET_VS_SPEC.md` (98 landed rows tagged) · `docs/build/BACKLOG.csv` BL-30 (the
four REC.2-scoped ruling rows) · `docs/build/COVERAGE_MATRIX.csv` (six `MET-DIFFERENTLY` rows) ·
`docs/build/readouts/GATE-ACCEPT.md` (10 signed deviation rows) · `docs/adr/` (ADR-0217–0330) vs
spec Appendix B (ADR-0001–0216) · `research/registers/{open-questions,conflicts}.md`.

**Tick state.** Every amendment row below is `[ ]` — **proposed, not applied**. The rulings the
plan rests on are issued in **ADR-0331**; nothing in this run edits `spec/` or the manifest's
`## Spec amendments applied` section. Requirement ids are append-only: fold-backs amend the
named family row; no renumbering, and **no new requirement ids are proposed** (every fold-back
target is an existing row or an appendix).

## Amendment rows (proposed)

| # | scope | target file | anchor | before → after | tick |
|---|---|---|---|---|---|
| A-1 | DF-S1.26-2 parked-detach result member (R-2.11.1 family) | `spec/sections/07-surfaces.md` + inlined copy in `spec/CANONICAL_SPEC.md` | §7.1 — the §2.5 exit-class/result table (the `result` record row beside the `ExitClass` sum at line ~103 and the `run`-verb contract row at line ~55) | **Before:** `result.exit_class` is authoritative; the record declares no detach member — `detached:"parked"` is a build interim (ADR-0261 §9; ADR-0274 D9). **After:** the run-verb result record gains a declared optional member `detached?: "parked"` — a parked detach (`--park`, EOF on stdin, an unanswered prompt) answers `ok` + `detached:"parked"`; a parked run stays live/resumable, opens no turn and takes no `ExitClass` row; the closed `ExitClass` sum is unchanged. Ruling: ADR-0331 D2. | [ ] |
| A-2 | DF-S4.13-1 / OQ-388 measured bound + maintenance policy (R-2.2.1/AC-R-2.2.1-16; AC-R-2.11.2-13) | `spec/sections/07-surfaces.md` (the `[[DEFERRED ADR-0214: OQ-388 …]]` marker at line 258) + `spec/sections/05a-runtime-durability.md` (AC-R-2.2.1-16 note, line ~130) + inlined copies | §7.1 deferred-marker cell; §5a acceptance table | **Before:** "the spec states the bound's shape (proportional to result size) without a value". **After:** the deferred marker gains a dated measurement note — S4.13 (ADR-0305 D1) landed `by_event_id`/`by_class`/`ir_index` incremental at append (rebuild-identical), every other projection a shared-fold rebuild, and measured a 100-of-10⁵ filtered `read` at ≈15 ms (`ac_r_2_2_1_16_latency_1e5_fixture`, CI-enforced) — confirming the result-proportional shape; **ratification of the bound value and the policy remains the WS-B1/WS-K2 act** the register names. Ruling: ADR-0331 D4. | [ ] |
| A-3 | ADR-appendix parity — the build ADR set (BM-ADR-04) | `spec/CANONICAL_SPEC.md` only (Appendix B has no section-file source) | `## Appendix B — ADR index`, the prose paragraph under the header | **Before:** "ADR-0001…ADR-0216: title … regenerated at each assembly" — the appendix indexes only the pre-build ratified set. **After:** append a paragraph — "Build-phase ADRs **ADR-0217–ADR-0330** (per-ticket deviation/ruling records authored by the implement-spec chain) live in `docs/adr/` with a generated index (`docs/adr/README.md`); they amend no ratified contract and are indexed here by reference; the Sections column does not apply." | [ ] |
| A-4 | Dispositioned — DF-S2.5-1 custody ruling (OQ-170; ADR-0213) | no amendment proposed | — | The ratification act belongs to **WS-H3/WS-H6/WS-L4** (ADR-0213's OQ-170 row — "blocking for Stage 2 signing", `KernelSignerRecord` in the manifest, custody unassigned). The build's interim is spec-conformant and is recorded, not amended: `AuditSigner`/`AuditKeyResolver` seam consumed by `hh-ledger`, `signer_key_ids` empty in manifests, `audit_view` rendering `checkpoints: n/a{no signer_key_ids declared}` (ADR-0331 D3). A spec edit that named a custody answer would mint the deferred ruling the ADR reserves — DF-S2.5-1 stays OPEN. | n/a — external |
| A-5 | Dispositioned — CF-476 / OQ-471 (§10.2 seventh gate) | no amendment proposed | §10.2 gate table | The interim restatement (gate 7 removed from the "verbatim" table; `outcome_bounds`/`multiplicity` as a reporting rule with ADR-0190 D6's scope) is already in the spec; whether the robust verdict becomes a general C4 admissibility gate is WS-I5/WS-J4's, due Stage 6 (register `conflicts.md` CF-476, OQ-471). Nothing to fold. | n/a — external |
| A-6 | Dispositioned — CF-487 / OQ-470 (`refused_by_kernel` derivation) | no amendment proposed | §7.1 open-question blockquote | The CF-487 interim reading is already carried in `spec/sections/07-surfaces.md` (the OQ-470 note, line 122); ratification is an ADR-0169 amendment owned by WS-K1 with WS-K4/WS-F2, due Stage 2 — an ADR-level act, not a spec text gap. Nothing to fold. | n/a — external |
| A-7 | Dispositioned — DF-S1.24-3 `debt.hypothesis` string vs `Text` (R-2.9.4/R-2.10.x homes) | **no amendment needed** | §5b `ModelProfile/1` envelope (line ~309); §3.2 debt-record shape (line ~221) | The spec does not pin the leaf type of `debt.hypothesis` at either home — the divergence is below spec granularity. **ADR-0331 D1 rules the divergence intentional** (the /1 dialect is landed; `hypothesis_typed` carries the structured form on both homes; a `ModelProfile/2` bump MAY unify at the owning cadence but is not owed) — the deferral's "an ADR records the divergence as intentional" exit is satisfied and the row flips DONE this run. | n/a — discharged by ADR |
| A-8 | Dispositioned — the six `MET-DIFFERENTLY` rows + signed GATE-ACCEPT deviations | no amendment proposed | `docs/build/COVERAGE_MATRIX.csv` | R-2.7.2 is a spec-sanctioned split (ADR-0146) already carried as R-2.7.2a/b; R-2.12.4 is deferred by the spec itself (ADR-0210); R-2.12.3/2.12.5/2.11.2/2.12.6 are operator-signed ACCEPTED deviations (GATE-ACCEPT §4, blanket-signed 2026-10-02) — deviations on the record, not spec text to change. | n/a — signed |

## Fold-back id accounting

- **New requirement ids proposed: none.** A-1 amends the existing R-2.11.1 surface contract;
  A-2 annotates the R-2.2.1/R-2.11.2 acceptance rows; A-3 extends Appendix B's header prose.
  Every `ticket-added` deliverable in `TICKET_VS_SPEC.md` is build/program infrastructure
  (tooling, fixtures, build memory, retained spikes) — dispositioned `ID`, outside the product
  spec's scope; none earns a requirement id.
- Families touched if ticked: `R-2.11.1` (A-1), `R-2.2.1`/`R-2.11.2` (A-2), Appendix B (A-3).

## ADR set vs spec Appendix B — parity result

| set | membership | home |
|---|---|---|
| Spec Appendix B | ADR-0001…ADR-0216 (216 rows, ratified; Sections column recomputed from section-file citations) | `research/decisions/ADR-*.md` |
| `docs/adr/` | ADR-0217…ADR-0330 (114 files, build-phase ruling/deviation records) + generated `README.md` | `docs/adr/` |

**Delta = disjoint by construction, not an error**: the appendix indexes the ratified decision
record the spec was assembled from; the build set postdates it and is the implement-spec
chain's own rulings (each lands in `docs/adr/` with a `## Revisit trigger`). `check-build-memory`
reports the diff as BM-ADR-04 (warning, not a violation). Amendment **A-3** closes the parity
loop by indexing the build set by reference in Appendix B — until ticked, this row **is** the
dispositioned difference the acceptance criterion allows ("the ADR set equals the spec's ADR
appendix, or the difference is a dispositioned row").

## What re-running with `apply_amendments=true` does (operator checklist)

1. Tick `[x]` on each approved row (A-1/A-2/A-3; A-4–A-8 carry no tick — their acts are external
   or already discharged).
2. Edit `spec/sections/<file>` **and** the inlined copy inside `spec/CANONICAL_SPEC.md`
   identically (no in-repo generator; keep the two verbatim).
3. Append one `## Spec amendments applied` line per amendment to
   `docs/tickets/00_MANIFEST.md` (date · section · before/after · approver · ADR) — append-only,
   never rewriting existing lines.
4. ADR-0331 already carries the rulings; a ticked A-1 needs no new ADR (the decision exists).
5. On A-1's tick, flip DF-S1.26-2's residual cell; on A-2's tick, flip DF-S4.13-1's.
