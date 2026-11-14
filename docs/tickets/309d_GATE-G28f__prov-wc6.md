<!--
  Round-6 contract — run-time insert (GATE-G28f scope-decision, 2026-11-13).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Insert authorized by the operator 2026-11-13T20:45Z (LEDGER GATE DECISIONS
  scope-decision row GATE-G28f; operator authorization "a Wc6 re-proof window
  — D = 8 h on the proven machinery + the widened allowlist" relayed by the
  coordinator — Wc5's re-proof ran REAL trials to 29/30 held surfaces under
  the epoch-2 widened allowlist: the widened allowlist **verified live
  per-cell** — zero denied hosts, real `reward.txt` digests on every
  completed H-allow cell — yet the 6 h dead-man fired with query-optimize's
  H-allow `inconclusive{backend_terminated}`, the dev leg unlaunched and no
  `hh-reproof-packet/1` landed → `blocked` verbatim; the residual deficit is
  a window-bound property — the real apt bootstrap pace ~12 min/surface
  needs ~6.5–7 h for both legs — never machinery, never declared data).
  A sixth repair window beyond the spec's at-most-once Wc, discharged at
  R6.45, and beyond the recorded Wc2, Wc3, Wc4 and Wc5 amendments,
  discharged at R6.45b, R6.45c, R6.45d and R6.45e. The Σ-exceed is
  recorded: Σ declared ceilings 28,799,390 + Wc6 3,193,860 = 31,993,250 µ$
  ≈ $32.00 under the operator's authorization.
  This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
-->
# GATE-G28f — PROV-Wc6: authorize the sixth repair window's compute (scope amendment — the D = 8 h re-proof on the thrice-repaired runner under the epoch-2 widened allowlist)

- **Sequence:** 309d of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** Wc6 · **Alias:** GATE PROV-Wc6
- **base_branch:** current checkout
- **Depends on:** R6.45e
- **Readout:** `docs/build/readouts/GATE-G28f.md` (presented 2026-11-13; Status PASSED — run)

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

