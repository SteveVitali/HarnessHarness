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
# HUMAN-H13 — Configure the GCP billing alert before the first paid compute

- **Sequence:** 288 of 325 · **Phase:** Round 6 · **Kind:** human
- **Block:** W1 · **Alias:** HUMAN billing-alert
- **base_branch:** current checkout
- **Depends on:** R6.30
- **Blocks:** GATE-G25 (PROV-W1) and, through it, every first-pass window (W1, W2, W3, Wc, W4)
- **Owner:** operator
- **Scheduled:** trigger — after R6.30 lands (SELECT commit, W1 declaration and quote) and before GATE-G25 is presented; the actual UTC is recorded at completion (`date -u +%FT%TZ`)
- **Withheld until done:** every window (spec §10.3 HUMAN table) — no PROV-W1 presentation, no provisioning, no paid compute
- **Deferrals so far:** 0
- **Readout:** `docs/build/readouts/HUMAN-H13.md` (create from `_TEMPLATE.md` at presentation; no pre-filled readout)

> **Operator work — NOT an `implement-spec` input.** Only the operator can configure the billing
> account, its budget and the controller's credentials. An operator or authorized human record
> supplies the decision; an agent must not sign or assume silence is approval. The row is never
> skipped by default.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10), HUMAN table:

> | GCP billing alert | operator | before PROV-W1 | budget amount and 50/90/100 % thresholds recorded (first-pass default amount: the $20 compute authorization, so the first alert fires at $10; Q-X38); recipients `provided: yes` | every window |

Q-X38 (Appendix B, recorded default — a recorded default is never an operator answer):

> pending — default: STOP dead-man; D from the authorized quote; teardown across gate waits; residual-disk h_r ≤ 6 h; the pre-authorized restart of Q-X60; billing alert at the compute authorization (first pass: $20, at 50/90/100 %); console-read reconciliation

REQ-XB-219 (§3.11, first sentence):

> GCP credentials, SSH keys and billing-account ids SHALL never enter a committed record (`provided: yes/no` only).

## Checklist

- [ ] Budget configured on the billing account of the pass's one GCP project (REQ-XH-120): the **amount** is recorded (first-pass default $20 = the compute authorization, Q-X38); any other amount is recorded verbatim with the operator's words.
- [ ] Thresholds 50 / 90 / 100 % configured and recorded (under the default, the first alert fires at $10).
- [ ] Alert recipients configured — `recipients provided: yes` (no address recorded).
- [ ] Billing account linked — `billing account provided: yes`. The billing-account id is never written to the readout, LEDGER, any receipt or log (REQ-XB-219; plan Phase-4 record SR-20, reviewer option rejected).
- [ ] Controller `gcloud` authentication done on the operator's controller — `gcloud auth provided: yes` (no account name, project number or token recorded; X-05).
- [ ] Readout records owner, completion UTC, amount, thresholds and the three `provided: yes` fields; matching PHASE LOG entry (XO-06).

## Exit criterion

The readout `Status:` leaves `PENDING` only when every checklist item is recorded (the completion
token follows R6.1's HUMAN readout vocabulary). A partly configured alert is not completion. A
`DEFERRED` status citing a DF id would land the row for the guard but withholds every window, so
GATE-G25 is not presentable under it; completion is the only route to PROV-W1.

## DEFERRALS rule

No code row depends on this marker; only GATE-G25 follows it. A deferral needs a `DF-*` row naming
owner, trigger and the withheld claim ("every window"); a second deferral needs the operator's
explicit words (keep with a new date, amend the spec, or waive by ADR). Never silently passed.

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 and the
manifest operating rules. X-02 (the alert is a notifier, not an enforcement mechanism — enforcement
stays the dead-man ceilings and teardown deadline), X-05 (`provided: yes/no` only). No code branch
is created by a HUMAN row.

## Notes

- Owns no REQ id (coverage register rule O3). Readout tokens come from R6.1 (shared-decision owner).
- The per-window billing reads ("Reconcile window Wn") are consolidated in HUMAN-H18 before the tail; this row covers only the alert.
