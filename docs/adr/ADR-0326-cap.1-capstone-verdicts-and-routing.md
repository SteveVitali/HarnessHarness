# ADR-0326: CAP.1 capstone verdicts and gap routing

- **Status:** Accepted
- **Date:** 2026-10-02
- **Ticket:** CAP.1
- **Requirement ids:** all
  `R-2\.[0-9]+\.[0-9]+[a-z]?` (66
  rows); routing names DF-S2.4-1,
  DF-S2.8-1, DF-S3.12b-1,
  DF-S3.12b-2, DF-S2.9-3,
  DF-S4.11-1/-2/-3, DF-S2.10-1,
  DF-S1.26-2, DF-S4.2-1/-2,
  DF-S4.10-1, DF-S5.6-1, DF-S1.25-1
  and the GATE-ACCEPT V-row set
  (DF-S0.3-2/-3, DF-S1.2-2,
  DF-S1.5-3, DF-S1.8-1, DF-S1.27-1).
- **Spec:** whole corpus; Appendix A
  coverage matrix; §9.8 (ladder
  verification); §10.8 (evidence
  discipline); §11 (deferral rule).
- **Rung (ADR-0024 obligation):**
  capstone analysis — records
  verdicts and routing; no contract
  or schema migrates.

## Context

CAP.1 is the whole-build verdict:
an independent judge of the composed
92-ticket chain against
`spec/CANONICAL_SPEC.md`, run in
fresh context with all 66 verdicts
recorded (commit `efd71f5`) before
any `docs/build/runs/*` file was
opened. The verdict roll-up is 29
MET / 6 MET-DIFFERENTLY / 29
PARTIAL / 0 MISSING / 2
AT-RISK-INTEGRATION. Thirty-one rows
carry open residual legs; the
analysis had to rule, per top gap,
whether it is CAP.3 fixable, an
accepted deviation, or already
deferred — and whether any newly
discovered gap lacked a
`DEFERRALS.md` row.

## Decision

D1. **Verdict discipline.** A
requirement is MET only where its
landed-stage contract rows are
landed *and* tested on the composed
tree; a specified behavior that is
absent, unwired, or refused on a
real path is PARTIAL even when the
machinery exists in pieces; landed
pieces whose required composition is
never exercised are
AT-RISK-INTEGRATION; a contract
satisfied through a split, a
spec-level deferral, a ratified
decision, or a recorded proxy is
MET-DIFFERENTLY.

D2. **CAP.3 fix list — three items.**
(1) Wire the `net_egress` effect
surface to `EgressMediator` or rule
fail-closed refusal as intended
(R-2.8.4; DF-S2.4-1). (2) Land a
real `AssemblerPort` backed by
`hh-context`, or rule the
pass-through `KernelAssembler` as
the embed-boundary contract
(R-2.4.1; DF-S2.8-1). (3) Rule the
`measurement.experiment.closed` row
shape vs `AUDIT_FIELD_MAX_BYTES`
(member shrink, `content_refs`
offload, or class re-bind) so an
honestly-measured exemplar can close
(R-2.10.3; DF-S3.12b-1) — the only
seam that outright blocks a
specified path.

D3. **Accepted deviations.** The
R-2.7.2 split (ADR-0146), the
R-2.12.3 decision-plus-E1 discharge
(ADR-0050), the R-2.12.4 spec
deferral (ADR-0210), the R-2.12.5
program-level item, and the two
human-prerequisite proxies
(HUMAN-H1 → in-ecosystem generated
client, DF-S4.10-1; HUMAN-H2 →
fixture adapter, DF-S5.6-1) stand as
accepted deviations for the
GATE-ACCEPT set.

D4. **Everything else defers.** All
remaining residual legs are already
carried by open `DEFERRALS.md` rows
with named owners and verification
paths — no new deferral row was
required (CAP.1 opens zero).

D5. **Matrix column semantics.** In
`COVERAGE_MATRIX.csv`: `level` is
the spec's two-level reading
(`object` runtime machinery,
`instrument` Lab/measurement,
`cross` §8 cross-cutting, `program`
non-code items); `class` is the
Appendix-A tier (C0–C4, `—` for
program items); `routing` names the
disposition (fix-CAP.3 / accept /
defer-DF-row).

## Consequences

- CAP.3 has a bounded fix list (D2)
  and a pre-adjudicated deviation set
  (D3); CAP.2 has a prioritized list
  of never-composed paths (S8 of the
  analysis).
- The honest-refusal pattern means
  PARTIAL rows are *known-incomplete
  surfaces*, not silent gaps; the
  deferral ledger already owns all
  of them.
- The two AT-RISK verdicts are the
  load-bearing finding: security and
  context machinery that per-ticket
  ledgers reported green exists, but
  the composed call sites were never
  driven outside tests.

## Alternatives considered

- **Verdict per self-report:**
  rejected — the ticket's whole
  purpose is a fresh-context verdict
  that per-ticket self-reports cannot
  contaminate; verdicts were
  committed before run ledgers were
  read.
- **MISSING for unwired paths:**
  rejected — the machinery is real
  and tested; PARTIAL/AT-RISK is the
  honest grade and preserves the
  fix-vs-absence distinction CAP.3
  needs.
- **New deferral rows for the seam
  findings:** rejected — every seam
  traced back to an existing row;
  duplicate rows would fork the
  residual ledger.

## Revisit trigger

CAP.3 closes or re-routes any D2
item; GATE-ACCEPT discharges or
refuses the D3 set; CAP.2's composed
runs either confirm the never-run
list or find further seams.
