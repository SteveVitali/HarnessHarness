# ADR-0331: REC.2 spec-reconciliation rulings — `debt.hypothesis` canonical, parked-detach declared, custody and OQ-388 external, ADR-appendix parity disposition

- **Status:** Accepted
- **Date:** 2026-10-06
- **Ticket:** REC.2
- **Requirement ids:** R-2.8.6 · R-2.2.1 · R-2.9.4 · R-2.9.6 · R-2.11.1 (the BL-30 ruling rows; DF-S2.5-1 · DF-S4.13-1 · DF-S1.24-3 · DF-S1.26-2)
- **Spec:** §5b (`ModelProfile/1` envelope) · §3.2 (debt-record shape) · §7.1 (exit-class/result record; OQ-388 deferred marker; OQ-470 note) · §5a (AC-R-2.2.1-16) · §10.2 · Appendix B · ADR-0213/OQ-170 · ADR-0214/OQ-388 · CF-476/CF-487
- **Rung (ADR-0024 obligation):** MUST-data — the rulings fix data-level spellings and surface members; no code contract migrates.

## Context

REC.2 owns the four BL-30 rows GATE-ACCEPT routed to spec reconciliation — cells
whose residual is a *ruling*, not code. Two rulings sit inside the build's
authority (the `debt.hypothesis` spelling, the parked-detach surface shape); two
are explicitly reserved to named workstreams by the ratified deferral ADRs
(OQ-170 custody to WS-H3/H6/L4; OQ-388 ratification to WS-B1/K2). The
reconciliation also owes the ADR-set-vs-appendix parity answer (BM-ADR-04).

## Decision

**D1 — `ModelProfile/1`'s `debt.hypothesis` stays a plain string; the divergence
is intentional (discharges DF-S1.24-3).** The landed dialect keeps `hypothesis`
as a plain string at the profile home while the HIR debt home's
`AssumptionDebtRecord.hypothesis` is a `Text` leaf. Both spellings are canonical
at their homes: the `Text` leaf preserves provenance where the IR needs it, the
profile's string is the versioned-document form, and the additive
`hypothesis_typed{subject, deficiency_class, predicted_effect}` member —
available on both homes — already carries the structured form for any consumer
that needs it. A `ModelProfile/2` dialect bump MAY unify the spellings at the
owning bump cadence but is **not owed**; old `/1` bodies still decode, so the
deferral's exit ("an ADR records the divergence as intentional; old /1 bodies
still decode") is satisfied here.

**D2 — The parked-detach surface is `detached:"parked"` on the run-verb result
record; no `ExitClass` member is minted (ruling for DF-S1.26-2).** A parked
detach (`--park`, EOF on stdin, an unanswered prompt) is a live/resumable run,
not a terminal — `ok` + `detached:"parked"` is the only honest answer:
`finished` would claim a terminal that did not happen and `refused` would claim
a refusal that never occurred. The closed `ExitClass` sum is untouched
(widening it unilaterally was correctly refused at S1.26/ADR-0261 §9 and
S2.12/ADR-0274 D9). The spec fold is amendment **A-1** of
`docs/build/SPEC_RECONCILIATION_PLAN.md` — declaring the member in §7.1 is a
spec-text act, so it waits for the sponsor's tick; the ruling of *what the
right answer is* is made here. DF-S1.26-2 stays OPEN pending the tick (its
exit needs the §2.5 table or the declared schema).

**D3 — Kernel signer custody stays external; the landed seam is the conformant
interim (DF-S2.5-1).** ADR-0213 reserves OQ-170's ratification to
WS-H3/WS-H6/WS-L4 ("custody unassigned; blocking for Stage 2 signing"). The
build may not mint it — and does not need to: `hh-ledger` consumes the
`AuditSigner`/`AuditKeyResolver` traits so custody lives outside the store,
manifests declare `signer_key_ids` (empty until custody lands), and
`audit_view` reports `checkpoints: n/a{no signer_key_ids declared}` rather than
a fabricated row. No spec amendment is proposed; a text edit naming a custody
answer would steal the deferred ruling. DF-S2.5-1 stays OPEN; the checkpoint-
emission/cadence legs it carries ride the same act.

