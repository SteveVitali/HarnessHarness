<!--
  Round-6 contract — experiments first pass (decompose-spec mode=extend, 2026-11-07).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Hand-off: docs/experiments-decomposition-prompt.md. Partition + coverage:
  docs/build/planning/2026-11-07_round6-{decomposition,coverage}.md.
  Insertion authorized by the operator (planning conversation, 2026-11-07): "I want you to
  continue on until you're totally done, the new tickets are inserted, there's a new PR with
  the changes". This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
  Tail template: ~/agent-skills/skills/build-memory/templates/tail/GATE-ACCEPT.md.
-->
# GATE-G32 — Round-6 acceptance (GATE-ACCEPT role)

- **Sequence:** 324 of 325 · **Phase:** Round 6 · **Kind:** gate
- **base_branch:** current checkout
- **Depends on:** CAP6.3
- **Blocks:** DOC6.1; Round-6 completion (BM-TAIL-03)
- **Readout:** `docs/build/readouts/GATE-G32.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator dispositions the list for signature. Pending operator review is RETURN PASS; a failed technical prerequisite is blockedOn. Agents never sign, tick or infer approval; old acceptance (GATE-G17) never substitutes.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10) gate table, row "GATE-ACCEPT (tail)":

> | GATE-ACCEPT (tail) | first | CAP.3 | Round-6 sums from billed figures where they exist, including the $100 check (REQ-XH-110); accepted deviations |

REQ-XH-110 acceptance ([§10.1](../experiments-spec.md#s10-1), `AC-10.1-1`):

> CAP.3 closure recompute from the phase spend ledger: Σ authorized ≤ $90; Σ (model billed, else settled, else metered with orphans at `bounded.hi`; compute billed, else estimated; other) ≤ $100, using the billed figure wherever a bill exists; readouts cite Q-X47 and Q-X54 verbatim.

## Pre-registered thresholds

The operator reviews, in one packet (each artifact's sha256 stated):

1. **CAP6.1** — the REQ-X `COVERAGE_MATRIX.csv` verdicts, the seam hunt, the composed offline verification and the deviation section.
2. **CAP6.2** — `CAPSTONE_CLOSURE.md`'s two sums (engineering closed; requirement satisfied) and highest layer; the $100 recompute and per-kind/per-phase spend from billed figures where HUMAN-H18 recorded them; the live reads (open compute, project deletion state, Ollama balance and usage, dataset visibility, operator-disk retention, stack CI) with their UTCs; the operator-disk/dataset retention decision (REQ-XH-106) if not yet recorded; the later-pass routing register (34 ids) and its live re-read; the §0.3 history check.
3. **CAP6.3** — the spec reconciliation (ticked amendments only), the ADR-set comparison and the integration plan (read-only; recommends, never merges).
4. **The list for signature**, three parts: (a) accepted deviations — at least W1's archive upload as W2's first step (SR-18), the consolidated reconcile placement (REQ-XH-008 interpretation), the re-partition and tail-split departures from §10.6 (minimal / 4-run tail instead of `tail=full`; RECIPE a/b merged; row count below the first-cut band), and any CAP6.1 found; (b) scoped-out owed legs with owner and trigger (including every `withheld{…}` claim and any `DEFERRED` HUMAN row); (c) waivers requested, each naming an ADR quoting the operator.

Every proposed row is signed (accepted) or sent back to closure; none is left unsigned. A scoped acceptance authorizes action and never raises a verdict (`accepted_scope` set ⇒ verdict ≤ MET-ENGINEERED). A routed id's record is accepted as routing, never as satisfaction. No threshold moves to fit a result; an `inconclusive` or `withheld` first-pass outcome is acceptable as recorded.

## Readout

`docs/build/readouts/GATE-G32.md`, append-only: the packet presented (agent-drafted, with its sha256), the two sums and the highest layer, the operator's per-row disposition (ACCEPTED / SEND-BACK + what would accept it) in their own words, UTC and channel, `Status:` PASSED | NOT-PASSABLE | SKIPPED-BY-OPERATOR, and the Signature block appended at signing.

- [ ] *(live-read)* The readout cites the live state it describes: the CI read of every open PR in the stack (`ci-boundary.sh --stack --no-wait` with `read_at`, BM-CI-01) and CAP6.2's live reads no older than 24 h at presentation, or a fresh re-read; a production-state statement with no cited read is removed.

## Disposition

Recorded only from the operator's own words: the readout's Signature block and the matching LEDGER GATE DECISIONS row (`kind: decision`), verbatim, committed on the chain tip by `orchestrate-build`. No merge, publication or spend permission beyond explicit words.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to the Round-6 tail remains without an explicit, protocol-compliant disposition (owner, trigger, withheld claims) on the signed list.

## Operating clauses

Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) (X-02, X-03, X-08; XO-04, XO-06) and the manifest operating rules. Consumes R6.1's gate-disposition templates. No code branch is created by a gate.
