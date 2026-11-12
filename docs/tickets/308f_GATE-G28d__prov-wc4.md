<!--
  Round-6 contract — run-time insert (GATE-G28d scope-decision, 2026-11-12).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Insert authorized by the operator 2026-11-12T15:45Z (LEDGER GATE DECISIONS
  scope-decision row GATE-G28d; operator authorization "a Wc4 window — the
  verifier fix + a longer D" relayed by the coordinator — Wc3's re-proof
  verified BOTH carried repairs live yet ran to the 5 h dead-man at 28/30
  surfaces with two ~1 h+ slow-member stretches and the dev leg never
  launched — DF-R6.45b-1 stands — and every H-allow surface recorded
  `exited{0}` → `verifier_unavailable:reward_missing`, a verifier-side
  machinery defect repaired in parallel — a fourth repair window beyond the
  spec's at-most-once Wc, discharged at R6.45, and beyond the recorded Wc2
  and Wc3 amendments, discharged at R6.45b and R6.45c). The Σ-exceed is
  recorded: Σ declared ceilings 23,668,230 + Wc4 2,565,580 = 26,233,810 µ$
  ≈ $26.23 under the operator's authorization.
  This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
-->
# GATE-G28d — PROV-Wc4: authorize the fourth repair window's compute (scope amendment — D = 6 h re-proof + the verifier `reward_missing` fix)

- **Sequence:** 308f of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** Wc4 · **Alias:** GATE PROV-Wc4
- **base_branch:** current checkout
- **Depends on:** R6.45c
- **Readout:** `docs/build/readouts/GATE-G28d.md` (presented 2026-11-12; Status PASSED — run)

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

Wc4 is a **recorded spec deviation — the fourth repair window**: the spec's
gate table declares PROV-Wc "at most once" and the Wc row "at most one
contingency window" — discharged at R6.45; the recorded Wc2 and Wc3
amendments were discharged at R6.45b (the mediated-egress repair verified
live, but the re-proof ran to the 3 h dead-man at 28/30 surfaces) and
R6.45c (both carried repairs verified live — zero `cell-terminate`
machinery exceptions, no scope-mint/egress signature — yet the held leg
again ran to the dead-man, now at 5 h and 28/30 surfaces, and every
H-allow cell exited `exited{0}` then `verifier_unavailable:reward_missing`).
Wc3's two signals need their own gate+window: **DF-R6.45b-1 stands** — even
D = 5 h does not bound the held leg's real-trial pace at R = 3 (two ~1 h+
slow-member stretches — pypi-server H-allow ~1 h, query-optimize
mid-flight at cut-off; the dev leg never launched; D → 6 h gives headroom
and the recorded split-leg option stays a later-gate fallback if even 6 h
is tight); **the H-allow `verifier_unavailable:reward_missing` outcome is a
verifier-side machinery defect** (agents `exited{0}` yet `reward.txt`
never lands — `/logs/verifier` was never created in the cell so the reward
write fails, plus the deny-fold read `host` vs the payload's
`host_norm`/`host_raw` members — the fifth defect), repaired in parallel
under the scope-decision. The operator's scope-decision authorized the
D = 6 h window + the verifier fix ("a Wc4 window — the verifier fix + a
longer D", 2026-11-12T15:45Z), and the **Σ-exceed is recorded** — Σ
declared ceilings 23,668,230 + Wc4 2,565,580 = **26,233,810 µ$ ≈ $26.23**
> $20 under the operator's authorization. This gate is never treated as
the declared §10.3 PROV-Wc row re-run; it is the scope amendment's own
record.

## Pre-registered thresholds

1. **The repairs are merged** — the mediated-egress fix merged `5b8e8f28`
   (verified live in Wc2: the mediator's `requested`/`decided` rows land)
   **and the `pkill` repair** merged `bb60a46f` (verified live in Wc3: zero
   `cell-terminate` machinery exceptions on the 9 fully-surfaced members)
   **and the verifier `reward_missing` repair** merged `fe87c637`
   (lifecycle-owned `/logs/verifier` — `run_cell`'s ST5 leg `mkdir -p`s the
   reward sink before the verify exec — plus the verify-phase deny fold
   `host` → `host_norm`/`host_raw` in `verdict.rs`; machinery only — the
   drawn tasks, surfaces O/H-pub/H-allow, the R/R admit rule and the
   committed replacement orders untouched); the REQ-XC-330 new runner
   version recorded (`wc4-repair-record.json` — `hh-w1` `sha256:b1ba7ffdc…`,
   mediator `sha256:ec8d4408…`, hh-helper `sha256:ed4e132a…` byte-identical
   to Wc3's; the `carried_rebuild_guard` satisfied under this gate). The
   window's runner carries ALL THREE repairs.
2. **Compute terms** — quote `pq-78f1e79eeb33f575` (the one quote path's
   deterministic id), ceiling **2,565,580 µ$ ~$2.57** = r·(D + restart) +
   residual disk at h_r + egress = 314,140·(6+1) + 300·137·6 + 0.12·1
   = 2,198,980 + 246,600 + 120,000; D = 6 h dead-man; in-guest timer
   375 min; one ≤ 1 h harvest-only restart; h_r = 6 h; C = 4; ~38 oracle
   trials (the 14 `needs: wc_reproof` members × committed R — 10 held-out
   @R=3 + 4 dev @R=2); `retry_trials: 0`; W_p = 7/3 h ≈ 2.333 ≤ D − cut-off
   5.35 h; egress 1 GiB; e2-standard-8, debian-12, 300 GiB.
3. **$0 model** — `model_line: none`; `deliver-secret` ABSENT (REQ-XB-219 — a
   call against a $0-window declaration refuses); no model key in-window.
4. **Σ-exceed recorded** — never silently re-scoped; the authorization
   names the exceed.
5. **Previous window closed** — Wc3's harvest verified and its upload
   `archive_verified` (terminal dataset revision
   `c34bb22f249d8c676cecaa97b03bf2c646ee6c38`) — the index Wc4's own upload
   cites (REQ-XH-102; DF-R6.23-1's R6.45d share).

**Pre-registered dispositions** (each recorded `PASSED — …; basis: <verbatim>`):

- `run` — items 1–5 met and authorized; R6.45d dispatches for the Wc4 body
  (PROV → LOAD → the recorded in-window steps → the re-proof on the
  thrice-repaired runner at D = 6 h → the frozen-list outcome → teardown →
  UPLOAD-Wc4 → SETTLE-Wc4).
- `not_run` — the re-proof does not run; `task_lists.state` stays `blocked`
  verbatim and every subject leg keeps its `withheld{un_reproofed_successor}` /
  `withheld{no_adjacent_donor}` — the honest state, never rescued.

Only an operator refusal stops the chain.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to the
triggering repair or Wc's closure remains (DEFERRALS rule 4); never silently
passed.
