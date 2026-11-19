<!--
  docs/build/reports/round6/SPEC_RECONCILIATION_PLAN.md — reconcile-build mode=spec
  deliverable (BM-RECON-02), round-6 scope. Written by CAP6.3 (2026-11-19, ticket 323).
  Append-only. Historical plans: docs/build/SPEC_RECONCILIATION_PLAN.md (rounds 1–4) and
  docs/build/reports/round5/SPEC_RECONCILIATION_PLAN.md. Amendments apply ONLY where
  ticked; a tick is an operator/sponsor act — re-run `reconcile-build mode=spec
  apply_amendments=true` after ticking, with a manifest `## Spec amendments applied`
  line + ADR. The supplement `docs/experiments-spec.md` is frozen ratified text: no
  spec_src/ exists for it, so an applied amendment edits the named file directly per
  the row's target field (X-01's protocol: register row + manifest line + ADR).
-->
# SPEC RECONCILIATION PLAN — round 6: folding shipped reality back into the contracts

**Inputs.** `TICKET_VS_SPEC.md` (this directory — 85 landed round-6 rows tagged) ·
`docs/experiments-spec.md` (canonical v0.3, frozen sha256 `1879824c5fe5…`) · §10.5
register (AR-01…AR-33 + intervention rows A1–A7) vs `docs/adr/` · CAP6.1
`COVERAGE_MATRIX.csv` + `CAPSTONE_GAP_ANALYSIS.md` · CAP6.2 `CAPSTONE_CLOSURE.md` +
`LATER_PASS_ROUTING.md` (the §0.3 history result) · `docs/build/BACKLOG.csv` +
`reports/round6/BACKLOG.{csv,md}` · `docs/tickets/DEFERRALS.md` · manifest
`## Spec amendments applied` (lines 916–925 for round 6) · `docs/build/readouts/`
GATE-G20…G31 / HUMAN-H12…H18.

**Tick state.** Every amendment row below is `[ ]` — **proposed, not applied**
(`apply_amendments=false`). Nothing in this run edits `spec/`,
`docs/experiments-spec.md`, `docs/adr/`, or the manifest's `## Spec amendments applied`
section. Requirement ids are append-only — a fold-back gets a new id in its family,
never a renumber; **no new requirement ids are proposed**.

## Amendment rows (proposed)

