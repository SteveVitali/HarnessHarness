<!--
  Round-6 contract — experiments first pass (decompose-spec mode=extend, 2026-11-07).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Hand-off: docs/experiments-decomposition-prompt.md. Partition + coverage:
  docs/build/planning/2026-11-07_round6-{decomposition,coverage}.md.
  Insertion authorized by the operator (planning conversation, 2026-11-07): "I want you to
  continue on until you're totally done, the new tickets are inserted, there's a new PR with
  the changes". This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
-->
# HUMAN-H18 — Consolidated billing reconciliation (W1, W2, W3, Wc, W4)

- **Sequence:** 320 of 325 · **Phase:** Round 6 · **Kind:** human
- **base_branch:** current checkout
- **Depends on:** R6.32, R6.40, R6.44, R6.45, R6.47
- **Blocks:** CAP6.2 (the billed-figure recompute); R6.46's D_max cites any entry already appended (REQ-XH-126(b))
- **Owner:** operator
- **Scheduled:** one trigger per window — ≥ 24 h after that window's TEARDOWN; each entry appended as soon as it is due. The row lands when all five entries are recorded (expected HOLD only while the W4 read is not yet due).
- **Withheld until done:** round closure totals (spec §10.3: "round closure totals"); every billed-figure statement for a window without its entry
- **Deferrals so far:** 0
- **Readout:** `docs/build/readouts/HUMAN-H18.md` (create from `_TEMPLATE.md` at the first due entry; dated, append-only)

> **Operator work — NOT an `implement-spec` input.** Billing reads are the operator's own; an agent never reads the billing console for them, estimates a bill, or infers an entry.

## Source

Spec §10.3 HUMAN table, row "Reconcile window Wn | operator | ≥ 24 h after each TEARDOWN, for every window (W1–W4 and Wc) | billing read: within / outside / unavailable | round closure totals"; [§4.7](../experiments-spec.md#s4) (reconciliation with provider telemetry); [§10.1.1](../experiments-spec.md#s10-1) (bill bounds hold only under its conditions). Consolidation: plan [Re-partition](../build/planning/2026-11-07_round6-decomposition.md#re-partition-operator-q-2026-11-07) (former HUMAN-H14, H18, H19, H20, H22) and tradeoff 10.

## Checklist

One dated entry per window, appended in window order as each falls due:

- [ ] **W1** — billing read ≥ 24 h after W1's teardown (R6.32's teardown receipt UTC cited): `within` / `outside` / `unavailable`, with the billed compute figure where one exists.
- [ ] **W2** — as above (R6.40), compute and the model line.
- [ ] **W3** — as above (R6.44), or `N/A` citing R6.44's `unused{…}` record when GATE-G27 recorded `not_run{…}`.
- [ ] **Wc** — as above (R6.45), or `N/A` citing R6.45's `unused{not_triggered}` record when GATE-G28 recorded "not triggered; Wc unused".
- [ ] **W4** — as above (R6.47), including the GCP project deletion receipt's UTC; or `N/A` citing R6.47's `unused{scope_end}` record (and its project-deletion receipt UTC) when GATE-G30 recorded `end_after_w2`.

Every entry: read UTC, the window, the read result token, the figure(s) in the phase spend ledger's unit, and the source (console/export) named generically. The billing-account id never enters the record (REQ-XB-219); credentials only `provided: yes/no`.

## Exit criterion

All five entries are recorded, each `within` / `outside` / `unavailable` or `N/A` with its cited unused record; the readout's `Status:` is non-PENDING. An `outside` or `unavailable` read is recorded as such and is not a failure of this row; CAP6.2 folds it honestly.

## DEFERRALS rule

A row that runs before this completes opens a `DF-*` row in `DEFERRALS.md` — why deferred = "HUMAN-H18 pending · withholds: round closure totals", unblocked by = "HUMAN-H18 · owner: operator · trigger: ≥ 24 h after <window> TEARDOWN", how to verify — and reports "pending", never a fabricated pass. A second deferral needs an operator scope/date/ADR decision.

## Notes

- Contributes to REQ-XH-110 (owner CAP6.2): the closure recompute uses the billed figure wherever an entry exists.

> Consistency fix 2026-11-07: under GATE-G30 `end_after_w2`, W4 never provisions, so its entry is `N/A` citing R6.47's `unused{scope_end}` record; R6.47 still issues the project deletion, whose receipt UTC is cited (§3.9 step 7). (item 11)

- REQ-XH-008's "GATE-PROV → PROV → runs → TEARDOWN → HUMAN-reconcile" is realized as the ordered per-window subsequence (plan tradeoff 10; confirmed at GATE-G20).
- Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) (X-02, X-03, X-05) and the manifest operating rules.

> Consistency fix 2026-11-07: `N/A` here is a per-window entry token (window did not run); the row's `Status:` is non-PENDING once all five entries are recorded. HUMAN-H15 uses `NOT_NEEDED` and HUMAN-H17 `N/A`/`DECLINED`, all R6.1 tokens. (item 16)
