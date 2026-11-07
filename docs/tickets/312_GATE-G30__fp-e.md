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
# GATE-G30 — FP-E: authorize the W4 main window, its hosted-only form, or the end of the pass after W2

- **Sequence:** 312 of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** W4 · **Alias:** GATE FP-E
- **base_branch:** current checkout
- **Depends on:** R6.46
- **Readout:** `docs/build/readouts/GATE-G30.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10), gate table (presented after "G-L1 (or the fallback trigger) + W2 settlement"):

> | **FP-E** main window | first | G-L1 (or the fallback trigger) + W2 settlement | W2-measured $/trial and wall time per arm (cost fields only); the cost projection and arm inclusion by the priority rule, V5 only if room remains; the wall-time projection, D_max and any wall-time drop (REQ-XH-126), stating whether Wc's reservation is released; W2's queue-wait measurement and C (REQ-XD-062); per-arm cap values (µ$) under expected-cost admission (REQ-XH-117); m (= .20, restated); purchased-credit need and amount (Q-X54, Q-X63), A_eff and the recomputed root (REQ-XH-120); the carried root pool (REQ-XH-120); W4 quote, D and restart; exclusivity acknowledgement; any PP0 hold's disposition (REQ-XH-127); the hosted-only form if the fallback triggered |

Cited requirements, verbatim:

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

> | **REQ-XH-112** | First-pass spending SHALL follow the windows and steps of §10.1.4. Every continuation rule SHALL read only cost fields (measured $/trial, measured wall time per trial, spend to date, remaining root, compute ceiling, remaining window time), never an outcome field. FP-E SHALL include arms by the priority order from the W2-measured cost projection of §10.1.4 and the wall-time projection of REQ-XH-126. Inside W4, after round 2 and at each later round boundary, the runner SHALL re-project the remaining rounds with the same formulas and SHALL withhold the remaining slots of the lowest-priority arms until the rest fits (`withheld{budget}` for cost, `withheld{window_deadline}` for wall time), never a protected arm while an unprotected arm remains. The checkpoint is automatic and its readout carries cost fields only. Rounds already run stay in the record. [Synthesis-allocated from Q-X47; revised O2 per Q-X52, RB-7, RB-8; O3 per RD-2.] | Continuation fixtures: an input manifest containing an outcome field refuses; a projected breach at the round-2 checkpoint withholds V5's remaining slots, then Claude Code's, then Pi's, in that order; a protected-pair breach withholds every remaining slot; a round-boundary fixture withholds V5 before any HH-native slot; a wall-time shortfall at a round boundary withholds in the same order. · `AC-10.1-3` · E |

> | **REQ-XH-117** | Each unprotected first-pass arm (Pi, Claude Code, V5) SHALL have a per-arm cap in integer µ$, tabulated in the pre-registration as a formula (planned slots × p̂_h of §10.1.4) and filled with values at FP-E from cost fields only. An arm's slot is admitted on expected cost: only when consumed_h (its finished slots' metered spend) + Σ over its in-flight slots of max(p̂_h, consumed-to-date) + p̂_h ≤ cap_h; otherwise the arm's remaining slots are `withheld{budget}`, with no cell and never a scored refusal. The slot slice s (REQ-XC-126) is reserved against the root only, whose feasibility check already subtracts C × s, so an arm's in-flight overshoot of its cap is bounded by its in-flight slots × (s − p̂_h) and is covered by the root. The protected arms, original and HH-native mini-swe-agent, are bounded only by the root cap and REQ-XH-112's checkpoint rule (Q-X61). A dropped arm stays in its Holm family as an unavailable member; a protected-pair truncation withholds P2-FP, P3-L2 and P4-FP primary inference with typed reasons. [Synthesis-allocated from Q-X47; revised O2 per RA-2, RB-7; O3 per RD-3: a uniform slice charged against a cheap arm's cap stranded its last slots.] | A simulated over-cap arm is withheld at slot admission, with overshoot bounded by its in-flight slots × (s − p̂_h); a simulated arm whose every trial costs exactly p̂_h / 1.8 completes all 50 slots; the family test retains the unavailable member; the pre-registration carries the µ$ cap table. · `AC-10.1-8` · E |

> | **REQ-XH-120** | For REQ-XC-112, REQ-XB-212 and REQ-XH-013, the first pass SHALL be one paid phase with one GCP project for all its windows and one model authorization. The first-pass root cap SHALL be A_eff × (1 − m), with A_eff = min(A, the model credit available to the pass: included credit as Q-X21's ruling makes it available, plus credit purchased) and m = .20 (REQ-XH-005). FP-E SHALL re-declare A_eff after any purchase and recompute the root; its readout records A, A_eff, the purchase and m. Each window's proxy root pool SHALL equal the first-pass root cap minus Σ prior windows' consumed spend and their orphaned calls at `bounded.hi`, read from the harvested proxy ledgers; a window whose proxy ledger is unharvested or unverified SHALL block every later paid window. FP-S SHALL require automatic usage billing confirmed off in the Ollama HUMAN row, or record verbatim that the model line is meter-enforced only (Q-X21). [Synthesis-allocated, O2 RB-6, RA-27; revised O3 per RD-7, RD-8.] | An A_eff fixture (A = $70, no purchase, $60 included credit) yields a $48 root; carry-over fixture: W4's root equals the cap minus W2's consumed and orphans; an unharvested-ledger fixture refuses the next window; receipts show one project id across the pass; the FP-S readout carries the billing-setting field. · `AC-10.1-11` · E |

> | **REQ-XH-123** | The first pass SHALL carry Q-X52's pre-declared fallback: if G-L1 fails on both gate partitions, or has not passed by the trigger recorded at FP-S (Q-X62), W4 SHALL run the three hosted arms alone (phase 2 only), and P3-L2 and P4-FP SHALL be `withheld{l1_not_passed}` and moved to a later pass. The fallback is decided at FP-E from the trigger record, never from an outcome field. [Synthesis-allocated, O2 RB-14; Q-X52.] | Fallback fixture: a trigger record yields a hosted-only W4 schedule and both typed withholdings; the FP-E readout cites the trigger. · `AC-10.1-14` · E |

> | **REQ-XH-126** | First-pass W4 wall time SHALL be governed by a pre-registered cost-only rule. (a) W4's W_p SHALL include the declared retry allowance and the artifact load: W_p = Σ_h (planned slots_h + ⌈0.1 × planned slots_h⌉) × t̂_h / C + overhead + load, with t̂_h of §10.1.4. (b) At FP-E, W4's D SHALL be at most D_max, the largest D that keeps Σ of compute ceilings ≤ $20 (Q-X54). In that sum, completed windows count at their billed figure where a reconcile read exists and at their ceiling otherwise. When Wc has not been authorized, its ceiling is released except one restart-overflow reservation (r × 1 h + S·r_disk·h_r). If W_p > D_max − the REQ-XB-128 cut-off, FP-E SHALL drop arms before W4 starts, in the order V5, then Claude Code, then Pi, until it fits; the protected pair is never dropped while an unprotected arm remains. (c) At each W4 round boundary, the runner SHALL project the remaining wall time with W4-measured t̂ against the cut-off and SHALL withhold remaining slots by the same order. (d) A slot not admitted before the cut-off is `withheld{window_deadline}`, and an O/H pair is admitted together or withheld together, never split. Wall time is a cost field under REQ-XH-112's firewall. [Synthesis-allocated, O3 RD-2.] | Fixtures: a W2 measurement giving W_p > D_max − cut-off drops V5, then Claude Code, then Pi at FP-E; a round-boundary wall-time shortfall withholds in the same order; a cut-off fixture types both slots of an unadmitted pair `withheld{window_deadline}` and never just one; the D_max recompute releases Wc only when it is unauthorized and keeps the overflow reservation. · `AC-10.1-17` · E |

> | **REQ-XH-127** | A first-pass PP0 hold SHALL have a declared disposition. A hold is ≥ 1 of 3 PP0 trials infrastructure-failed, or a failed AG3/AG5/AG6/AG8 check. The disposition must be declared because a repair is a new participant version (REQ-XC-330) and no first-pass window offers a paid re-smoke (Wc is $0 model). (a) A held Pi or Claude Code arm is `withheld{admission:AG<n>}` for the pass, naming the failed gate, or `withheld{admission:pp0_infra_hold}` for an infrastructure hold with no failed gate. It drops from FP-E and stays in P2-FP's Holm family as an unavailable member. (b) A held original mini-swe-agent withholds the corpus, so G-L1 cannot pass. FP-E SHALL then present a recorded scope decision whose default ends the pass after W2, with every first-pass claim withheld by the hold's typed reason; a repaired arm is a later pass. (c) A development-task exclusion that empties its difficulty cell SHALL be replaced from the adjacent cell's committed order (easy or hard → medium), re-proofed under REQ-XH-116, and disclosed in the pre-registration. [Synthesis-allocated, O3 RD-14.] | Fixtures: a held Pi arm is absent from the FP-E schedule while its family member is retained as unavailable; a held mini-swe-agent yields the FP-E scope-decision record with the typed withholdings; an empty-cell exclusion draws from the adjacent cell, and the disclosure is present. · `AC-10.1-18` · E |

> | **REQ-XD-062** | Per-trial proxy queue wait SHALL be recorded and reported beside timeout rates. The pilot calibrates the in-flight limit (provisional: median queue wait ≤ 10 % of mean agent time). | Receipt field; the quote uses it. · `AC-6.8-3` · E |

Cited operator decisions, verbatim from Appendix B (recorded defaults are never operator answers):

> | Q-X54 | First-pass $100 split at the `high` pin (RB-4/RB-5/RA-11) | "Model ≤$70, compute ≤$20, $10 reserve (Recommended)" — spend above the $60 included credit needs purchased Ollama credit, a HUMAN step at the spend gate only if the measured quote requires it; the optional variant arm runs only if the quote leaves room | answered; §10.1.1, §10.1.5 |

> | Q-X61 | First-pass arm priority and reference-arm protection under the cap (RB-7) | pending — ledger default (verbatim): "Original and HH-native mini-swe-agent protected from cost-based dropping; truncation order V5 → Claude Code → Pi". Spec specifics: a protected-pair truncation withholds P2-FP, P3-L2 and P4-FP with typed reasons (REQ-XH-112, REQ-XH-117). REQ-XH-126's wall-time rule uses the same order; that rule is spec text, not part of this default | FP-S (pre-registration) |

> | Q-X63 | Accounting for purchased Ollama credit (RB-20) | pending — ledger default (verbatim): "Any purchased Ollama credit counts in full against the $100, used or not". Spec specifics: it counts against the model line too; buy only what the FP-E quote needs (≤ $10); A_eff includes it (REQ-XH-120) | FP-E |

## Pre-registered thresholds

The readout carries, from R6.46's FP-E quote and the committed records, cost fields only:

1. W2-measured $/trial and wall time per arm; W2's queue wait and C (REQ-XD-062).
2. The cost projection p̂_h and **arm inclusion by priority** (Q-X61 as ruled at FP-S; recorded default: original and HH-native mini-swe-agent protected; truncation V5 → Claude Code → Pi; V5 only if room remains after every higher arm, Q-X54).
3. The wall-time projection, D_max and any wall-time drop (REQ-XH-126), stating whether Wc's reservation is released.
4. Per-arm cap values in µ$ under expected-cost admission (REQ-XH-117); m = .20 restated (REQ-XH-005).
5. The purchased-credit need and amount and HUMAN-H15's readout (`NOT_NEEDED` or the purchase); the Q-X63 treatment; A, A_eff and the recomputed root; the carried root pool (REQ-XH-120).
6. W4 quote, D and the one harvest-only restart (REQ-XB-224); W4 is the last window (its fleshing includes GCP project deletion).
7. Exclusivity acknowledgement for W4 (REQ-XH-013) and the credit-period fields (REQ-XH-007) — contributor legs of GATE-G26's owned ids.
8. Any PP0 hold's disposition (REQ-XH-127); the hosted-only form if GATE-G29 recorded `fallback` (REQ-XH-123), citing the trigger record — never an outcome field.
9. Authorizations (REQ-XH-004): the model line within FP-S's A (no new model authorization; a reset or purchase is never new authorization), the W4 compute authorization separately, the quote id and the hard-cap formula.
10. Previous windows closed: W2 settlement status (REQ-XC-127), W3 and Wc indices `archive_verified` or their `n/a{…}` records (REQ-XH-102), every prior proxy ledger harvested and verified (REQ-XH-120).
11. Every inclusion or drop labelled "cost-triggered; cost correlates with failure length" (§10.1.4 firewall).

**Pre-registered dispositions** (each recorded `PASSED — disposition <token>; basis: <verbatim>`; basis never contains the tokens `FAILED` or `NOT PASSABLE`):

- `run{full}` — GATE-G29 recorded `full`; W4 runs the arms the quote includes; R6.47 is fleshed for the W4 body.
- `run{hosted-only}` — GATE-G29 recorded `fallback`; W4 runs original mini-swe-agent, Pi and Claude Code alone; P3-L2 and P4-FP stay `withheld{l1_not_passed}`.
- `end_after_w2` — REQ-XH-127(b)'s recorded scope decision (a held original mini-swe-agent); the pass ends after W2; R6.47 is fleshed in place to `unused{scope_end}` with `n/a` upload and settlement; every first-pass claim withheld by the hold's typed reason.

Only an operator refusal stops the chain.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to W2's settlement, the prior
windows' archives or the FP-E quote remains (DEFERRALS rule 4); never silently passed.

## Disposition

Record the operator's exact words, UTC and channel, the disposition and basis verbatim, every value
in items 1–11, and confirmation of any agent-drafted block's sha256; append the readout Signature and
the matching LEDGER GATE DECISIONS row. A scoped `decompose-spec` insert then fleshes R6.47 per the
disposition (X-03, Q-X58). The W4 checkpoint after round 2 is automatic and pre-registered: no gate.

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 and the
manifest operating rules; X-02 (whole first pass ≤ $100, Q-X47/Q-X50; Σ authorized ≤ $90), X-03,
X-07. No code branch is created by a gate.

## Notes

- Owns no REQ id; contributes readout legs of REQ-XH-004 (R6.1), REQ-XH-007 and REQ-XH-013 (GATE-G26), REQ-XH-120 (R6.11).