| # | scope | target file | anchor | before → after | tick |
|---|---|---|---|---|---|
| R6A-1 | Canonical Appendix-B ADR-index range — the frozen mismatch extended by the landed set (carries forward unticked R5A-1) | `spec/CANONICAL_SPEC.md` (Appendix B has no section-file source) | `## Appendix B — ADR index`, the applied-A-3 paragraph | **Before:** "Build-phase ADRs **ADR-0217–ADR-0330** … indexed here by reference". **After:** the same paragraph re-reads "Build-phase ADRs **ADR-0217–ADR-0451** (per-ticket deviation/ruling records authored by the implement-spec chain through round 6) live in `docs/adr/` with a generated index; they amend no ratified contract and are indexed here by reference; the Sections column does not apply." Same BM-ADR-04 family as R5A-1 — preserved as dispositioned legacy until ticked, never silently rewritten | [ ] |
| R6A-2 | §10.5 register coverage — mint the two owed ADRs | `docs/adr/ADR-0452-*.md` + `docs/adr/ADR-0453-*.md` (new files; append-only homes), then `adr-index.sh` regen | §10.5 register rows AR-08 / AR-10; R6.10 contract item 12 | **Before:** AR-08 (model-mediation proxy: hosting placement, phase proxy run, release modes, `hh.model_io/1` `proxy` member, two measurement classes, the S-9 reading) and AR-10 (`GenParamPolicy` path-qualified `pin`/`params_scope`) landed at R6.10/R6.11 with no ADR — the register's "Every ADR lands with its implementing ticket" is unmet. **After:** two dated landing ADRs authored from the committed design records (runs/R6.10.md, runs/R6.11.md, `hh-model-proxy`/`GenParamPolicy` code) documenting the design *as landed* — a record of what shipped, never a retroactive design claim. Alternatively GATE-G32 waives the pair as an accepted deviation; either way the register difference is dispositioned, not silent | [ ] |
| R6A-3 | Dispositioned — intervention amendments A1–A7 already applied | **no amendment proposed** | §10.5 "First-pass intervention amendments" + manifest lines 919–925 | The seven operator-ruling amendments are already appended to the re-pinned spec with ADR-0443…0449 and verbatim manifest lines — the register's own protocol ran correctly. Nothing to fold | n/a — applied at landing |
| R6A-4 | Dispositioned — AR-24 canonical amendment | **no amendment proposed** | `spec/sections/05e-control-orchestration.md` + manifest line 917 | GATE-G24 PASSED (verbatim 2026-11-10) → `stop_rule.submit{marker, line, require_returncode}` applied verbatim by R6.38 through the canonical section-source + regen path. The one canonical amendment the register names is landed and indexed | n/a — applied at landing |
| R6A-5 | Dispositioned — register non-ADR kinds | **no amendment proposed** | §10.5 kinds column | AR-02/AR-03 (REC) landed as R6.3's dated records; AR-16 (SUPP §4.5) landed as the `evaluate_admitted_costs`/`hh-slot-costs/1` mechanism at R6.14; AR-25 (SCHEMA) landed as `open_session.exec` + regenerated clients at R6.38; AR-31 (decomposition decision) recorded at GUARD-X/R6.1 + the decomposition plan; AR-32 (SUPP + `check-position.sh`) landed at R6.3. Kinds were honoured; nothing to fold | n/a — landed |
| R6A-6 | Dispositioned — later-pass register rows | **no amendment proposed** | §10.5 Pass column | AR-19 (`lab_host`), AR-26 (effort factor), AR-27 (compaction INTERP), AR-29 (optional fonts), AR-30 (Terminus-2 stage two) are marked `later`/`not v1` in the register itself — their absence from round 6 is the register working as designed. AR-19 appears in ADR-0436 only as a "refuses typed until AR-19" note — correct | n/a — by design |
| R6A-7 | Dispositioned — the owed predicate set (46 PARTIAL) | **no amendment proposed** | `COVERAGE_MATRIX.csv` + `LATER_PASS_ROUTING.md` + DEFERRALS | Every PARTIAL has a typed home: 34 routed to the named later pass (owner + trigger per row; a routed id is never satisfied), 5 discharged by CAP6.2's artifacts, 4 scope-ended under GATE-G30 verbatim, 1 unexecuted (XC-325 — waiver rides G32), 2 aliases inheriting. Documentation cannot discharge them; they close on their own triggers | n/a — owed work, not text |
| R6A-8 | Dispositioned — deferral row-reader seams | **no amendment proposed** | `docs/tickets/DEFERRALS.md` (coordinator-owned) | DF-R6.45l-1's DONE text sits on unmerged #413 (lands with the merge or a coordinator dated row — proposed text in `runs/CAP6.3.md`); DF-CAP6.1-1…5 carry dispositions in `CAPSTONE_CLOSURE.md` but no dated status rows — proposed status-update text in `runs/CAP6.3.md` (the DF-R6.50-1 pattern). Row-reader seams, not spec text | n/a — ledger maintenance, not text |
| R6A-9 | Dispositioned — XC-325 waiver + Ws1 over-ceiling + retention + merge order | **no amendment proposed** | GATE-G32's signature list | These are operator rulings, not spec text: the verbatim staging list is in `INTEGRATION_PLAN.md` §7 and `runs/CAP6.3.md`. Nothing the spec says changes their state | n/a — gate items, not text |

## Fold-back id accounting

- **New requirement ids proposed: none.** R6A-1 amends appendix header prose only;
  R6A-2 mints two ADRs (records of landed design) — neither creates a requirement.
  Every `ticket-added` group in `TICKET_VS_SPEC.md` is build/evidence infrastructure
  (emitters, checkers, batteries, capstone instruments, run ledgers) — outside the
  product spec's scope; none earns a requirement id.
- Families touched if ticked: canonical Appendix-B header prose only (R6A-1);
  `docs/adr/` set + generated index (R6A-2). No §-numbered section, requirement row,
  or acceptance cell changes.

