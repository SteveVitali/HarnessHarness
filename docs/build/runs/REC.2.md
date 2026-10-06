# REC.2 run — spec reconciliation

- **Harness:** devin-cli subagent (Cognition Devin; deterministic/CPU
  only — offline/hermetic; this is a reconciliation ticket, no runtime
  surface, credentials, or network legs)
- **Ticket:** `docs/tickets/103_REC.2__spec-reconciliation.md`
  (manifest row 103 — reconcile 2 of 3, sequence 103 of 106)
- **Branch:** `svitali/harnessharness-rec.2` (stacked on the chain tip
  `svitali/harnessharness-rec.1` @ `bb8b482`)
- **PR:** https://github.com/SteveVitali/HarnessHarness/pull/101
  (stacked on `svitali/harnessharness-rec.1` / PR #100 — do not
  merge, OPERATOR policy)
- **Mode:** `reconcile-build mode=spec` — proposals-only
  (`apply_amendments=false` default: every amendment stays `[ ]`; the
  operator ticks and re-runs)

## What landed

- `docs/build/TICKET_VS_SPEC.md` — every landed chain row (98 landed
  rows across BUILD_INDEX seq 1–99) tagged: **in-spec 389 ·
  spec-implied 114 · ticket-added 17** tagged deliverable groups, each
  with a disposition (`ID` implementation detail / `AX` appendix note /
  `FB→plan` fold-back). No landed deliverable contradicts the spec.
- `docs/build/SPEC_RECONCILIATION_PLAN.md` — the amendment plan: **3
  proposed amendments, 0 applied** (A-1 parked-detach `detached` member
  → §7.1/R-2.11.1; A-2 OQ-388 measured bound + maintenance policy →
  §7.1/§5a AC-R-2.2.1-16; A-3 build-ADR-set reference note → Appendix
  B) plus five dispositioned rows (A-4 custody external, A-5 CF-476,
  A-6 CF-487, A-7 `debt.hypothesis` discharged by ADR, A-8 the signed
  deviation set). No new requirement ids — every fold lands on an
  existing row or an appendix.
- `docs/adr/ADR-0331` — the five REC.2 rulings: D1 `debt.hypothesis`
  divergence intentional (discharges DF-S1.24-3); D2 parked-detach =
  declared `detached:"parked"` member, no `ExitClass` widening; D3
  signer custody external (OQ-170/WS-H3-H6-L4); D4 OQ-388 measurement
  recorded, ratification WS-B1/K2; D5 the ADR-appendix delta
  dispositioned. `docs/adr/README.md` regenerated via `adr-index.sh`.
- `docs/tickets/DEFERRALS.md` — DF-S1.24-3 flipped **DONE**
  (ADR-0331 D1 is its ADR-exit); dated progress notes on DF-S1.26-2,
  DF-S2.5-1, DF-S4.13-1 (all stay OPEN — their folds need a tick or a
  named workstream act).
- `docs/build/BACKLOG.csv`/`BACKLOG.md` — ADR-0331 appended to BL-30's
  sources + the dated ADR sweep table (tally 55 fired-unanswered /
  115 total); BL-30 keeps `open` with a dated progress note.

## The four BL-30 rulings — what was owed vs delivered

| row | owed | delivered |
|---|---|---|
| DF-S1.24-3 | an ADR ruling `debt.hypothesis` canonical | ADR-0331 D1 — **row DONE** |
| DF-S1.26-2 | the parked-detach closed-sum ruling | ADR-0331 D2 (declare `detached`, no ExitClass member) + plan A-1 — **OPEN pending tick** |
| DF-S2.5-1 | the ADR-0213/OQ-170 custody ruling | ADR-0331 D3 — external to WS-H3/H6/L4; landed seam recorded conformant — **OPEN (external)** |
| DF-S4.13-1 | WS-B1/K2 ratification of the OQ-388 bound + policy | ADR-0331 D4 + plan A-2 proposing the measured text — **OPEN pending tick/workstream** |

## Verification

- `check-backlog.sh` → exit 0 (146 expected sources placed once;
  ADR-0331's revisit trigger placed at BL-30).
- `check-build-memory.sh .` → **0 violations** (7 pre-existing
  warnings: manifest round banners, BM-ADR-04 appendix delta —
  dispositioned as plan row A-3, nextTicket ordering, oversized
  phase-log entries, 2 BUILD_INDEX column warnings, old readout guard
  sentence, old run-ledger `Harness:` headers — none introduced by
  REC.2).
- `grep -n '^||' docs/tickets/DEFERRALS.md` → empty.
- `cargo test --workspace` **not run** — no code changed (reconcile
  ticket; the ticket's own contract).
- `spec/` untouched: no `spec_src/`/`BUILD.sh` exists in-repo (the
  canonical spec is a committed artifact); amendments would edit
  `spec/sections/*` + the inlined copy — none applied (nothing ticked).
- Write-integrity: all file writes via shell/python (not the edit
  tool); `git diff`/`git show --stat` verified after every write.

## Not done (status discipline)

- No spec text amended, no manifest `## Spec amendments applied` line —
  zero ticked amendments (the manifest section stays "(none …)").
- No executed ticket contract edited; rulings land in the ADR + plan.
- BL-30 stays `open`; DF-S1.26-2/DF-S2.5-1/DF-S4.13-1 stay OPEN.
- ADR-appendix delta (BM-ADR-04 warning) left as a warning — A-3 is the
  tickable fix; the delta is a dispositioned plan row meanwhile.

## Evidence report

Deterministic ACs: `TICKET_VS_SPEC.md` exists with all 98 landed rows
tagged and dispositioned; `SPEC_RECONCILIATION_PLAN.md` rows carry
target file + anchor + before/after + tick state, and applied ⊆ ticked
(vacuously — none ticked, none applied); the ADR-set-vs-appendix delta
is a dispositioned row (§"ADR set vs spec Appendix B"); ADR-0331
written for the owned rulings; BUILD_INDEX row 100 + LEDGER advanced in
the closeout commit. Agentic AC: the requirement ids stamped in the PR
are the BL-30 family `R-2.8.6 · R-2.2.1 · R-2.9.4 · R-2.9.6 · R-2.11.1`.
