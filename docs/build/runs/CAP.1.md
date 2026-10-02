# CAP.1 run — capstone gap analysis (whole-build, fresh-context)

- **Harness:** devin-cli subagent (Cognition Devin; deterministic/CPU
  only; analysis ticket — no run code, no live stage)
- **Ticket:** `docs/tickets/098_CAP.1__capstone-gap-analysis.md`
  (manifest row 98 — capstone 1 of 3)
- **Branch:** `svitali/harnessharness-cap.1` (forked from
  `svitali/harnessharness-s6.4` @ `257dcf5`)
- **PR:** (stacked PR URL recorded in the phase log at close)
- **Requirement ids:** all `R-2\.[0-9]+\.[0-9]+[a-z]?` — 66 ids
- **Mode:** offline/hermetic — read-only analysis over the composed
  tree + the build record; `cargo test --workspace` not required
  (analysis artifacts only)

## Anti-bias record (the point of this ticket)

All 66 requirement verdicts were derived from
`spec/CANONICAL_SPEC.md` (read in full), `docs/tickets/00_MANIFEST.md`,
`docs/tickets/DEFERRALS.md`, and the composed tree at the chain tip —
then committed as `docs/build/COVERAGE_MATRIX.csv` in **`efd71f5`**
before any `docs/build/runs/*` file was opened. Run ledgers were read
afterward only to annotate `CAPSTONE_GAP_ANALYSIS.md` (workspace test
posture at tip: 3010 tests / 0 failures / 1 pre-existing ignored, per
the S6.4 ledger) and the closeout formats. The verdicts did not change
after ledger consultation.

## What landed

- `docs/build/COVERAGE_MATRIX.csv` — exactly one row per canonical id
  (66/66), fixed column set
  `id, level, spec_section, class, verdict, evidence, owning_tickets, tests, adrs, routing, note`;
  `evidence` non-blank on every MET / MET-DIFFERENTLY row (35/35).
- `docs/build/CAPSTONE_GAP_ANALYSIS.md` — method + commands; roll-up by
  verdict and by requirement family; the seam hunt (S1–S8); the top-gap
  table with a routing decision per gap.
- `docs/adr/ADR-0326-cap.1-capstone-verdicts-and-routing.md` — the
  routing decisions as an owned ruling (D1 verdict discipline, D2 the
  three-item CAP.3 fix list, D3 the accepted-deviation set, D4 deferral
  coverage, D5 column semantics); `docs/adr/README.md` regenerated via
  `adr-index.sh` (never hand-edited).

## Verdict roll-up

| verdict | n |
|---|---|
| MET | 29 |
| MET-DIFFERENTLY | 6 |
| PARTIAL | 29 |
| MISSING | 0 |
| AT-RISK-INTEGRATION | 2 |

## Seam findings (verified on the tree, not on self-reports)

1. **R-2.8.4 AT-RISK-INTEGRATION** — `EgressMediator`
   (`hh-env/src/egress.rs:313`, `hh-containment/src/egress.rs:630`) has
   *no production caller*: `grep -rn 'EgressMediator' crates/` finds
   only `crates/hh-env/tests/egress_mediation.rs`. On the dispatch path,
   `PreconditionDomain::Network` is unconditionally violated
   (fail-closed, `hh-env/src/dispatch.rs`), so a `net_egress` effect is
   refused before mediation. The mediation machinery is test-only.
   → CAP.3 fix list (DF-S2.4-1; DF-S1.12-1/-2; DF-S2.4-2).
2. **R-2.4.1 AT-RISK-INTEGRATION** — `AssemblerPort`'s only production
   impl is `KernelAssembler`
   (`hh-embed/src/runtime.rs:163–176`), a documented pass-through; the
   live turn loop never reaches `hh_context::assemble`
   (`hh-context/src/assemble.rs:276`); `ContextWindowExceeded →
   CompactionRequired → compact` unwired (DF-S2.8-1).
   → CAP.3 fix list (DF-S2.8-1; DF-S1.19-1/-2).
3. **R-2.10.3 close-path defect** — `AUDIT_FIELD_MAX_BYTES = 512`
   (`hh-ledger/src/classes.rs:186`) refuses an honestly-measured
   `measurement.experiment.closed` at exemplar scale
   (`AuditFieldsTooLarge`, `hh-ledger/src/store.rs:3535`;
   DF-S3.12b-1). A contract-shape ruling is owed.
   → CAP.3 fix list.
4. Producer-cadence and surface residuals (snapshot cadence
   DF-S2.9-3; env verbs DF-S2.10-1; parked-detach DF-S1.26-2;
   SSE/oauth/respond_approval DF-S4.11-1/-2/-3; foreign vocab
   DF-S4.2-2; DefinitionInput::Ref DF-S1.25-1) — all already ledgered.
5. Environment/human-bound legs — six cross-impl V-rows to
   GATE-ACCEPT; HUMAN-H1/H2 proxies recorded (DF-S4.10-1, DF-S5.6-1).

## Deferrals

- **Opened:** none — every gap found is already carried by an existing
  OPEN/PARTIAL `DEFERRALS.md` row (D4 of ADR-0326).
- **Closed:** none — CAP.1 analyzes; CAP.3 owns closure.

## Verify

- Matrix integrity checked with the deterministic script in the
  analysis (66 unique rows; id set == spec regex universe; no blank
  evidence on MET/MET-DIFFERENTLY).
- `check-build-memory.sh .` — see closeout (result recorded in
  BUILD_INDEX row + LEDGER).
- No code changed; `cargo test --workspace` not run by design.

## chainTip → svitali/harnessharness-cap.1 · next → CAP.2
