<!--
  Round-6 contract — run-time insert under GATE-G30 (post-Wc10 scope decision)'s
  recorded operator ruling ("use your best judgement. what happened exactly and
  why do you need my ruling? I just want you to finish all of the benchmarks and
  stuff", readout docs/build/readouts/GATE-G30-postwc10-scope.md, Reading 1,
  2026-11-17 — recorded PASSED, operator's own ruling: a directed successor
  re-proof to cover the remaining subject slots). GATE-G28m is a normal gate,
  presented to the operator — NOT pre-answered by the insert. This contract
  CITES the spec; it does not copy a design. Executed contracts are frozen:
  amend by appending a dated note.
-->
# GATE-G28m — PROV-Wcs1: authorize the bounded successor re-proof's compute (the uncovered-slot members on the verified machinery at pinned `b3f72ab7`, epoch-5 verbatim, `model_port` band)

- **Sequence:** 309t of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** Wcs1 · **Alias:** GATE PROV-Wcs1
- **base_branch:** current checkout
- **Depends on:** R6.45o
- **Also depends (record, not a chain row):** the post-Wc10 scope-decision readout `docs/build/readouts/GATE-G30-postwc10-scope.md` (recorded ruling)
- **Readout:** `docs/build/readouts/GATE-G28m.md` (owed — presented to the operator; not pre-answered)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

REQ-XH-004 (verbatim):

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

Wcs1 is a **recorded scope continuation — a bounded successor window, not a same-shape Wc11**: the
post-Wc10 stop rule forbade a drafted Wc11; the GATE-G30 own-ruling arm authorizes a new-window
insert on the operator's ruling. Under the R-5/A2 accounting the first-pass compute ceiling is $30;
the readout carries the amended budget table, never the superseded Σ-of-ceilings rule.

## What the readout must contain (pre-registered — every element present before presentation)

1. **The Wc10 outcome** — `task_lists.state: blocked` verbatim, 12/14 covering `admit`, the exact
   per-slot blocked states this window exists to cover.
2. **The scope derivation** — which slots are coverable and by which members (committed orders +
   drawn members), and which are not (pytorch-model-cli's deterministic `allowlist_gap` under
   epoch-5 — X-06's verify-phase-only amendment boundary stands; no epoch-6 minted).
3. **The machinery state** — declared pinned commit `b3f72ab7…` unchanged (the verified tip); the
   Wc10 live evidence the repairs held on the evidence leg itself.
4. **The declared data** — epoch-5 `sha256:e61a6ed9…` carried verbatim; `model_port` 49152/49153
   band; `retry_trials: 0` per leg.
5. **The quote** — minted by the one quote path (`receipt_lib.py quoteid`), ceiling by the §3.10
   formula; `model_line: none`.
6. **The A2 budget table** — first-pass compute under the amended accounting (completed Σ
   $21.702850 + Wcs1 ceiling + W4 planned ≤ $30).
7. **The §6 re-scope ladder** — `run | not_run | re-scope{harden-again} | re-scope{exclude-and-replace}
   | re-scope{end-first-pass-early}`.

## The window this gate would authorize

- **Shape:** the Wc-line evidence-window shape, bounded — the evidence rules unchanged: no
  in-window fixes, `retry_trials: 0`, typed outcomes, never rescued. **R-10 does not apply.**
- **Runner:** built in-window at `b3f72ab784cedc73457c87c4817a23a45873650a` — in-window digests
  recorded, expected byte-identical to smoke5/Wc10's verified build; a differing digest is
  recorded, never patched.
- **Members — the covering candidates for every coverable uncovered slot:**
  - held @ R=3, epoch-5: `query-optimize` (retry of Wc10's `o_fail:flaky` exclusion — oracle
    nondeterminism is a task-side property a re-run reads again) and `merge-diff-arc-agi-task`
    (the medium committed order's next member — covers held-medium slot 6 either way).
  - dev @ R=2, epoch-5: `cobol-modernization` (the easy slot's **drawn** member — its
    `oracle_fails_on_O` exclusion predates the repaired machinery) and `extract-elf` /
    `openssl-selfsigned-cert` / `sqlite-with-gcov` (the three medium-cell **drawn** members —
    `blocked{no_adjacent_donor}` is a donor absence, not a member impossibility; a drawn member's
    fresh `admit` covers its own slot).
  - `pytorch-model-cli` is NOT re-run: its exclusion is the deterministic agent-phase
    `allowlist_gap{download.pytorch.org}` — provably futile under epoch-5 (X-06's agent-phase
    boundary); an agent-phase grant is an epoch amendment beyond the delegated verify-phase scope.
- **Deadline D = 8 h** (the recipe bound; ~1.5–3 h expected at 14 trials), in-guest dead-man
  495 min, h_r = 6 h, C = 4, ≤ 1 h harvest-only restart declared.
- **Outcome rule:** the successor fold mints `hh-task-lists-outcome/1` window `wcs1` — `frozen`
  only if every slot admits a covering-verdict member, `blocked` verbatim otherwise; the
  Wc10 exclusions and register roll-ups stand append-only and are cited, never rewritten.
- **Post-Wcs1 rule:** a `machinery{…}` discordance or `verifier_unavailable:*` after the window →
  `blockedOn` + a §6-shaped readout; no further same-shape window is drafted.

## Dispositions (pre-registered)

`run` (the bounded successor window as specified) | `not_run` (the task list stays `blocked`) |
`re-scope{harden-again}` | `re-scope{exclude-and-replace}` | `re-scope{end-first-pass-early}` — the
operator answers when presented; nothing is pre-answered.

## Out of scope

- The window's execution — R6.45p's body (fleshed after this gate).
- The follow-on L1 corpus legs — they admit only on a frozen list and run under their own PROV
  (a W3′-shaped window), gated separately under REQ-XH-004.
- Any agent-phase allowlist amendment (a new epoch granting `download.pytorch.org` to the agent
  phase) — an operator-level declared-data scope call beyond the recorded verify-phase delegation.