## ADR set vs spec §10.5 register — parity result (round 6)

| set | membership | home |
|---|---|---|
| §10.5 needed-in-round-6 ADRs | AR-01, AR-04…AR-15, AR-17, AR-18, AR-20…AR-23, AR-28, AR-33 (21 rows) | `docs/adr/` expected |
| `docs/adr/` minted for those rows | AR-01→0423 · AR-04→0427 · AR-05→0433 · AR-06→0428 · AR-07→0429 · AR-09→0425 · AR-11→0426 · AR-12→0434 · AR-13→0435 · AR-14→0432 · AR-15→0431+0437 · AR-17→0438 · AR-18→0436 · AR-20→0430+0437 · AR-21→0440 · AR-22→0441 · AR-23→0442 · AR-28→0424 · AR-33→0439 | **19 minted** |
| **Delta — owed** | **AR-08, AR-10 → no file** | → R6A-2 |
| Register non-ADR kinds landed | AR-02/03 (REC, R6.3) · AR-16 (SUPP mechanism, R6.14) · AR-24 (AMEND-C, applied) · AR-25 (SCHEMA, R6.38) · AR-31 (decision, R6.1) · AR-32 (SUPP, R6.3) | records/code |
| §10.5 intervention amendments | A1–A7 → ADR-0443…0449 | `docs/adr/` |
| Unplanned round-6 ADRs | ADR-0450 (Ws1 recording-relative L1.1 fold), ADR-0451 (`hh-l1-corpus/1` manifest) — Ws1 machinery records, placed in BL-47 | `docs/adr/` |
| Later-pass register rows (absent by design) | AR-19, AR-26, AR-27, AR-29, AR-30 | none |
| Canonical Appendix-B pointer | names ADR-0217–0330 vs landed → ADR-0451 | → R6A-1 |

## REQ-X coverage state (cited, not re-computed)

From CAP6.1's matrix × CAP6.2's closure (V-13): **engineering closed 410/456 ·
requirement satisfied 410/456 · PARTIAL 46 · MISSING 0**, MET-ENGINEERED never
counted. The 46: 34 routed (later pass owns; trigger per row —
`LATER_PASS_ROUTING.md`), 5 discharged at CAP6.2, 4 scope-ended verbatim under
GATE-G30 `re-scope{end-first-pass-early}` (XD-110, XD-119, XH-111, XH-115), 1
unexecuted (XC-325 — `unexecuted{no_executor}`, waiver rides G32), 2 aliases
inheriting at targets (XD-124→XE-013, XE-127→XD-205).

## §0.3 decision-register history (REQ-XH-015 — cited from CAP6.2)

CAP6.2's stage-3 check over `LEDGER.md ## GATE DECISIONS` (LATER_PASS_ROUTING.md):
every prior decision row preserved verbatim including the 15 restated round-6 rows
(originals kept in place, never rewritten); every later change dated (restatement
block headlined "Restated 2026-11-19 (DF-CAP6.1-1 / CAP6.1 finding GAP-01)"; every
`correction` row names its date and target); O2 dispositions reference ruling ids;
`git diff` over the region shows additions only; `supplement_guard.load_decisions`
reads all 15 restated rows — guard PASS `DISPATCH — CAP6.3` re-verified this run.

## Reconciliation findings the operator may want

1. **The round is honest to a machine-checkable depth.** V-1…V-15 in
   `TICKET_VS_SPEC.md`: zero new dependencies, generated artifacts in sync, every
   admitted amendment carries its manifest line + ADR, no silent deviation. The
   genuine seams are record-keeping: two owed ADRs (R6A-2), one stale appendix
   pointer (R6A-1), two deferral row-reader seams (R6A-8), and the inherited
   red-stack readers-scan violation (INTEGRATION_PLAN item I-3).
2. **The only owed spec text is optional by design.** R6A-1/R6A-2 are the entire
   proposed set; both stay `[ ]` until the operator ticks them.
3. **Obligations, not text.** 34 routed ids + XC-325 + the retention decision +
   merge ordering are GATE-G32 material — staged verbatim, never folded into the
   contract to look closed.
