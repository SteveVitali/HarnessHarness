<!--
  Round-6 contract — run-time insert (GATE-G28c scope-decision, 2026-11-12).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Insert authorized by the operator 2026-11-12T03:30Z (LEDGER GATE DECISIONS
  scope-decision row GATE-G28c; operator authorization "a longer-D window + the
  pkill fix" relayed by the coordinator — Wc2's re-proof hit the 3 h dead-man at
  28/30 surfaces, DF-R6.45b-1, and the H-pub cell-terminate path depends on
  `pkill` absent in task env images, DF-R6.45b-2 — a third repair window beyond
  the spec's at-most-once Wc, discharged at R6.45, and beyond the recorded Wc2
  amendment, discharged at R6.45b). The Σ-exceed is recorded: Σ declared
  ceilings 21,416,790 + Wc3 2,251,440 = 23,668,230 µ$ ≈ $23.67 under the
  operator's authorization.
  This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
-->
# GATE-G28c — PROV-Wc3: authorize the third repair window's compute (scope amendment — longer-D re-proof + the `pkill` fix)

- **Sequence:** 308d of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** Wc3 · **Alias:** GATE PROV-Wc3
- **base_branch:** current checkout
- **Depends on:** R6.45b
- **Readout:** `docs/build/readouts/GATE-G28c.md` (presented 2026-11-12; Status PASSED — run)

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

Wc3 is a **recorded spec deviation — the third repair window**: the spec's
gate table declares PROV-Wc "at most once" and the Wc row "at most one
contingency window" — discharged at R6.45; the recorded Wc2 amendment was
discharged at R6.45b (the mediated-egress repair verified live, but the
re-proof ran to the 3 h dead-man at 28/30 surfaces — real trials execute,
slower than the window allowed — and no packet landed). Wc2's two machinery
debts need their own gate+window: **DF-R6.45b-1** — the re-proof's real-work
pace exceeds the 3 h D at R=3 (~3.3 h+ for held-30 alone, plus dev-4's ~8
surfaces — D → 5 h covers both legs in one window); **DF-R6.45b-2** — the
H-pub `cell-terminate` path's `pkill` dependency is absent in several task
env images (verbatim rc-127 machinery exceptions on 3 members). The
operator's scope-decision authorized the longer-D window + the `pkill` fix
("a longer-D window + the pkill fix", 2026-11-12T03:30Z), and the **Σ-exceed
is recorded** — Σ declared ceilings 21,416,790 + Wc3 2,251,440 =
**23,668,230 µ$ ≈ $23.67** > $20 under the operator's authorization. This
gate is never treated as the declared §10.3 PROV-Wc row re-run; it is the
scope amendment's own record.

## Pre-registered thresholds

1. **The repairs are merged** — the mediated-egress fix merged `5b8e8f28`
   (verified live in Wc2: the mediator's `requested`/`decided` rows land)
   **and the `pkill` repair** — `terminate_group`'s `/proc`-cmdline walk in
   `crates/hh-helper/src/podman.rs`, landed `b25adc21` on
   `codex/harnessharness-o-repair-4`, merged `bb60a46f` (machinery only; the
   drawn tasks, surfaces O/H-pub/H-allow, the R/R admit rule and the
   committed replacement orders untouched); the REQ-XC-330 new runner
   version recorded (`wc3-repair-record.json` — `hh-w1` `sha256:c8184fed…`,
   hh-helper `sha256:ed4e132a…`; the `carried_rebuild_guard` satisfied under
   this gate). The window's runner carries BOTH repairs.
2. **Compute terms** — quote `pq-55417e3bc7a1f673` (the one quote path's
   deterministic id), ceiling **2,251,440 µ$ ~$2.25** = r·(D + restart) +
   residual disk at h_r + egress = 314,140·(5+1) + 300·137·6 + 0.12·1
   = 1,884,840 + 246,600 + 120,000; D = 5 h dead-man; in-guest timer
   315 min; one ≤ 1 h harvest-only restart; h_r = 6 h; C = 4; ~38 oracle
   trials (the 14 `needs: wc_reproof` members × committed R — 10 held-out
   @R=3 + 4 dev @R=2); `retry_trials: 0`; W_p = 7/3 h ≈ 2.333 ≤ D − cut-off
   4.35 h; egress 1 GiB; e2-standard-8, debian-12, 300 GiB.
3. **$0 model** — `model_line: none`; `deliver-secret` ABSENT (REQ-XB-219 — a
   call against a $0-window declaration refuses); no model key in-window.
4. **Σ-exceed recorded** — never silently re-scoped; the authorization
   names the exceed.
5. **Previous window closed** — Wc2's harvest verified and its upload
   `archive_verified` (terminal dataset revision
   `1d4311d1f950b805c410be07202f9017ab72d4df`) — the index Wc3's own upload
   cites (REQ-XH-102; DF-R6.23-1's R6.45c share).

**Pre-registered dispositions** (each recorded `PASSED — …; basis: <verbatim>`):

- `run` — items 1–5 met and authorized; R6.45c dispatches for the Wc3 body
  (PROV → LOAD → the recorded in-window steps → the re-proof on the
  twice-repaired runner at D = 5 h → the frozen-list outcome → teardown →
  UPLOAD-Wc3 → SETTLE-Wc3).
- `not_run` — the re-proof does not run; `task_lists.state` stays `blocked`
  verbatim and every subject leg keeps its `withheld{un_reproofed_successor}` /
  `withheld{no_adjacent_donor}` — the honest state, never rescued.

Only an operator refusal stops the chain.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to the
triggering repair or Wc's closure remains (DEFERRALS rule 4); never silently
passed.
