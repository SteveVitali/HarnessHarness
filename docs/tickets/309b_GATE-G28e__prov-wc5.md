<!--
  Round-6 contract — run-time insert (GATE-G28e scope-decision, 2026-11-13).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Insert authorized by the operator 2026-11-13T01:10Z (LEDGER GATE DECISIONS
  scope-decision row GATE-G28e; operator authorization "a Wc5 re-proof window
  — the thrice-repaired runner + the widened allowlist" relayed by the
  coordinator — Wc4's re-proof ran to COMPLETION at D = 6 h on the
  thrice-repaired runner with ALL THREE repairs verified live — zero
  cell-terminate machinery exceptions, no scope-mint/egress signature, every
  H-allow cell produced its reward.txt — yet produced NON-COVERING verdicts
  on all 14 members: 13 `exclude:runner_infeasible:allowlist_gap{…}` (the
  frozen `fp-w1-q-x55` epoch-1 allowlist is Debian-family while the task env
  images are Ubuntu noble — the verifier's apt bootstrap legitimately denied)
  + query-optimize `repair:machinery` — a declared-data allowlist property,
  never machinery; the widened allowlist (the declared-data amendment's
  new-epoch `hh-egress-allowlist-freeze/1` record under `protocol_id
  fp-w1-q-x55` covering the Ubuntu noble apt hosts) is amended in parallel —
  a fifth repair window beyond the spec's at-most-once Wc, discharged at
  R6.45, and beyond the recorded Wc2, Wc3 and Wc4 amendments, discharged at
  R6.45b, R6.45c and R6.45d). The Σ-exceed is recorded: Σ declared ceilings
  26,233,810 + Wc5 2,565,580 = 28,799,390 µ$ ≈ $28.80 under the operator's
  authorization.
  This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
-->
# GATE-G28e — PROV-Wc5: authorize the fifth repair window's compute (scope amendment — the re-proof on the thrice-repaired runner under the widened allowlist)

- **Sequence:** 309b of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** Wc5 · **Alias:** GATE PROV-Wc5
- **base_branch:** current checkout
- **Depends on:** R6.45d
- **Readout:** `docs/build/readouts/GATE-G28e.md` (presented 2026-11-13; Status PASSED — run)

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

Wc5 is a **recorded spec deviation — the fifth repair window**: the spec's
gate table declares PROV-Wc "at most once" and the Wc row "at most one
contingency window" — discharged at R6.45; the recorded Wc2, Wc3 and Wc4
amendments were discharged at R6.45b (the mediated-egress repair verified
live, but the re-proof ran to the 3 h dead-man at 28/30 surfaces), R6.45c
(both carried repairs verified live — zero `cell-terminate` machinery
exceptions, no scope-mint/egress signature — yet the held leg again ran to
the dead-man, now at 5 h and 28/30 surfaces, and every H-allow cell exited
`exited{0}` then `verifier_unavailable:reward_missing`) and R6.45d (the
re-proof ran to COMPLETION at D = 6 h in ~4.6 h — DF-R6.45b-1 discharged —
with ALL THREE repairs verified live, yet 0/14 covering verdicts: 13
`exclude:runner_infeasible:allowlist_gap{…}` + query-optimize
`repair:machinery`). Wc4's deficit is a **declared-data allowlist property,
never machinery**: the committed `fp-w1-q-x55` epoch-1 freeze carries
Debian-family apt hosts (`deb.debian.org`, `security.debian.org`) while the
task env images are **Ubuntu noble** — the verifier's apt bootstrap is
legitimately denied → the typed `allowlist_gap` exclusion (REQ-XB-125's
declared-data rule). The remedy is the **declared-data amendment**: a
new-epoch `hh-egress-allowlist-freeze/1` record under the unchanged
`protocol_id fp-w1-q-x55` (epoch-2, `sha256:86def07e…`) adding
`archive.ubuntu.com`/`security.ubuntu.com`/`ports.ubuntu.com` — landed
committed on main (`28617cb0`) before dispatch as the window's recorded
prerequisite; REQ-XB-116's changed-list refusal refuses an unchanged
`protocol_id`+`epoch` carrying a different digest — a new epoch is the
amendment, never a silent list swap. The operator's scope-decision
authorized the re-proof under the widened allowlist ("a Wc5 re-proof window
— the thrice-repaired runner + the widened allowlist", 2026-11-13), and the
**Σ-exceed is recorded** — Σ declared ceilings 26,233,810 + Wc5 2,565,580 =
**28,799,390 µ$ ≈ $28.80** > $20 under the operator's authorization. This
gate is never treated as the declared §10.3 PROV-Wc row re-run; it is the
scope amendment's own record.

## Pre-registered thresholds

1. **The repairs are merged and the amendment landed** — the
   thrice-repaired runner lineage stands: the mediated-egress fix merged
   `5b8e8f28` (verified live in Wc2), the `pkill` repair merged `bb60a46f`
   (verified live in Wc3) and the verifier `reward_missing` repair merged
   `fe87c637` (verified live in Wc4 — every H-allow cell produced its
   `reward.txt`; `denied_hosts` carried the real
   `archive.ubuntu.com`/`security.ubuntu.com`) — `hh-w1`
   `sha256:b1ba7ffdc…`; **no new machinery repair** — and the widened
   allowlist's epoch-2 `hh-egress-allowlist-freeze/1` record
   (`sha256:86def07e…`, `docs/build/reports/
   2026-11-13-fp-egress-allowlist-freeze-epoch-2.json`) is committed on
   main (`28617cb0`) — the declared-data amendment the re-proof runs under
   (the drawn tasks, surfaces O/H-pub/H-allow, the R/R admit rule and the
   committed replacement orders untouched).
2. **Compute terms** — quote `pq-4183fe9dc342aa10` (the one quote path's
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
5. **Previous window closed** — Wc4's harvest verified and its upload
   `archive_verified` (terminal dataset revision
   `1f4b2ed5b3ebd632054d4879532c55efa59ad207`) — the index Wc5's own upload
   cites (REQ-XH-102; DF-R6.23-1's R6.45e share).

**Pre-registered dispositions** (each recorded `PASSED — …; basis: <verbatim>`):

- `run` — items 1–5 met and authorized; R6.45e dispatches for the Wc5 body
  (PROV → LOAD → the recorded in-window steps + the epoch-2 freeze → the
  re-proof on the thrice-repaired runner under the widened allowlist at
  D = 6 h → the frozen-list outcome → teardown → UPLOAD-Wc5 → SETTLE-Wc5).
- `not_run` — the re-proof does not run; `task_lists.state` stays `blocked`
  verbatim and every subject leg keeps its `withheld{un_reproofed_successor}` /
  `withheld{no_adjacent_donor}` — the honest state, never rescued.

Only an operator refusal stops the chain.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to the
triggering repair or Wc's closure remains (DEFERRALS rule 4); never silently
passed.
