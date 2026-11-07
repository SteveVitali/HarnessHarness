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
# GATE-G20 — Review the Round-6 plan and authorize the GUARD-X bootstrap

- **Sequence:** 252 of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** B0 · **Alias:** R6 plan review + bootstrap (B0)
- **base_branch:** current checkout
- **Depends on:** nothing
- **Readout:** `docs/build/readouts/GATE-G20.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** Pending operator review is RETURN PASS; a failed technical prerequisite is blockedOn. Agents never sign or infer approval.

## Criterion (verbatim from the spec)

> | R6 plan review + bootstrap | first | decomposition | partition, coverage register; authorizes only GUARD-X |

> Each gate's marker quotes its pre-registered content verbatim from this spec before the gated work begins. None is answered here.

> GUARD-X is the first row after the bootstrap gate.

> **REQ-XH-002** The first ticket after Round 6's bootstrap gate SHALL extend `hh-supplement-guard/1` to this supplement: second frozen-source digest, REQ-X coverage register, Round-6 routing (AR-31). No product ticket dispatches before it lands.

> **REQ-XH-025** Every gate marker SHALL quote its thresholds verbatim from this spec before the gated work begins. A pending gate is RETURN PASS, never `blockedOn`.

> | Q-X46 | Schedule vs the credit reset 2026-11-22T16:30:34Z | pending — planning choice between linearization A and B; authorizes no spend | ratification |

Source: [s10](../experiments-spec.md#s10) (§10.3 gate table and REQ table, §10.6 hand-off, §10.2 linearizations); Appendix B (Q-X46). Frozen hand-off §4: "with the stock checker the guard reports 46 `unexplained-diagnostic` legacy `reqcov` warnings absent from its reviewed baseline; record as an OPEN FINDING for the guard-extension row, never silently re-baselined."

## Pre-registered thresholds

The decision content below is fixed before presentation. Each numbered item is decided separately; none is pre-answered.

1. **Artifact identity (reconfirmed at presentation, never pre-filled).** The readout records: the spec sha256 (`0bcfb3572dcc0bc921014f265ee34876f2f74bad7b6cd685abe214ebcf78362e` at hand-off) recomputed; the hand-off file's sha256; the sha256 of each planning file (`2026-11-07_round6-{decomposition.md,coverage.md,coverage.json,invariants.md}`) and of `docs/build/tools/check_round6_coverage.py`, with that checker re-run (recorded at planning: `RESULT: PASS (35/35)`, 20/20 negative controls failing); `main` `185c16c7` as an ancestor of the chain base; the manifest Round-6 banner (declaring `tail=minimal`) and rows 252–325 present; LEDGER `round: 6`, `nextTicket: GATE-G20`, `chainTip: main`, `buildBranchBase` and `pinnedBaseSha` unchanged; `bash scripts/deferrals-open.sh` output verbatim.
2. **Partition and coverage register.** Accept or redirect the re-partitioned plan (74 rows, seq 252–325; 49 implement-spec runs = 45 tickets + 4 tail runs; 13 gates; 7 HUMAN rows; 5 skeletons) and the register (456 candidates, 370 adopted, each with exactly one primary owner).
3. **Q-X46 linearization (no recorded default).** `A` (phase 1 and site shell first; the plan's order) or `B′` (the checked permutation in the plan's `linearization-B-prime` marker: R6.6, R6.20, R6.21, HUMAN-H12, GATE-G22, R6.31 moved to just after GATE-G29). `B′` is applied by a scoped manifest re-order, never inferred.
4. **Deviations needing the operator's verbatim acceptance** (each `accept` or `refuse`; a refused item returns to a scoped `decompose-spec mode=extend` re-plan before the first row it affects, never silently kept):
   - D-1 ARCHIVE-UPLOAD-W1 runs as the first step of W2's fleshed skeleton (R6.40), after GATE-G26 and before PROV-W2 — a departure from REQ-XH-102's "before the next paid window's gate" ([plan ambiguity 1](../build/planning/2026-11-07_round6-decomposition.md#spec-ambiguities-resolved-here-to-be-confirmed-at-gate-g20)).
   - D-2 Reconcile placement: the five window reconcile reads consolidated in HUMAN-H18 before the tail; REQ-XH-008's sequence read as an ordered per-window subsequence (plan tradeoff 10).
   - D-3 `tail = full` (§10.6) replaced by the minimal tail the banner declares with `tail=minimal`, split by the operator to four runs (CAP6.1, CAP6.2, CAP6.3, then GATE-G32, then DOC6.1; plan "Tail split").
   - D-4 RECIPE's §10.6 pre-split (a/b) merged into R6.13, B3's seam kept as the in-row stage boundary.
   - D-5 Row and implement-spec counts below §10.6's first-cut band (≈ 100–110 / 85–90), with rows sized to ≤ 95k focused + 10k reserve (plan tradeoff 11, Totals).
   - D-6 Wc block placed between W3 and G-L1 (plan tradeoff 5).
5. **Interpretations to confirm or redirect.** The plan's twelve "Spec ambiguities resolved here (to be confirmed at GATE-G20)", plus REQ-XH-002's "the second Round-6 row" read as R6.2, the first product row (plan "Bootstrap and the legacy guard warnings").
6. **Scoped bootstrap authorization (GATE-G8 precedent, [168](168_GATE-G8__bootstrap.md)).** It authorizes the dispatch of **R6.1 only**. Its ROUTE is computed by `bash docs/build/tools/check-supplement-guard.sh --no-checker` and must read `DISPATCH — R6.1`. The stock build-memory validator runs separately and must report 0 violations. The 46 legacy `reqcov` warnings are disclosed in the readout as an OPEN FINDING owned by R6.1, with their count and the sha256 of the listed warnings. This is quoted, never a silent substitution. **Expiry:** R6.1 landing; from R6.2 on, `PASS` with the stock checker is required (XO-02). **Void if:** a reviewed artifact's bytes change, the base moves without re-review, or the validator reports a violation.
7. **Not authorized by this gate:** product semantics, spend of any kind, provisioning, publication, external writes or merges. It answers no later gate.

## Disposition

Record exact operator words, UTC, channel, each item's decision (1–7, with the Q-X46 choice and every D-n `accept`/`refuse` verbatim), expiry/void conditions and explicit confirmation of the agent-draft hash. Append readout/signature and the matching GATE DECISIONS row; old acceptance never substitutes. A pending or failed gate stops dependent dispatch. A scoped decision names carried unmet predicates (the 46-warning OPEN FINDING) and compensating controls (stock validator at 0 violations; R6.1's AC-3).

## DEFERRALS rule

No OPEN scoped technical deferral is silently passed. Resolve it or obtain an explicit protocol-compliant disposition with owner/trigger and withheld claims; this decomposition is not that disposition.

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) X-01, X-03 and XO-01…XO-06, and the manifest operating rules. No code branch is created by a gate. The coordinator, not this marker, writes the LEDGER extension values.