**D4 — The OQ-388 measurement is on the record; ratification is WS-B1/K2's
(DF-S4.13-1).** ADR-0305 D1's landed answer — `by_event_id`/`by_class`/
`ir_index` incremental at append (rebuild-identical), every other projection a
shared-fold rebuild, a 100-of-10⁵ filtered `read` at ≈15 ms — confirms the
deferred marker's result-proportional shape and is CI-enforced by
`ac_r_2_2_1_16_latency_1e5_fixture`. Amendment **A-2** proposes writing that
measurement into the §7.1 deferred-marker cell and the §5a AC-R-2.2.1-16 note;
the register's close text names the workstreams' ratification act, which no
build ticket mints. DF-S4.13-1 stays OPEN pending the tick/act.

**D5 — The ADR-set/appendix delta is disjoint-by-construction, dispositioned,
not a defect (BM-ADR-04).** Appendix B indexes the pre-build ratified set
(ADR-0001–0216, `research/decisions/`); the build's rulings are ADR-0217–0330
in `docs/adr/` with a generated index. Amendment **A-3** proposes the one-paragraph
reference note under the Appendix B header so the parity check has a spec-side
answer; until ticked, the delta is a dispositioned row per the ticket's AC.
The six `MET-DIFFERENTLY` coverage rows need no fold: R-2.7.2's split is
spec-sanctioned (ADR-0146), R-2.12.4 is deferred by the spec itself
(ADR-0210), and R-2.12.3/2.12.5/2.11.2/2.12.6 are operator-signed
ACCEPTED deviations (GATE-ACCEPT §4). CF-476/OQ-471 (§10.2 gate-7) and
CF-487/OQ-470 (`refused_by_kernel`) already carry their interim readings in the
spec; their ratifications are WS-I5/J4 (Stage 6) and an ADR-0169 amendment by
WS-K1/K4/F2 — recorded, not folded.

## Consequences

- DF-S1.24-3 flips DONE this run (D1 satisfies its ADR-exit).
- DF-S1.26-2, DF-S2.5-1, DF-S4.13-1 stay OPEN with dated progress notes naming
  this ADR and the owning act (tick on plan A-1/A-2, or the named workstream's
  ratification).
- `spec/` and the manifest's `## Spec amendments applied` section are
  untouched — every amendment stays `[ ]` until the operator ticks and re-runs
  `reconcile-build mode=spec apply_amendments=true`.
- BL-30's owed rulings are discharged as *rulings* (D1–D5); what remains on the
  rows is the external/ticked fold, which the plan names per row.

## Alternatives considered

- **Apply the A-1/A-2/A-3 folds unilaterally now:** rejected — the manifest's
  amendment protocol requires the approver and the tick; a reconcile ticket
  proposing its own approval rewrites the spec wishfully, which the standing
  rule forbids.
- **Rule custody/OQ-388 in-build anyway:** rejected — ADR-0213/ADR-0214 name the
  owning workstreams and mark the interim rules ratified-in-force; an in-build
  answer would fork the register.
- **Widen `ExitClass` for parked-detach:** rejected — a parked run is not a
  terminal; the closed sum stays closed (ADR-0261 §9 / ADR-0274 D9 precedent).
- **Re-type `debt.hypothesis` to `Text` in place:** rejected — `/1` is a landed
  dialect; re-typing a ratified member is a breaking change, not additive.

## Revisit trigger

The operator ticks A-1/A-2/A-3 (the folds land and the named DF rows flip);
WS-H3/H6/L4 ratifies OQ-170 custody (D3's interim ends and the custody/emission
legs of DF-S2.5-1 close); WS-B1/K2 ratifies the OQ-388 bound + policy (D4's
interim ends); a `ModelProfile/2` dialect bump is scheduled (D1's unification
option becomes live); or the BM-ADR-04 warning must clear (A-3 becomes the only
path).
