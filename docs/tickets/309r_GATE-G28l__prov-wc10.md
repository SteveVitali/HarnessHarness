<!--
  Round-6 contract — run-time insert under GATE-G29's recorded operator ruling
  ("route to a re-proof PROV gate", readout docs/build/readouts/GATE-G29.md,
  Reading 1, 2026-11-16 — G-L1 recorded PASSED with neither `full` nor
  `fallback`; `full` stays reachable through re-proof → frozen list → withheld
  L1 legs before the Q-X62 trigger 2026-11-24T21:42:46Z). GATE-G28l is a normal
  gate, presented to the operator — NOT pre-answered. This contract CITES the
  spec; it does not copy a design. Executed contracts are frozen: amend by
  appending a dated note.
-->
# GATE-G28l — PROV-Wc10: authorize the Wc10 evidence re-proof's compute (the re-proof on the verified machinery at pinned `b3f72ab7`, epoch-5, D = 8 h, `model_port` in the dynamic/private band)

- **Sequence:** 309r of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** Wc10 · **Alias:** GATE PROV-Wc10
- **base_branch:** current checkout
- **Depends on:** R6.45n
- **Readout:** `docs/build/readouts/GATE-G28l.md` (owed — presented to the operator; not pre-answered)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

REQ-XH-004 (verbatim):

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

Wc10 is a **recorded spec deviation — a tenth repair window**: PROV-Wc "at most once" was discharged
at R6.45; Wc2–Wc9 amendments discharged at R6.45b–R6.45m. Under the R-5/A2 accounting the first-pass
compute ceiling is $30 (completed windows at billed/estimate figures, authorized-but-unrun at
ceilings); the readout carries the amended budget table, never the superseded Σ-of-ceilings rule.

## What the readout must contain (pre-registered — every element present before presentation)

1. **The Wc9 outcome** — `task_lists.state: blocked` verbatim, the three declared residual classes,
   and the discharged `repair:machinery` trigger's repair+verification chain (smoke4/smoke5).
2. **The smoke-verification evidence** — `docs/build/runs/R6.45n.md` + `R6.45n-smoke5.md`: both named
   machinery defect classes carry repair + live verification on the identical recipe; no defect
   stands.
3. **The machinery tip** — declared pinned commit `b3f72ab7…` carrying every repair (exec-drain,
   transcript persistence, model-port guard), each recorded with its regression (REQ-XC-330).
4. **The declared-data change** — window cfgs' `model_port` moved into the dynamic/private band
   (≥ 49152) the guard mandates; `egress_port` unchanged (3128/3130, spec-pinned pattern).
5. **The quote** — minted by the one quote path (`receipt_lib.py quoteid`), ceiling by the §3.10
   formula; `model_line: none`.
6. **The A2 budget table** — first-pass compute under the amended accounting; the trigger deadline
   (Q-X62: 2026-11-24T21:42:46Z) as the scheduling bound for this window and the follow-on L1-legs
   window's own PROV.
7. **The §6 re-scope ladder** — `run | not_run | re-scope{harden-again} | re-scope{exclude-and-replace}
   | re-scope{end-first-pass-early}`.

## The window this gate would authorize

- **Shape:** R6.45m's evidence-window shape, cloned — the evidence rules unchanged: no in-window
  fixes, `retry_trials: 0`, typed outcomes, never rescued. **R-10 does not apply** (evidence window).
- **Runner:** built in-window at the declared pinned commit `b3f72ab784cedc73457c87c4817a23a45873650a`
  — the verified tip carrying exec-drain + transcript persistence + the model-port guard.
- **Cfg delta:** `model_port` held `49152` / dev `49153` (the band the guard mandates; Wc9's 8080/8090
  now refuse typed), `egress_port` unchanged.
- **Freeze:** epoch-5 `sha256:e61a6ed9…` carried verbatim.
- **Members:** the 14 `needs: wc_reproof` members at their committed R (held 10 @ R=3 + dev 4 @ R=2 =
  38 trials) — the full re-proof re-mints covering verdicts; the member/slot narrowing is the fleshed
  body's own declared-data decision under REQ-XH-116's replacement orders.
- **Early-inspection checkpoint** (A3/R-8 shape): as Wc9's — at the first H-allow cells of ≥ 3
  distinct members, harvest and check for machinery signatures; else `early_stop` typed outcome.
- **Post-Wc10 stop rule:** any `machinery{…}` discordance or `verifier_unavailable:*` after the
  window → `blockedOn` + a §6-shaped readout; no Wc11 is drafted.

## Dispositions (pre-registered)

`run` (the evidence window as specified) | `not_run` (the task list stays `blocked`) |
`re-scope{harden-again}` | `re-scope{exclude-and-replace}` | `re-scope{end-first-pass-early}` — the
operator answers when presented; nothing is pre-answered.

## Out of scope

- The window's execution — R6.45o's body (fleshed after this gate).
- The follow-on L1 corpus legs — they admit only on a frozen list and run under their own PROV
  (a W3′-shaped window), gated separately under REQ-XH-004.

> Dated note 2026-11-19 (DF-CAP6.1-4 / CAP6.2 cell-normalization record): the `Depends on` header
> read `R6.45n, GATE-G29 (recorded ruling)` — the parenthetical annotation made the cell
> unresolvable to the chain parser (guard diagnostic `[dependency]`), and GATE-G29's manifest row
> also sits after this insert (a `[forward-dependency]`). The dependency itself is real and
> discharged: GATE-G29's ruling (route to a re-proof PROV gate, operator verbatim 2026-11-16) and
> its Reading-3 `full` disposition (2026-11-18) are recorded decisions — the original cell text is
> preserved verbatim inside this note. Normalized 2026-11-19 to the chain-resolvable
> `Depends on: R6.45n`; the narrative dependency on GATE-G29's recorded ruling stands here, not in
> the parser-facing cell.
