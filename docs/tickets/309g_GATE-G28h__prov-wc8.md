<!--
  Round-6 contract — run-time insert (GATE-G28h scope-decision, 2026-11-15).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator under Q-X20).
  Insert authorized by the operator 2026-11-15T19:40:00Z (readout
  docs/build/readouts/GATE-G28h.md Status PASSED — run; operator verbatim
  "authorized, continue", interactive — Wc7's re-proof ran to COMPLETION
  inside D = 8 h on the four-times-repaired runner under the epoch-3
  widened allowlist: held 30/30 + dev 8/8 surfaces, both
  `hh-reproof-packet/1`s landed ~5.4 h wall — the duplicate-Host repair
  verified live (single `Host:` on the wire, zero denied hosts, the
  verifier streams landed as evidence so the §3.4 signature check
  classified `verifier_unavailable:bootstrap:signature` per cell) — yet
  0/14 covering verdicts: every member's H-allow cell produced
  `reward.txt = 0`. Diagnosed live as a SEVENTH machinery defect: the
  mediated forward's 8 MiB `max_body` cap (`read_response_full`) rejected
  Ubuntu `noble/universe`'s `Packages` index (~15 MB xz) → `502 Bad
  Gateway` → the verifier's apt bootstrap failed → curl/uvx never
  installed → pytest never ran → `echo 0 > reward.txt` → member verdicts
  rolled up `allowlist_gap{unknown}` by table rule (`denied_hosts` empty
  — every request was *allowed*; the 502 was the mediator's transport
  bound, not a deny). The repair landed (`abc8d850` — `FORWARD_MAX_BODY =
  128 MiB` on both mediated transports so the `noble/universe` index
  forwards → apt's bootstrap installs).
  An eighth repair window beyond the spec's at-most-once Wc, discharged
  at R6.45, and beyond the recorded Wc2, Wc3, Wc4, Wc5, Wc6 and Wc7
  amendments, discharged at R6.45b, R6.45c, R6.45d, R6.45e, R6.45f and
  R6.45g. The Σ-exceed is recorded: Σ declared ceilings 35,187,110 + Wc8
  3,193,860 = 38,380,970 µ$ ≈ $38.38 under the operator's authorization.
  This contract CITES the spec; it does not copy a design. Executed contracts
  are frozen: amend by appending a dated note.
-->
# GATE-G28h — PROV-Wc8: authorize the eighth repair window's compute (scope amendment — the D = 8 h re-proof on the seven-defect-repaired runner under the epoch-3 widened allowlist)

- **Sequence:** 309g of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** Wc8 · **Alias:** GATE PROV-Wc8
- **base_branch:** current checkout
- **Depends on:** R6.45g
- **Readout:** `docs/build/readouts/GATE-G28h.md` (presented 2026-11-15; Status PASSED — run)

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

Wc8 is a **recorded spec deviation — the eighth repair window**: the spec's
gate table declares PROV-Wc "at most once" and the Wc row "at most one
contingency window" — discharged at R6.45; the recorded Wc2, Wc3, Wc4,
Wc5, Wc6 and Wc7 amendments were discharged at R6.45b (the mediated-egress
repair verified live, but the re-proof ran to the 3 h dead-man at 28/30
surfaces), R6.45c (both carried repairs verified live — zero
`cell-terminate` machinery exceptions — yet the held leg again ran to the
dead-man, now at 5 h and 28/30 surfaces, and every H-allow cell exited
`exited{0}` then `verifier_unavailable:reward_missing`), R6.45d (the
re-proof ran to COMPLETION at D = 6 h in ~4.6 h — DF-R6.45b-1 discharged —
with ALL THREE repairs verified live, yet 0/14 covering verdicts: 13
`exclude:runner_infeasible:allowlist_gap{…}` + query-optimize
`repair:machinery`), R6.45e (the re-proof under the epoch-2 widened
allowlist ran REAL trials to 29/30 held surfaces — the widened allowlist
**verified live per-cell** — yet the 6 h dead-man fired with
query-optimize's `inconclusive{backend_terminated}`, the dev leg
unlaunched and no packet landed → `blocked` verbatim), R6.45f (the
re-proof ran to COMPLETION inside D = 8 h — held 30/30 + dev 8/8
surfaces, both `hh-reproof-packet/1`s landed ~4.8 h wall — the Wc5
window-bound deficit discharged — yet 0/14 covering verdicts: every
member's H-allow cell produced `reward.txt = 0`, rolled up
`allowlist_gap{unknown}` with `denied_hosts` empty — the duplicate-`Host`
defect) and R6.45g (the re-proof ran to COMPLETION inside D = 8 h under
the epoch-3 widened allowlist — held 30/30 + dev 8/8 surfaces, both
packets landed ~5.4 h wall — the duplicate-Host repair **verified live**:
single `Host:` on the wire, zero denied hosts, the verifier streams
landed as evidence so `bootstrap:signature` classified honestly — yet
0/14 covering verdicts again: every member's H-allow cell produced
`reward.txt = 0`).
Wc7's residual deficit is a **machinery property — the seventh defect**:
the mediated forward's `read_response_full` 8 MiB `max_body` cap
(pre-existing since `a7292296`/R4.5c) rejected Ubuntu `noble/universe`'s
`Packages` index (~15 MB xz / ~50 MB plain) → `502 Bad Gateway` on the
*allowed* request → the verifier's apt bootstrap failed → curl/uvx never
installed → pytest never ran → `reward.txt = 0` →
`verifier_unavailable:bootstrap:signature` per cell (classified honestly;
member verdicts rolled up `allowlist_gap{unknown}` by table rule —
`denied_hosts` empty). The repair is merged `abc8d850`
(`FORWARD_MAX_BODY = 128 MiB` on both mediated transports — ~4–8×
headroom over the real archive indexes; regression batteries
`crates/hh-env/tests/hallow_repair.rs` + `crates/hh-env/tests/r6_45.rs`
fixture-verified — this window's re-proof is its live verification act).
The epoch-3 widened allowlist (`sha256:787db9e1…` under `protocol_id
fp-w1-q-x55` — `release-assets.githubusercontent.com` in `verify` only)
is carried verbatim — no new declared-data amendment. With the proxy
headers AND the body cap fixed, the verifier's bootstrap installs →
pytest runs → real `reproofed` verdicts become possible →
`task_lists.state` can freeze. The operator's scope-decision authorized
the window verbatim ("authorized, continue", 2026-11-15T19:40:00Z), and
the **Σ-exceed is recorded** — Σ declared ceilings 35,187,110 + Wc8
3,193,860 = **38,380,970 µ$ ≈ $38.38** > $20 under the operator's
authorization. This gate is never treated as the declared §10.3 PROV-Wc
row re-run; it is the scope amendment's own record.

## Pre-registered thresholds

1. **The repair is merged and the epoch-3 amendment landed** — the
   seven-defect-repaired runner lineage stands: the mediated-egress fix
   merged `5b8e8f28` (verified live in Wc2), the `pkill` repair merged
   `bb60a46f` (verified live in Wc3), the verifier `reward_missing`
   repair merged `fe87c637` (verified live in Wc4 — every H-allow cell
   produced its `reward.txt`), the duplicate-Host repair merged
   `b8310b68` (verified live in Wc7 — single `Host:` on the wire, zero
   denied hosts, verifier streams landed as evidence) and the
   `max_body` repair merged `abc8d850` (`FORWARD_MAX_BODY = 128 MiB` on
   both mediated transports — fixture-verified; this window's re-proof
   is its live verification act) — **no new machinery repair in this
   amendment** — and the epoch-3 widened allowlist's
   `hh-egress-allowlist-freeze/1` record (`sha256:787db9e1…`,
   `docs/build/reports/2026-11-14-fp-egress-allowlist-freeze-epoch-3.json`,
   `verify`-only delta adding `release-assets.githubusercontent.com`
   under `protocol_id fp-w1-q-x55` — REQ-XB-116's changed-list refusal
   honored by the new epoch) is committed on main and carried verbatim
   (the drawn tasks, surfaces O/H-pub/H-allow, the R/R admit rule and
   the committed replacement orders untouched).
2. **Compute terms** — quote `pq-b14f50e9fd44e669` (the one quote path's
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
5. **Previous window closed** — Wc7's harvest verified and its upload
   `archive_verified` (terminal dataset revision
   `4868cc78c006a506db6b1efb54e33ad478420c90`) — the index Wc8's own upload
   cites (REQ-XH-102; DF-R6.23-1's R6.45h share).

**Pre-registered dispositions** (each recorded `PASSED — …; basis: <verbatim>`):

- `run` — items 1–5 met and authorized; R6.45h dispatches for the Wc8 body
  (PROV → LOAD → the recorded in-window steps + the epoch-3 freeze → the
  re-proof on the seven-defect-repaired runner under the epoch-3 widened
  allowlist at D = 8 h → the frozen-list outcome → teardown → UPLOAD-Wc8 →
  SETTLE-Wc8).
- `not_run` — the re-proof does not run; `task_lists.state` stays `blocked`
  verbatim and every subject leg keeps its `withheld{un_reproofed_successor}` /
  `withheld{no_adjacent_donor}` — the honest state, never rescued.

Only an operator refusal stops the chain.

## DEFERRALS rule

This gate does not pass while any `OPEN` row in `DEFERRALS.md` scoped to the
triggering repair or Wc's closure remains (DEFERRALS rule 4); never silently
passed.