Wc6 is a **recorded spec deviation — the sixth repair window**: the spec's
gate table declares PROV-Wc "at most once" and the Wc row "at most one
contingency window" — discharged at R6.45; the recorded Wc2, Wc3, Wc4 and
Wc5 amendments were discharged at R6.45b (the mediated-egress repair
verified live, but the re-proof ran to the 3 h dead-man at 28/30 surfaces),
R6.45c (both carried repairs verified live — zero `cell-terminate`
machinery exceptions, no scope-mint/egress signature — yet the held leg
again ran to the dead-man, now at 5 h and 28/30 surfaces, and every H-allow
cell exited `exited{0}` then `verifier_unavailable:reward_missing`), R6.45d
(the re-proof ran to COMPLETION at D = 6 h in ~4.6 h — DF-R6.45b-1
discharged — with ALL THREE repairs verified live, yet 0/14 covering
verdicts: 13 `exclude:runner_infeasible:allowlist_gap{…}` + query-optimize
`repair:machinery`) and R6.45e (the re-proof under the epoch-2 widened
allowlist ran REAL trials to 29/30 held surfaces — the widened allowlist
**verified live per-cell**: zero denied hosts, real `reward.txt` digests on
every completed H-allow cell — yet the 6 h dead-man fired with
query-optimize's H-allow `inconclusive{backend_terminated}`, the dev leg
unlaunched and no `hh-reproof-packet/1` landed → `blocked` verbatim).
Wc5's residual deficit is a **window-bound property, never machinery and
never declared data**: the real apt bootstrap executing per cell makes the
pace ~12 min/surface — the held 10@R=3 leg's 30 surfaces need ~6 h and the
dev 4@R=2 leg ~0.8 h — ~6.5–7 h of real work, more than D = 6 h allows.
The remedy is **D = 8 h** (in-guest 495 min) on the proven machinery under
the proven allowlist — the longer bound covers the Wc5-measured real-work
pace with margin. The operator's scope-decision authorized the longer-D
re-proof ("a Wc6 re-proof window — D = 8 h on the proven machinery + the
widened allowlist", 2026-11-13), and the **Σ-exceed is recorded** — Σ
declared ceilings 28,799,390 + Wc6 3,193,860 = **31,993,250 µ$ ≈ $32.00**
> $20 under the operator's authorization. This gate is never treated as the
declared §10.3 PROV-Wc row re-run; it is the scope amendment's own record.

## Pre-registered thresholds

1. **The repairs are merged and the amendment landed** — the
   thrice-repaired runner lineage stands: the mediated-egress fix merged
   `5b8e8f28` (verified live in Wc2), the `pkill` repair merged `bb60a46f`
   (verified live in Wc3) and the verifier `reward_missing` repair merged
   `fe87c637` (verified live in Wc4 — every H-allow cell produced its
   `reward.txt`; `denied_hosts` carried the real
   `archive.ubuntu.com`/`security.ubuntu.com`) — `hh-w1`
   `sha256:b1ba7ffdc…`; **no new machinery repair** — and the epoch-2
   widened allowlist's `hh-egress-allowlist-freeze/1` record
   (`sha256:86def07e…`, `docs/build/reports/
   2026-11-13-fp-egress-allowlist-freeze-epoch-2.json`) is committed on
   main (`28617cb0`) and **verified live per-cell in Wc5** — zero denied
   hosts, real `reward.txt` on every completed H-allow cell; the freeze is
   carried verbatim (the drawn tasks, surfaces O/H-pub/H-allow, the R/R
   admit rule and the committed replacement orders untouched).
2. **Compute terms** — quote `pq-2866041059397e91` (the one quote path's
   deterministic id), ceiling **3,193,860 µ$ ~$3.19** = r·(D + restart) +
   residual disk at h_r + egress = 314,140·(8+1) + 300·137·6 + 0.12·1
   = 2,827,260 + 246,600 + 120,000; D = 8 h dead-man; in-guest timer
   495 min; one ≤ 1 h harvest-only restart; h_r = 6 h; C = 4; ~38 oracle
   trials (the 14 `needs: wc_reproof` members × committed R — 10 held-out
   @R=3 + 4 dev @R=2); `retry_trials: 0`; W_p = 7/3 h ≈ 2.333 ≤ D − cut-off
   7.35 h; egress 1 GiB; e2-standard-8, debian-12, 300 GiB.
3. **$0 model** — `model_line: none`; `deliver-secret` ABSENT (REQ-XB-219 — a
   call against a $0-window declaration refuses); no model key in-window.
4. **Σ-exceed recorded** — never silently re-scoped; the authorization
   names the exceed.
5. **Previous window closed** — Wc5's harvest verified and its upload
   `archive_verified` (terminal dataset revision
   `ccaba7d67627bfe11bc8dc40a88b6e92993eb316`) — the index Wc6's own upload
   cites (REQ-XH-102; DF-R6.23-1's R6.45f share).

**Pre-registered dispositions** (each recorded `PASSED — …; basis: <verbatim>`):

- `run` — items 1–5 met and authorized; R6.45f dispatches for the Wc6 body
  (PROV → LOAD → the recorded in-window steps + the epoch-2 freeze → the
  re-proof on the thrice-repaired runner under the widened allowlist at
  D = 8 h → the frozen-list outcome → teardown → UPLOAD-Wc6 → SETTLE-Wc6).
- `not_run` — the re-proof does not run; `task_lists.state` stays `blocked`
  verbatim and every subject leg keeps its `withheld{un_reproofed_successor}` /
  `withheld{no_adjacent_donor}` — the honest state, never rescued.

Only an operator refusal stops the chain.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to the
triggering repair or Wc's closure remains (DEFERRALS rule 4); never silently
passed.
