<!--
  Round-7 contract — experiments §8.x second pass (decompose-spec mode=extend, 2026-11-20).
  Scope ruling: docs/build/readouts/GATE-G34.md + docs/adr/ADR-0454-round7-composite-ruling.md.
  A HUMAN marker is not an implement-spec input: it is the operator's scheduled act, recorded
  when done. Executed contracts are frozen: amend by appending a dated note.
-->
# HUMAN-H21 — Round-7 billing/settlement reconciliation

- **Sequence:** 334 of 338 · **Phase:** Round 7 · **Kind:** human
- **Block:** records · **Alias:** R7 billing reconcile
- **base_branch:** current checkout
- **Depends on:** R7.5
- **Gate status:** none — operator act

> **HUMAN marker — not an `implement-spec` input.** Owner: the operator. A code ticket that
> needs a settled figure before the entry lands records `pending{clock}` honestly — never a
> fabricated number.

## What the operator does

One dated billing/settlement entry per executed Round-7 window (X0+X1 exploration; held-out
confirmation), each a billing read ≥ 24 h after that window's teardown, appended as soon as
it is due — the H18 consolidated pattern. Entries record billed figures verbatim (model and
compute separately), reconcile against the gate's recorded quote/hard cap, and confirm
teardown (`DELETE_REQUESTED`/equivalent states and zero-running checks already enforced by
the window bodies).

## Exit

Every executed Round-7 window has its dated entry (or a recorded `N/A` when the window did
not run); unreconciled spend claims stay withheld until then.

## Withheld claims

Any billed-figure claim (total spend, per-window spend, $/trial reconciliation) a ticket
emits before its entry lands — recorded `pending{clock}` instead.

## Notes

- Scheduled like a ticket; CAP7.2's closure accounting cites these entries or records
  `pending{clock}` verbatim.
- Account setup, billing alert, credentials: already landed under H12–H14; nothing new to
  provision — `provided: yes/no` posture unchanged.
