# ADR-0452: W4 scope amendment — GATE-G30 `end-first-pass-early` superseded for W4 only by operator ruling (R6.52 resurrection)

- **Status:** Accepted — recorded verbatim from the operator, 2026-11-19 (channel: operator CLI session)
- **Date:** 2026-11-19
- **Ticket:** R6.52 (W4 resurrection); amendment authored under the operator's explicit revert ruling
- **Requirement ids:** REQ-XH-126 (W4 wall-time rule), REQ-XH-127 (PP0 holds); GATE-G30 / GATE-G32 decision records
- **Spec:** `hh-fp-window/1` / `hh-fp-w4-outcomes/1`; W4 window protocol as recorded in `docs/build/runs/R6.52.md`
- **Rung (ADR-0024 obligation):** an explicit non-contract note — this ADR governs the experiments-program scope amendment boundary, not the canonical kernel contract ladder.

## Context
GATE-G30 ruled `re-scope{end-first-pass-early}` (2026-11-19 ~03:52Z): the A2 compute accounting
left ~$0.33 headroom and D_max ≈ 1.04 h < W4's declared 1.75 h overhead, so every arm was withheld
on cost fields. R6.47 subsequently fleshed `unused{scope_end}`, requested deletion of
`hh-fp-a1b2c3` (DELETE_REQUESTED 2026-11-19T02:01:58Z — GCP soft-delete, ~30-day restore window),
and recorded W4 upload/settlement `n/a{scope_end}`. GATE-G32 accepted the round with those
withholds verbatim.

## Decision (operator verbatim)

> "I want to revert that ruling. We should not skip that. Let's undelete the GCP project, increase
> the budget if that is a constraint, get new PP0 admission evidence or do whatever we need to
> avoid the evidence failure. So yeah, amend the compute pool to run W4's admissible arms
> (protected pair + V5), record Pi/CC not_run{admission}"

Applied: GATE-G30's `re-scope{end-first-pass-early}` disposition is **superseded** for the W4
scope (it stands in history; this ADR is the dated amendment). The compute-pool constraint is
amended to admit W4's executable arms:

- **Protected pair** (mini-swe-agent + HH-native) — was `withheld{window_deadline}`; now runs.
- **V5** — was `withheld{budget}`; now runs.
- **Pi + Claude-Code** — `withheld{admission:AG3}` (PP0 evidence failure, not budget); the
  operator's ruling records them `not_run{admission}` verbatim — no fabrication of admission.
- R6.47's teardown act stands (DELETE_REQUESTED is real history); the W4 resurrection restores
  the project via `gcloud projects undelete` and runs the window under the standard protocol
  (declaration → runs → TEARDOWN → receipts → reconcile).
- New work rides new manifest rows (inserts after 325); the earlier `n/a{scope_end}` records
  stay append-only — a superseding record set carries the executed outcome.

## Consequences
- W4 results packets may supersede the withheld P2-FP/P3-L2/P4-FP claims if k≥5/R≥5 complete;
  `inconclusive`/`not_run{admission}` are landed honest outcomes.
- Spend bound: operator's standing ≤$50/day + "increase the budget if that is a constraint".
- All window invariants unchanged (quotes never authorize; withholds verbatim; no fabricated
  frozen/pass/settlement).

## Revisit trigger

Any proposal to re-run an arm the operator recorded `not_run{admission}` without new PP0 admission evidence, or a request to extend this amendment's supersession beyond the W4 scope — then the narrow-supersession boundary (GATE-G30's `end-first-pass-early` stands for every other scope) must be re-ruled explicitly rather than inferred.
