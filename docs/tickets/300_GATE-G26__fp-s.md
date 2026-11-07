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
# GATE-G26 — FP-S: authorize first-pass model spend and the W2 window

- **Sequence:** 300 of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** W2 · **Alias:** GATE FP-S
- **base_branch:** current checkout
- **Depends on:** HUMAN-H14, R6.39
- **Readout:** `docs/build/readouts/GATE-G26.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10), gate table:

> | **FP-S** first-pass spend | first | FP-PREREG + Ollama/HF HUMAN rows + the E2-T8 rung record (Q-X62) | Q-X21 (automatic usage billing off, or meter-only recorded; exclusivity; post-reset allowance); verbatim model authorization A ≤ $70 for the pass; m (= .20 for the whole pass, REQ-XH-005); A_eff at FP-S (REQ-XH-120); the slice rule (REQ-XC-126); the per-arm cap formula with expected-cost admission (REQ-XH-117) and arm priority (Q-X61); the wall-time rule (REQ-XH-126); the fallback trigger (Q-X62, quoted from the ledger row); W2 quote, D and restart; pre-registration hash (tasks, arms, effort `high`, ε = ±.20, the V5 level, claims and their expected precision); credit period (REQ-XH-007); the per-window archive-upload authorization (Q-X35); Q-X47 and Q-X54 cited |

Cited requirements, verbatim:

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

> | **REQ-XH-005** | The proxy root cap SHALL be authorized × (1 − m), with m declared at each spend gate: default m = 0.20 at the first spend gate of a pass (the pilot gate in later passes; FP-S in the first pass); later default m = 0.10 once a `settled` reconciliation from REQ-XC-127 exists (C-15; Q-X36). The first pass keeps m = .20 at FP-S and at FP-E, under Q-X36's recorded default "m = .20 first pass/pilot", because its $10 reserve and the A_eff rule of REQ-XH-120 are sized on it; the .10 default can first apply at a later pass's first spend gate. Per-harness caps are slices under the root. | Proxy budget property test with m read from the gate record; readout field `m`. · `AC-4.4-8` · E |

> | **REQ-XH-007** | Each pass's spend-gate readouts (the pilot gate in later passes; FP-S and FP-E in the first pass) SHALL record the provider credit period (start and end UTC; currently 2026-10-22T16:30:34Z → 2026-11-22T16:30:34Z) and whether the authorization covers one period or several. A provider reset is never new authorization. | Readout fields; review. · `AC-4.4-9` · D |

> | **REQ-XH-013** | No two paid collections SHALL run concurrently on the model account; the operator's own use of the account counts as overlap. Each paid window's gate SHALL record the operator's exclusivity acknowledgement (C-19). | Readout field; schedule check across window receipts. · `AC-4.4-10` · D |

> | **REQ-XH-120** | For REQ-XC-112, REQ-XB-212 and REQ-XH-013, the first pass SHALL be one paid phase with one GCP project for all its windows and one model authorization. The first-pass root cap SHALL be A_eff × (1 − m), with A_eff = min(A, the model credit available to the pass: included credit as Q-X21's ruling makes it available, plus credit purchased) and m = .20 (REQ-XH-005). FP-E SHALL re-declare A_eff after any purchase and recompute the root; its readout records A, A_eff, the purchase and m. Each window's proxy root pool SHALL equal the first-pass root cap minus Σ prior windows' consumed spend and their orphaned calls at `bounded.hi`, read from the harvested proxy ledgers; a window whose proxy ledger is unharvested or unverified SHALL block every later paid window. FP-S SHALL require automatic usage billing confirmed off in the Ollama HUMAN row, or record verbatim that the model line is meter-enforced only (Q-X21). [Synthesis-allocated, O2 RB-6, RA-27; revised O3 per RD-7, RD-8.] | An A_eff fixture (A = $70, no purchase, $60 included credit) yields a $48 root; carry-over fixture: W4's root equals the cap minus W2's consumed and orphans; an unharvested-ledger fixture refuses the next window; receipts show one project id across the pass; the FP-S readout carries the billing-setting field. · `AC-10.1-11` · E |

> | **REQ-XC-126** | Every slot's budget slice SHALL be at least the arm's P90 $/trial (the planning prior until ≥ 10 trials are measured) plus one worst-case call reservation w at the declared body-bytes bound (C2 F10: about $0.03 input + $0.066 output for a 200 KB request at the 131 072-token output cap, so w ≈ $0.10). Within one comparison every arm SHALL get the same slice, sized from its most expensive arm (first pass: s = 2 × the largest binding planning $/trial + w ≈ $0.85), so the per-trial budget is a common condition. A slot refusal while the slot's consumed-to-date is below s − w is premature and SHALL be `inconclusive{infra:budget_reservation}`, never scored; every refusal records `requested`/`available` and is reported per arm as a covariate. Root-cap feasibility checks SHALL count the in-flight slices (C × s), never mean costs. [Synthesis-allocated, O2 RA-2.] | Slice validator refuses a slice below P90 + w or unequal slices within a comparison; a premature-refusal fixture yields `inconclusive{infra:budget_reservation}`; a refusal at consumed ≥ s − w is scored `budget_refused`; the quote's feasibility check subtracts C × s. · `AC-4.4-11` · E |

> | **REQ-XH-117** | Each unprotected first-pass arm (Pi, Claude Code, V5) SHALL have a per-arm cap in integer µ$, tabulated in the pre-registration as a formula (planned slots × p̂_h of §10.1.4) and filled with values at FP-E from cost fields only. An arm's slot is admitted on expected cost: only when consumed_h (its finished slots' metered spend) + Σ over its in-flight slots of max(p̂_h, consumed-to-date) + p̂_h ≤ cap_h; otherwise the arm's remaining slots are `withheld{budget}`, with no cell and never a scored refusal. The slot slice s (REQ-XC-126) is reserved against the root only, whose feasibility check already subtracts C × s, so an arm's in-flight overshoot of its cap is bounded by its in-flight slots × (s − p̂_h) and is covered by the root. The protected arms, original and HH-native mini-swe-agent, are bounded only by the root cap and REQ-XH-112's checkpoint rule (Q-X61). A dropped arm stays in its Holm family as an unavailable member; a protected-pair truncation withholds P2-FP, P3-L2 and P4-FP primary inference with typed reasons. [Synthesis-allocated from Q-X47; revised O2 per RA-2, RB-7; O3 per RD-3: a uniform slice charged against a cheap arm's cap stranded its last slots.] | A simulated over-cap arm is withheld at slot admission, with overshoot bounded by its in-flight slots × (s − p̂_h); a simulated arm whose every trial costs exactly p̂_h / 1.8 completes all 50 slots; the family test retains the unavailable member; the pre-registration carries the µ$ cap table. · `AC-10.1-8` · E |

> | **REQ-XH-126** | First-pass W4 wall time SHALL be governed by a pre-registered cost-only rule. (a) W4's W_p SHALL include the declared retry allowance and the artifact load: W_p = Σ_h (planned slots_h + ⌈0.1 × planned slots_h⌉) × t̂_h / C + overhead + load, with t̂_h of §10.1.4. (b) At FP-E, W4's D SHALL be at most D_max, the largest D that keeps Σ of compute ceilings ≤ $20 (Q-X54). In that sum, completed windows count at their billed figure where a reconcile read exists and at their ceiling otherwise. When Wc has not been authorized, its ceiling is released except one restart-overflow reservation (r × 1 h + S·r_disk·h_r). If W_p > D_max − the REQ-XB-128 cut-off, FP-E SHALL drop arms before W4 starts, in the order V5, then Claude Code, then Pi, until it fits; the protected pair is never dropped while an unprotected arm remains. (c) At each W4 round boundary, the runner SHALL project the remaining wall time with W4-measured t̂ against the cut-off and SHALL withhold remaining slots by the same order. (d) A slot not admitted before the cut-off is `withheld{window_deadline}`, and an O/H pair is admitted together or withheld together, never split. Wall time is a cost field under REQ-XH-112's firewall. [Synthesis-allocated, O3 RD-2.] | Fixtures: a W2 measurement giving W_p > D_max − cut-off drops V5, then Claude Code, then Pi at FP-E; a round-boundary wall-time shortfall withholds in the same order; a cut-off fixture types both slots of an unadmitted pair `withheld{window_deadline}` and never just one; the D_max recompute releases Wc only when it is unauthorized and keeps the overflow reservation. · `AC-10.1-17` · E |

> | **REQ-XH-123** | The first pass SHALL carry Q-X52's pre-declared fallback: if G-L1 fails on both gate partitions, or has not passed by the trigger recorded at FP-S (Q-X62), W4 SHALL run the three hosted arms alone (phase 2 only), and P3-L2 and P4-FP SHALL be `withheld{l1_not_passed}` and moved to a later pass. The fallback is decided at FP-E from the trigger record, never from an outcome field. [Synthesis-allocated, O2 RB-14; Q-X52.] | Fallback fixture: a trigger record yields a hosted-only W4 schedule and both typed withholdings; the FP-E readout cites the trigger. · `AC-10.1-14` · E |

> | **REQ-XH-102** | Uploads SHALL run only from the operator's controller after the window's harvest manifest verifies (REQ-XB-220), using one operator upload tool whose identity and exact version AR-20 admits, recorded in the way AR-15 records `gcloud`. The HF write token SHALL be fine-grained and scoped to the one dataset repository, read at use from the git-ignored env file named by `HH_OPERATOR_ENV_FILE`, passed per invocation through the environment, never printed, written or persisted by a `login` command, and recorded only as `provided: yes/no` (REQ-XH-023). Uploads SHALL run per window, right after that window's harvest manifest verifies and before the next paid window's gate, each an execution act of that window's ARCHIVE-UPLOAD row; one authorization at the first spend gate of the pass (FP-S in the first pass) covers every window. [Synthesis-allocated, AR-20; revised O2 RA-7, RA-26.] | Tool-admission record cited by the upload receipt; the receipt's `provided` block; leak sweep over the receipt; each paid window's gate readout cites the previous window's `archive_verified` index. · `AC-4.10-3` · E |

Cited operator decisions, verbatim from Appendix B (recorded defaults are never operator answers):

> | **Q-X47** | First-pass spend ceiling (operator-initiated, received 2026-11-07 in the planning conversation, free text) | "Note that for the first pass, I don't want to spend more than $100 max. So please think carefully about how to tweak this so that it remains cheap. I'm okay with excluding certain models or harnesses or limiting in other ways that are reasonable. the purpose of the initial pass is to prove the evaluation capability as effectively as possible for as little cost as possible" | answered; §10.1, FP-1…FP-15 |

> | Q-X54 | First-pass $100 split at the `high` pin (RB-4/RB-5/RA-11) | "Model ≤$70, compute ≤$20, $10 reserve (Recommended)" — spend above the $60 included credit needs purchased Ollama credit, a HUMAN step at the spend gate only if the measured quote requires it; the optional variant arm runs only if the quote leaves room | answered; §10.1.1, §10.1.5 |

> | Q-X21 | Ollama credit mechanics: overage behaviour, post-reset allowance, exclusivity | pending — default: in a HUMAN row before FP-S the operator confirms automatic usage billing off (or the FP-S readout records the model line as meter-enforced only) and acknowledges exclusivity; the authorization states whether the post-reset allowance or purchased credit may fund A (FP-12; Q-X54) | FP-S gate (spend) |

> | Q-X35 | Raw receipt archive | "HF dataset, gated publish (Recommended)" — sha256 index committed; bytes to a Hugging Face dataset repo, private first, public only at a publication gate after contamination/secret scan (received 2026-11-06, planning conversation dialog) | answered; §4.10 |

> | Q-X36 | Settlement reserve | pending — default: rule C-15 (m = .20 first pass/pilot, .10 after a settled reconciliation) | ratification (rule); each spend gate (value) |

> | Q-X56 | First-pass phase-4 factor level (O1 recorded default; RA-3, RB-16) | pending — ledger default (verbatim): "First-pass optional phase-4 variant = V5 `rsn-drop` (spec v0.2 default)". Spec specifics: V5 is always active after turn 1 and the cheapest level; the alternative is V4 `obs-2.5k` | FP-S (pre-registration) |

> | Q-X61 | First-pass arm priority and reference-arm protection under the cap (RB-7) | pending — ledger default (verbatim): "Original and HH-native mini-swe-agent protected from cost-based dropping; truncation order V5 → Claude Code → Pi". Spec specifics: a protected-pair truncation withholds P2-FP, P3-L2 and P4-FP with typed reasons (REQ-XH-112, REQ-XH-117). REQ-XH-126's wall-time rule uses the same order; that rule is spec text, not part of this default | FP-S (pre-registration) |

> | Q-X62 | FP-S sequencing against the E2 chain, and the G-L1 fallback trigger (RB-14; Q-X46, Q-X52) | pending — ledger default (verbatim): "First-pass spend gate (FP-S) waits for the E2-T8 rung record; hosted-only fallback fires if G-L1 has not passed 14 days after FP-S is otherwise ready (14 is a synthesis placeholder)". Spec specifics (v0.3 re-synced to the ledger, RD-6; the v0.2 anchor "W2's teardown" and its rung-3 trigger are withdrawn): "otherwise ready" means every FP-S precondition except the E2-T8 rung record is met; a G-L1 failure on both gate partitions triggers Q-X52's fallback at once; an E2-T8 rung-3 record is not a trigger (REQ-XE-120's ADR route); once the trigger fires, FP-S no longer waits for the rung record (§10.1.4; REQ-XH-123) | ratification (the operator fixes the number); recorded at FP-S |

§10.1.4, the Q-X62 specifics (verbatim): "FP-S is "otherwise ready" when every FP-S precondition except the E2-T8 rung record is met." … "Once the trigger fires, FP-S no longer waits for the rung record, which only the HH-native arms need."

## Pre-registered thresholds

The readout carries each item below before the operator answers; values are presented, never pre-filled:

1. **Q-X21 confirmations.** Automatic usage billing off (HUMAN-H14 item 1), or the verbatim statement that the model line is meter-enforced only; the exclusivity acknowledgement for every paid window; whether the post-reset allowance or purchased credit may fund A.
2. **Model authorization** A ≤ $70 for the whole pass, verbatim (Q-X54); the W2 compute authorization stated separately, with the quote id and hard-cap formula (REQ-XH-004).
3. **m = .20** for the whole pass (REQ-XH-005; Q-X36 recorded default), restated.
4. **A_eff and root.** A_eff = min(A, included credit as Q-X21 makes it available + purchased); root = A_eff × (1 − m) — $56 at A_eff = $70, $48 at A_eff = $60 (REQ-XH-120 fixture).
5. **Slice rule** (REQ-XC-126): s ≥ P90 + w, equal across the comparison's arms (first pass s ≈ $0.85); feasibility subtracts C × s.
6. **Per-arm caps and priority.** The REQ-XH-117 cap formula with expected-cost admission (values filled at FP-E); arm priority per Q-X61 as the operator rules it (recorded default: protected original + HH-native mini-swe-agent; truncation V5 → Claude Code → Pi).
7. **Wall-time rule** (REQ-XH-126) as pre-registered.
8. **Fallback trigger** quoted from the Q-X62 ledger row verbatim; the operator fixes the number of days (14 is a synthesis placeholder); R6.33's FP-S readiness record re-run and cited, giving the readiness instant and the trigger instant.
9. **W2 quote** (R6.33's `comparative-quote/1`): D, the one harvest-only restart (REQ-XB-224), h_r, egress, retry allowance and load; Σ of every window's ceiling ≤ $20 with W1's billed or ceiling figure and any W1 export overrun taken from Wc's D (REQ-XH-118).
10. **Pre-registration hash** (R6.33): tasks, arms, effort `high`, ε = ±.20, the V5 level (Q-X56 ruled here), the claims and their expected precision.
11. **Credit period** (REQ-XH-007): start and end UTC (currently 2026-10-22T16:30:34Z → 2026-11-22T16:30:34Z) and whether the authorization covers one period or several; a reset is never new authorization.
12. **Upload authorization** (Q-X35, REQ-XH-102): one authorization covering every window's ARCHIVE-UPLOAD, W1's included as W2's first step (the SR-18 deviation as GATE-G20 accepted it); the readout cites W1's *verified harvest manifest* in place of an `archive_verified` index.
13. **Rung-record precondition.** R6.39's E2-T8 rung record (any rung, recorded honestly) is cited — **unless** a Q-X62 trigger record exists, in which case the readout names the waiver and the HH-native arms follow GATE-G29's disposition.
14. **Masking** (REQ-XH-121): cost fields only; no first-pass outcome field (W1 re-proof verdicts excepted, X-07). Q-X47 and Q-X54 cited verbatim.

**Dispositions.** `PASSED` with items 1–14 recorded verbatim; or the operator's refusal (the chain
stops). Pre-registered contingency: if the Q-X62 trigger fires while an E2 row ahead of this gate is
unlanded, or GATE-G24 is refused, the operator's recorded decision authorizes a scoped insert that
permutes {HUMAN-H14, GATE-G26, R6.40} ahead of the first unlanded E2 row (plan §Disposition gates).

## Readout fields owned (acceptance)

<a id="ac-gate-g26-1"></a>
<a id="ac-gate-g26-req-xh-007"></a>
- [ ] **AC-1 (D; `AC-4.4-9`, REQ-XH-007):** the readout records the provider credit period (start and end UTC) and whether the authorization covers one period or several; GATE-G30 repeats it (contributor). *(deterministic: readout field check)*

<a id="ac-gate-g26-2"></a>
<a id="ac-gate-g26-req-xh-013"></a>
- [ ] **AC-2 (D; `AC-4.4-10`, REQ-XH-013):** the readout records the operator's exclusivity acknowledgement, citing HUMAN-H14 item 1; GATE-G30 repeats it for W4; the window receipts' schedule check shows no overlapping paid collection. *(deterministic)*

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to FP-S, W1's harvest or the
pre-registration remains (DEFERRALS rule 4); never silently passed.

## Disposition

Record the operator's exact words, UTC and channel, every authorization and Q-X ruling verbatim,
expiry or void conditions, and explicit confirmation of any agent-drafted block's sha256. Append the
readout Signature and the matching LEDGER GATE DECISIONS row. After `PASSED`, a scoped `decompose-spec`
insert fleshes R6.40 in place (X-03, Q-X58).

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) XO-01…XO-06 and the
manifest operating rules; X-02 (the authorization is the only spend authority, enforced by the root
cap), X-03, X-05, X-07. No code branch is created by a gate.

## Notes

- Owns REQ-XH-007 and REQ-XH-013 (register rule O3); contributes the readout legs of REQ-XH-004 (R6.1), REQ-XH-120 (R6.11) and REQ-XE-120 (R6.39's rung record).

> Consistency fix 2026-11-07: W1 is $0 model with no `deliver-secret` (§10.1.4 W1 row), so it has no model calls, no REQ-XC-127 settlement clock and no model settlement; its settlement is compute-only — R6.32's `hh-compute-cost/1` record plus HUMAN-H18's W1 consolidated billing read when due. The readout records model settlement `n/a{no_model_calls}` for W1 and cites the compute record. (item 15)

- If the operator rules a level, priority or trigger number other than the recorded default, R6.33's packet is re-issued and re-hashed by a scoped insert before W2.
