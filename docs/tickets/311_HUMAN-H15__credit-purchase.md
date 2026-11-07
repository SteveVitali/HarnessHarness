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
# HUMAN-H15 — Ollama credit purchase, only if the FP-E quote needs it

- **Sequence:** 311 of 325 · **Phase:** Round 6 · **Kind:** human
- **Block:** W4 · **Alias:** HUMAN Ollama credit purchase (only if needed)
- **base_branch:** current checkout
- **Depends on:** R6.46
- **Blocks:** GATE-G30 (FP-E)
- **Owner:** operator
- **Scheduled:** trigger — after R6.46 lands, when its FP-E quote states a purchase need; otherwise recorded `NOT_NEEDED` at once; actual UTC recorded (`date -u +%FT%TZ`)
- **Withheld until done:** W4 arms beyond what the included credit supports (spec §10.3)
- **Deferrals so far:** 0
- **Readout:** `docs/build/readouts/HUMAN-H15.md` (create from `_TEMPLATE.md` at presentation)

> **Operator work — NOT an `implement-spec` input.** A purchase is the operator's act and the
> operator's money. An agent never buys, never infers the need, and never records a payment detail.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10), HUMAN table:

> | Ollama credit purchase (only if the FP-E quote needs it) | operator | before FP-E | amount purchased (≤ $10, Q-X54), `provided: yes` for the payment method, the recorded treatment of unused purchased credit (Q-X63), and A_eff re-declared from it (REQ-XH-120) | W4 arms beyond what the included credit supports |

Cited decisions, verbatim from Appendix B (recorded defaults are never operator answers):

> | Q-X54 | First-pass $100 split at the `high` pin (RB-4/RB-5/RA-11) | "Model ≤$70, compute ≤$20, $10 reserve (Recommended)" — spend above the $60 included credit needs purchased Ollama credit, a HUMAN step at the spend gate only if the measured quote requires it; the optional variant arm runs only if the quote leaves room | answered; §10.1.1, §10.1.5 |

> | Q-X63 | Accounting for purchased Ollama credit (RB-20) | pending — ledger default (verbatim): "Any purchased Ollama credit counts in full against the $100, used or not". Spec specifics: it counts against the model line too; buy only what the FP-E quote needs (≤ $10); A_eff includes it (REQ-XH-120) | FP-E |

§10.1.5 (verbatim): "Purchased credit (at most $10) is bought only if the FP-E quote needs a root above what the included credit supports (Q-X54). FP-E records A_eff and recomputes the root (REQ-XH-120)."

## Checklist

- [ ] Read R6.46's FP-E quote: purchase need and amount, as quoted from cost fields only. If the quote states no need, record `Status: NOT_NEEDED` citing the quote id and stop here.
- [ ] If needed: credit purchased, amount recorded (≤ $10, Q-X54; never more than the quote's need), payment method `provided: yes` (no card, invoice or account detail recorded).
- [ ] The treatment of unused purchased credit recorded (Q-X63 recorded default: counts in full against the $100, used or not — confirmed or changed by the operator; GATE-G30 cites it).
- [ ] A_eff re-declared from the purchase (A_eff = min(A, included + purchased), REQ-XH-120) for GATE-G30's recomputed root; A itself is unchanged (a purchase is never new authorization).

## Exit criterion

Lands at `NOT_NEEDED` (no purchase need in the quote), or when the purchase, its amount, the
`provided: yes` payment field, the Q-X63 treatment and the re-declared A_eff are all recorded. A
`DEFERRED` status (with DF id) leaves the arms beyond included credit withheld at FP-E.

## DEFERRALS rule

No code row depends on this marker; GATE-G30 follows it. A deferral names owner, trigger and the
withheld claim (W4 arms beyond the included credit); a second deferral needs the operator's explicit
words. Never silently passed.

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 and the
manifest operating rules; X-02 (purchased credit counts in full, Q-X63; whole first pass ≤ $100,
Q-X47/Q-X50), X-05. No code branch is created by a HUMAN row.

## Notes

- Owns no REQ id. Readout tokens (`NOT_NEEDED`, `DEFERRED` + DF id) from R6.1.
- Q-X63 is decided at FP-E (Appendix B); this row records the operator's treatment as stated at purchase time, and GATE-G30 confirms it — resolve any difference at GATE-G30, never here.
