<!--
  Round-7 contract — experiments §8.x second pass (decompose-spec mode=extend, 2026-11-20).
  Scope ruling: docs/build/readouts/GATE-G34.md + docs/adr/ADR-0454-round7-composite-ruling.md.
  GATE-G37 is a normal gate, presented to the operator — NOT pre-answered. This is where an
  incomplete pass re-presents the O-3 ruling (ADR-0454's consequence clause).
  This contract CITES the spec; it does not copy a design. Executed contracts are frozen.
-->
# GATE-G37 — Round-7 acceptance: the pass's terminal state, the O-3 record, BL-50 and reconciliation verified

- **Sequence:** 337 of 338 · **Phase:** Round 7 · **Kind:** gate
- **Block:** closure · **Alias:** GATE-ACCEPT (Round 7)
- **base_branch:** current checkout
- **Depends on:** CAP7.2
- **Readout:** `docs/build/readouts/GATE-G37.md` (owed — presented to the operator; not pre-answered)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion

Round-7 acceptance over the minimal tail: the pass's honest terminal state is what CAP7.1
recorded, the O-3 disposition record minted (or `not_effective{…}` recorded + the ruling
re-presents — the operator's verbatim call), BL-50's 40 rows dispositioned as recorded, the
lead-token reconciliation stands against the machine-readable sets, and the Round-7 ledger
and index agree.

## What the readout must contain (pre-registered)

1. **CAP7.1's terminal-state record** — the per-id verdict table + determination verbatim.
2. **The O-3 record** — `docs/build/reports/round7/O3-scope-out.md` minted with the verbatim
   remainder set, **or** the `not_effective{terminal_state_incomplete}` record with the
   named gaps — exactly one, matching the determination. When the mint could not honestly
   happen, **the O-3 ruling re-presents here** for the operator's fresh words.
3. **BL-50's closure note** — per-row evidence closures + verbatim O-3 dispositions; the
   dated backlog note.
4. **The reconciliation record** — the stale lead-token corrections (DF-CAP6.1-1…5,
   DF-R6.45l-1, DF-R6.50-1, DF-R6.53-1) + the re-run owed set.
5. **The accounting** — HUMAN-H21's entries or `pending{clock}` verbatim; the guard's
   verdict; ledger/index agreement.
6. **The gate decision rows** — GATE-G35/G36 verbatim dispositions cited (spend evidence,
   REQ-XD-221's record).

## Dispositions (pre-registered)

- `PASSED` — the round's record is accepted; the O-3 record stands as minted.
- `PASSED — re-present{…}` — the operator's fresh ruling where a conditional act re-presented
  (incomplete pass; a deferred disposition; a carried set needing words).
- `NOT PASSABLE` / `FAILED` — recorded with basis; named residuals carry forward as a
  named set, never silently.

## Operating clauses

Follow the [manifest operating rules](00_MANIFEST.md#operating-rules). No gate is pre-signed;
the O-3 record mints at closure on the ruling's conditional authority — this readout
verifies it, and where it could not mint the ruling itself returns to the operator.

## DEFERRALS rule

A failed prerequisite is `blockedOn` with a dated note, never a fabricated pass.
