# HUMAN-H15 readout
Status: NOT_NEEDED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval. `NOT_NEEDED` records the FP-E quote's own `purchase_decision` — no purchase act exists to sign.

## Criterion (verbatim)

Spec §10.3 HUMAN table:

> | Ollama credit purchase (only if the FP-E quote needs it) | operator | before FP-E | amount purchased (≤ $10, Q-X54), `provided: yes` for the payment method, the recorded treatment of unused purchased credit (Q-X63), and A_eff re-declared from it (REQ-XH-120) | W4 arms beyond what the included credit supports |

Cited decisions, verbatim from Appendix B:

> | Q-X54 | First-pass $100 split at the `high` pin (RB-4/RB-5/RA-11) | "Model ≤$70, compute ≤$20, $10 reserve (Recommended)" — spend above the $60 included credit needs purchased Ollama credit, a HUMAN step at the spend gate only if the measured quote requires it; the optional variant arm runs only if the quote leaves room | answered; §10.1.1, §10.1.5 |

> | Q-X63 | Accounting for purchased Ollama credit (RB-20) | pending — ledger default (verbatim): "Any purchased Ollama credit counts in full against the $100, used or not". Spec specifics: it counts against the model line too; buy only what the FP-E quote needs (≤ $10); A_eff includes it (REQ-XH-120) | FP-E |

§10.1.5 (verbatim): "Purchased credit (at most $10) is bought only if the FP-E quote needs a root above what the included credit supports (Q-X54). FP-E records A_eff and recomputes the root (REQ-XH-120)."

## Evidence (per item: source, `date -u` of the read, layer reached)

- Purchase need — `hh-fp-e/1` `docs/build/reports/2026-11-19-fp-e.json` sha256 `d6fa1dab0538bf759581b9524f5184a6f74a402d73128eac82b904e6482b60da` (PR #408, R6.46 commit `62e6a392`) — read 2026-11-19T00:34Z — layer: `engineered` (committed record, cost fields only).
- The quote's `credit` member: A 70,000,000 µ$ · A_eff 60,000,000 µ$ (min(A, included credit)) · root_cap 48,000,000 µ$ · consumed 4,072,434 µ$ · needed_root 7,472,434 µ$ · **`purchase_decision: NOT_NEEDED`**, `purchase_need_micros: 0`, `purchased_micros: 0`.

<!-- agent-drafted:begin sha256=44157ac853ce85dca980dee5b1ad14cc7f1c7f76e393dfac5f20174eae4901a1 -->
## Decision content for the operator

**No purchase.** R6.46's committed FP-E quote (`hh-fp-e/1`, `docs/build/reports/2026-11-19-fp-e.json`,
sha256 `d6fa1dab0538bf759581b9524f5184a6f74a402d73128eac82b904e6482b60da`) computes
`purchase_decision: NOT_NEEDED` — needed root 7,472,434 µ$ (consumed 4,072,434 + 3,400,000 margin
m=.20 floor) fits under the $48 root that A_eff = min(A $70, included credit $60) supports. No
W4 arm is withheld for want of purchased credit (the withhelds are `admission:AG3`, `budget`,
`window_deadline` — all cost/wall facts, none a credit shortfall). Q-X63's recorded default
stands trivially: purchased credit is 0, "any purchased Ollama credit counts in full against the
$100, used or not" — nothing was bought, nothing counts. A_eff stays 60,000,000 µ$ as declared;
A unchanged. `Status: NOT_NEEDED` is recorded here per the H15 checklist's first item — the
quote states no need, so the ticket stops. No payment, no card detail, no A_eff re-declaration is
owed; GATE-G30 presents the quote itself next.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- No purchase act occurred; `NOT_NEEDED` is the ticket's own recorded exit (checklist item 1:
"if the quote states no need, record `Status: NOT_NEEDED` citing the quote id and stop here").
Recorded by Devin (SWE-2 High, coordinator) 2026-11-19T00:34Z — the agent does not sign. -->
