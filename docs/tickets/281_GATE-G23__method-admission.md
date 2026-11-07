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
# GATE-G23 — Method admission: protocol, quote, estimators, schedule randomization and AR-33's departures

- **Sequence:** 281 of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** METHODS · **Alias:** GATE method-admission (spec §10.3 "Method admission")
- **base_branch:** current checkout
- **Depends on:** R6.27
- **Readout:** `docs/build/readouts/GATE-G23.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** The chain stops here until the operator dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite is `blockedOn`. Agents never sign or infer approval, and silence is not approval.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10) gate table, row "Method admission":

> | Method admission | first | D1-b | independent numerical and routing review record; known-answer tests; battery records (stopped scenarios never counted), including the first-pass T = 10 grid; admit or refuse `comparative_protocol/1`, `comparative-quote/1`, the descriptive estimators, `slot_schedule_randomization`, and AR-33's first-pass departures (shared baseline, family of one, T = 10 / H = 3 below the §4.5 precision target) |

Spec [§6.4](../experiments-spec.md#s6), the requirements the gate decides over:

> | **REQ-XD-023** | New method pieces SHALL be implemented only after an independent numerical and routing review and an operator method-admission gate, following §4.3's route (AR-17). | Review record and gate entry precede first use. · `AC-6.4-2` · D |
>
> | **REQ-XD-024** | A comparative extension of `sim_battery` (H ∈ {2,4,8}, T ∈ {20,37,48}, k = 5, mixtures `board`/`easyband`/`hard`, σ ∈ {0, .75}, interaction, informative infra missingness, flagged-pass vetoes, shared-shock rounds; ≥ 2,000 datasets per scenario; frozen seed rule) SHALL run on the exact pipeline before the first confirmatory subject. For the first pass the grid SHALL add T = 10 with H = 3 and H = 2 (one arm dropped), and the pre-registration SHALL quote from it P(m ≥ 7 non-tied tasks), the power table and the median half-width (§10.1.3). | Battery records retained; stopped scenarios never count as admission. · `AC-6.4-3` · E |

Spec §6.5:

> One Holm family per tranche is an explicit departure from §4.3's cross-strata family. It needs a method-admission decision (AR-17; Q-X40, recorded default: accept).

Spec [§10.5](../experiments-spec.md#s10) register rows:

> | AR-17 | Method admission of the new estimators; one Holm family per tranche | ADR via §4.3 route + gate | D1-b | first |
>
> | AR-33 | First-pass design and departures (Q-X47…Q-X55): one interleaved window with coupled O/H pairs; the shared phase-4 baseline and its concurrency with L2 (REQ-XD-215, REQ-XD-220); the family-of-one P4 contrast; a T = 10, H = 3 family below the §4.5 precision target; ε = ±.20 with the scoped label; the cost-only checkpoint and arm priority | ADR via the §4.3 route; listed at the method-admission gate | FP-PREREG | first |

[Evidence spec §4.3](../evidence-and-adoption-spec.md#s4), AC-4-2 (the route AR-17 follows), final sentence:

> This accepts an apparatus, never a favorable treatment effect.

## Pre-registered thresholds

**Record set presented** (each committed before presentation, from R6.27's method dossier and its predecessors; the readout cites each by path and sha256):

1. The independent numerical and routing review record over `comparative_protocol/1`, `comparative-quote/1` (R6.18), the new estimators (R6.19) and the battery's routing, with its independence basis and every finding's disposition.
2. The known-answer tests of `AC-6.4-1` (constant p gives f = 2p(1−p); the swap−rerun identity; $/solve at 0 solves refuses; the single-arm interval equals the contrast machinery's per-arm marginal), green, with run references.
3. The battery records of REQ-XD-024 — the full grid and the first-pass T = 10 grid with H = 3 and H = 2, ≥ 2,000 datasets per scenario, frozen seed rule — with stopped scenarios listed and **never counted** toward admission.
4. AR-17 and AR-33 as proposed ADRs.

**Decision content — one verbatim disposition per item, each `admit` | `refuse`:**

| item | source |
|---|---|
| (a) `comparative_protocol/1` | §10.3 row; REQ-XD-020, REQ-XD-021 |
| (b) `comparative-quote/1` | §10.3 row; REQ-XD-050, REQ-XH-030 |
| (c) the descriptive estimators (rerun flip, harness-swap flip, $/solve, tied-with-leader) and the other new estimator pieces AR-17 names (the single-arm finite-manifest rate) | §10.3 row; §6.4; AR-17 |
| (d) `slot_schedule_randomization` as the sharp-null exchangeability argument | §10.3 row; §6.4 table |
| (e) one Holm family per tranche (Q-X40 recorded default: accept — confirmed only at this gate) | §6.5; AR-17 |
| (f) AR-33's first-pass departures: shared baseline, family of one, T = 10 / H = 3 below the §4.5 precision target (and the other departures AR-33's §10.5 row lists, which this gate lists) | §10.3 row; §10.5 AR-33 |

**Not decided at this gate:** any spend (none is authorized); ε (fixed at ±.20 by Q-X53); the V5 level (Q-X56, at FP-S); arm priority (Q-X61, at FP-S); the fallback trigger (Q-X62, at FP-S); Q-X57's single-contrast departure is cited from the ledger, not answered here. No outcome data exists at this gate; battery figures are simulation outputs labelled UNVERIFIED for real tasks, and thresholds are never moved to fit them.

## Disposition

Record the operator's exact words, UTC, channel, the disposition of each item (a)–(f) verbatim, any scope or expiry/void conditions, and explicit confirmation of the agent-draft hash; append the readout signature and the matching LEDGER GATE DECISIONS row. Old acceptance never substitutes. A scoped decision names carried unmet predicates and compensating controls (round-5 GATE-G9 precedent). A refused item is not admitted for any consumer: R6.30, R6.33 and every quote or analysis row refuse typed for it, the chain stops here, and any repair is a scoped insert authorized by the operator's recorded words — never inferred. Use R6.1's gate-disposition templates and readout tokens.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `docs/tickets/DEFERRALS.md` scoped to the METHODS block remains undispositioned (including R6.27 review findings recorded there). Resolve it, or obtain an explicit operator disposition with owner, trigger and withheld claims.

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 and the manifest operating rules (X-03: no gate is pre-answered; X-07: no outcome field is read). No code branch is created by a gate.
