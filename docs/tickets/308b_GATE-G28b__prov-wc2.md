<!--
  Round-6 contract — run-time insert (GATE-G28b scope-decision, 2026-11-11).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Insert authorized by the operator 2026-11-11T19:23:10Z (LEDGER GATE DECISIONS
  scope-decision row GATE-G28b; operator verbatim "New repair window" — the
  O-surface machinery's third defect, the mediated-egress leg, needs a further
  repair+re-proof beyond the spec's at-most-once Wc, discharged at R6.45). The
  Σ-exceed is authorized verbatim: "that's okay if it exceeded by a little" —
  Σ declared ceilings 19,793,630 + Wc2 1,623,160 = 21,416,790 µ$ ≈ $21.42.
  This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
-->
# GATE-G28b — PROV-Wc2: authorize the second repair window's compute (scope amendment — mediated-egress re-proof)

- **Sequence:** 308b of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** Wc2 · **Alias:** GATE PROV-Wc2
- **base_branch:** current checkout
- **Depends on:** R6.45
- **Readout:** `docs/build/readouts/GATE-G28b.md` (presented 2026-11-11; Status PASSED — run)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

Spec [§10.3](../experiments-spec.md#s10), gate table:

> | PROV-Wc (at most once) | first | the triggering event: a W1 machinery repair (W1′), a gate-A L1 failure (W3′) or a restart overflow | compute quote (planning, bound, ceiling with restart, h_r and egress terms) within Wc's remaining reservation (at most $1.94, less any W1 export overrun taken from it); which of W1′, W3′ or restart overflow; D, restart and h_r; the repair's scope and its new participant or runner version record (REQ-XC-330); $0 model; Σ ≤ $20 recomputed (REQ-XH-118) |

§10.1.4, Wc row (verbatim):

> | Wc | at most one contingency window: a repair re-proof (W1′), an L1 rerun on gate-B (W3′), or a harvest-restart overflow; D = 4 h gives about 1.75 h of trial time after overhead, load and the admission cut-off (≈ 42 oracle trials at C = 4), and a larger repair is not runnable in the first pass, a scope decision PROV-Wc records | $0 | its own PROV-Wc gate | if not authorized by FP-E, its reservation is released to W4 except one restart-overflow reservation (REQ-XH-126) |

Cited requirement, verbatim:

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

## Scope amendment (recorded, verbatim)

Wc2 is a **recorded spec deviation**: the spec's gate table declares PROV-Wc
"at most once" and the Wc row "at most one contingency window" — discharged at
R6.45 (0/14 covering verdicts; the mediated-egress leg's third defect
diagnosed). The O-surface machinery's third defect (the mediator's `serve_conn`
scope-mint violation + dropped resolve-pins + the proxy-port projection/socket
wait — H-allow 0/R on all 14, `verifier_unavailable:reward_missing`,
`denied_hosts: []`) is a new machinery repair needing its own gate+window. The
operator's scope-decision authorized it verbatim ("New repair window",
2026-11-11T19:23:10Z), and the **Σ-exceed is authorized verbatim** ("that's
okay if it exceeded by a little" — Σ declared ceilings 19,793,630 + Wc2
1,623,160 = 21,416,790 µ$ ≈ $21.42 > $20). This gate is never treated as the
declared §10.3 PROV-Wc row re-run; it is the scope amendment's own record.

## Pre-registered thresholds

1. **The repair is merged** — `5b8e8f28` (machinery only: `hh_egress_mediator.rs`
   scope-mint + resolve-pins; `hh_w1.rs` egress-port projection + socket wait);
   the drawn tasks, surfaces O/H-pub/H-allow, the R/R admit rule and the
   committed replacement orders untouched; the REQ-XC-330 new runner version
   recorded (`wc2-repair-record.json` — `hh-w1` `sha256:f9b6ad05…`, built from
   source at the merge; the `carried_rebuild_guard` satisfied under this gate).
2. **Compute terms** — quote `pq-95bf3c1ff646a476` (the one quote path's
   deterministic id), ceiling **1,623,160 µ$ ~$1.62** = r·(D + restart) +
   residual disk at h_r + egress = 314,140·(3+1) + 300·137·6 + 0.12·1
   = 1,256,560 + 246,600 + 120,000; D = 3 h dead-man (re-proof only — the
   repair is merged); in-guest timer 195 min; one ≤ 1 h harvest-only restart;
   h_r = 6 h; C = 4; ~38 oracle trials (the 14 `needs: wc_reproof` members ×
   committed R — 10 held-out @R=3 + 4 dev @R=2); `retry_trials: 0`;
   W_p = 7/3 h ≈ 2.333 ≤ D − cut-off 2.35 h (thin margin flagged honestly);
   egress 1 GiB; e2-standard-8, debian-12, 300 GiB.
3. **$0 model** — `model_line: none`; `deliver-secret` ABSENT (REQ-XB-219 — a
   call against a $0-window declaration refuses); no model key in-window.
4. **Σ-exceed recorded** — never silently re-scoped; the operator's verbatim
   authorization names the exceed.
5. **Previous window closed** — Wc's harvest verified and its upload
   `archive_verified` (terminal dataset revision
   `309d7029a62094652b340997dadc8e69cd472506`) — the index Wc2's own upload
   cites (REQ-XH-102; DF-R6.23-1's R6.45b share).

**Pre-registered dispositions** (each recorded `PASSED — …; basis: <verbatim>`):

- `run` — items 1–5 met and authorized; R6.45b dispatches for the Wc2 body
  (PROV → LOAD → the recorded in-window steps → the re-proof on `f9b6ad05…` →
  the frozen-list outcome → teardown → UPLOAD-Wc2 → SETTLE-Wc2).
- `not_run` — the re-proof does not run; `task_lists.state` stays `blocked`
  verbatim and every subject leg keeps its `withheld{un_reproofed_successor}` /
  `withheld{no_adjacent_donor}` — the honest state, never rescued.

Only an operator refusal stops the chain.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to the
triggering repair or Wc's closure remains (DEFERRALS rule 4); never silently
passed.
