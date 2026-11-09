<!--
  Round-6 contract — run-time insert (DF-R6.32-1 discharge path, stage 2).
  Insert authorized by the operator 2026-11-09 (verbatim "approved, continue" — W1 repair
  path (a): re-cut the declaration, fresh gate, re-run W1 as the same window).
-->
# GATE-G33 — PROV-W1 re-presented: re-authorize the W1 window on the corrected declaration ($0 model)

- **Sequence:** 290b of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** W1 · **Alias:** GATE PROV-W1-rerun
- **base_branch:** current checkout
- **Depends on:** R6.32a
- **Readout:** `docs/build/readouts/GATE-G33.md` (create from `_TEMPLATE.md` at presentation; no pre-signed readout)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

Spec §10.3's PROV-W1 row applies again, verbatim, on the corrected declaration — REQ-XH-004 requires a gate row before every paid execution ticket; the first attempt's verbatim authorized a declaration whose `packages` cannot be provisioned, so a materially corrected declaration is re-presented rather than silently substituted (no silent provider/backend substitution):

> | PROV-W1 | first | SELECT commit + P2-infra + HUMAN billing alert | compute quote (planning, bound, ceiling with restart, h_r and egress terms); dead-man action (Q-X38) and the pre-authorized restart (Q-X60); network posture (Q-X55); re-proof scope (REQ-XH-116); layer-export size estimate against W1's egress line (a quote above it refuses, REQ-XH-118) and the controller's free disk; Q-X39b |

## Pre-registered content the readout must carry

1. **The delta, honestly.** The corrected `fp-w1.json` — resolvable `packages` pins and a working `startup-script` mechanism — vs the declaration GATE-G25 admitted, member-by-member; `quote_id`, `ceiling_micros`, `dead_man`, `h_r_hours`, `network`, `machine_type`, `image`, `work_items` unchanged unless a recorded change is presented.
2. **Spend accounting.** First attempt: PROV + TEARDOWN receipts + regenerated `hh-compute-cost/1` — **784,750 µ$ ≈ $0.78** of the $5.05 ceiling consumed (`billed.status: pending`). A re-run carries the same ceiling class (~5,050,420 µ$ worst case); worst-case W1 total ≈ 5,835,170 µ$ — inside the $20 compute authorization (~$19.22 headroom). The Σ of the five declared window ceilings is unchanged (19,793,630 µ$ ≤ 20,000,000) — the re-run is the same W1 window re-attempted, not a sixth declared window.
3. **Authorizations restated.** Model authorization none ($0 model — no `deliver-secret`); compute authorization verbatim; quote id `pq-3e4ad6c78a564b77`; the hard-cap formula with every term shown; the new declaration's `verbatim_sha256` zero until this gate stamps it.
4. **Standing terms reconfirmed.** Dead-man STOP 12 h; h_r 6 h; one ≤ 1 h harvest-and-teardown-only restart inside the ceiling; teardown across every operator-gate wait; network posture Q-X55 verbatim; re-proof scope REQ-XH-116 (the SELECT commit still predates); Q-X39b's recorded answer (the `reproof` live-verify leg registered) applies to the re-run.
5. **Lessons carried.** `receipt_lib` UTC normalization landed; `check-compute-receipts.sh` routes `hh-compute-cost/1`; the committed `hh_w1` driver + `run_leg_reproof` wiring carry over unchanged.

**Dispositions.** `PASSED` records the re-authorization verbatim and stamps the corrected declaration's `verbatim_sha256`; or refusal (the chain stops). The gate basis text never contains `FAILED` or `NOT PASSABLE` on a pass.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to W1 or its prerequisites remains, **except** DF-R6.32-1 itself — the deferred body this gate exists to re-authorize (its discharge is 290c's execution, not a precondition).
