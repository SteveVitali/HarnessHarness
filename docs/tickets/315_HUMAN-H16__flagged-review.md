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
# HUMAN-H16 — Flagged-pass review before the first-pass results freeze

- **Sequence:** 315 of 325 · **Phase:** Round 6 · **Kind:** human
- **base_branch:** current checkout
- **Depends on:** R6.48
- **Blocks:** R6.49 (its results-freeze stage)
- **Owner:** an operator-named reviewer (named in the readout)
- **Scheduled:** trigger — R6.48 landed (seal opened; `answer-scan/1` flagged-pass list committed); before R6.49's freeze. Actual date recorded at the trigger (`date -u +%F`).
- **Withheld until done:** the results-freeze claim and every "final" first-pass results packet; unreviewed flags at freeze stay vetoes (spec §6.9)
- **Deferrals so far:** 0
- **Readout:** `docs/build/readouts/HUMAN-H16.md` (create from `_TEMPLATE.md` at presentation)

> **Operator work — NOT an `implement-spec` input.** The reviewer's own words and records supply the dispositions; an agent never reviews, clears, confirms or infers silence as a disposition.

## Source

Spec [§6.9](../experiments-spec.md#s6) and §10.3's HUMAN table row: "Flagged-pass review | operator-named reviewer | each results freeze (zero flags recorded trivially) | dispositions with excerpt hashes | results packets". REQ-XH-121: the review runs after the seal opens and before the results freeze.

## Checklist

- [ ] Read R6.48's flagged-pass list (every flagged pass across P2-FP, P3-L2 and P4-FP trials; detector version and coverage stated). Zero flags: record that verbatim and close trivially.
- [ ] For each flag record `{confirmed, cleared}`, the reason, and the excerpt hash; review with harness identity masked where feasible (record whether masking was feasible).
- [ ] Record counts flagged / confirmed / cleared, the detector version and coverage, and the sentence "the confirmed count is a minimum".
- [ ] Record the reviewer's name/role, UTC and channel; no excerpt content beyond its hash enters the readout if it could carry task-derived solution text.

## Exit criterion

<a id="ac-human-h16-req-xd-071"></a>
Review records are complete for every flag (REQ-XD-071, `AC-6.9-2`, layer D): each flag has a disposition, reason and excerpt hash, and the readout's `Status:` is non-PENDING. If the review cannot be completed before the freeze, the readout records `Status: DEFERRED` citing an opened `DF-*` row (token per R6.1's HUMAN readout templates); R6.49 then lands with the freeze claim withheld and every unreviewed flag kept as a veto.

## Requirement IDs

- **Primary:** REQ-XD-071 (contributor R6.27, the review-record schema).

## DEFERRALS rule

A row that runs before this completes opens a `DF-*` row in `DEFERRALS.md` — why deferred = "HUMAN-H16 pending · withholds: results freeze", unblocked by = "HUMAN-H16 · owner: operator-named reviewer · trigger: R6.48 landed", how to verify — and reports "pending", never a fabricated pass. A second deferral needs an operator scope/date/ADR decision. Follow [Round-6 invariants](../build/planning/2026-11-07_round6-invariants.md) (X-03, X-07) and the manifest operating rules.
