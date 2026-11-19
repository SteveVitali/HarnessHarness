<!--
  Round-6 contract — run-time gate insert under GATE-G30's recorded
  `re-scope{harden-again}` disposition (operator verbatim "I authorized,
  proceed" 2026-11-16; LEDGER `## GATE DECISIONS` row).
  Spec: docs/experiments-spec.md (canonical v0.3, ratified by the operator
  under Q-X20); the smoke window class exists under amendment row A1 (R-3).
  This contract CITES the spec; it does not copy a design. Executed
  contracts are frozen: amend by appending a dated note.
-->
# GATE-G28k — PROV-SMOKE post-repair diagnostic envelope: the non-evidence diagnostic smoke envelope's compute authorization (up to 2 smoke windows on the diagnostic recipe, $0 model, results never re-proof evidence)

- **Sequence:** 309p of 327 · **Phase:** Round 6 · **Kind:** gate
- **Block:** smoke · **Alias:** GATE PROV-SMOKE-DIAG
- **base_branch:** current checkout
- **Depends on:** R6.45m
- **Readout:** `docs/build/readouts/GATE-G28k.md` (owed — the envelope presentation lands before smoke4's PROV)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

The scope-decision basis is a readout record — `docs/build/readouts/GATE-G30-postwc9-scope.md` Status PASSED + its LEDGER `## GATE DECISIONS` row — never a manifest row (the manifest's GATE-G30 slot at row 312 is the unexecuted FP-E gate; the post-Wc9 scope decision is recorded under the slot's name only).

## Criterion (verbatim — the scope-decision this gate serves)

GATE-G30's recorded disposition is `re-scope{harden-again}` — "a targeted pass on
pypi-server's reward-0: a bounded smoke/diagnostic under a new envelope (the R-3 smoke
class is the existing instrument) to pin machinery-vs-task-provability before any
window" (`docs/build/readouts/GATE-G30-postwc9-scope.md`, the §6 ladder verbatim).

The records-only diagnosis (`docs/build/reports/2026-11-16-pypi-server-discordance-diagnosis.json`,
`hh-discordance-diagnosis/1`) already discriminated the residual: the `agent_timeout`
signature on pypi-server and make-doom-for-mips is produced by the shared
`exec_guarded` capture path's undrained pipes — **residual machinery, reparable** —
with the h_allow egress leg fully exonerated (identical signature on zero-mediated-tunnel
H-pub). The repair landed (`0c4e0547` drain threads + `4679635d` stream persistence on
`codex/harnessharness-exec-drain`, PR #404; the XO-05 `exec_drain` battery fails pre-fix,
passes post-fix). What GATE-G30's `harden-again` owes is the diagnosis's prescribed
post-repair verification, verbatim: **"a post-repair smoke leg on pypi-server +
make-doom-for-mips verifies against the recorded signature"** — declared compute under
an envelope, which only a gate authorizes (REQ-XH-004).

Cited requirement, verbatim:

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

## The authorization this readout records (under A1/R-3, bounded for the diagnostic)

- **$0 model** — `model_line: none`, no `deliver-secret`, no model key in-window, no metered calls (REQ-XB-219's rule applies verbatim).
- **Up to 2 smoke windows** on the exact diagnostic recipe (`hh-compute-decl/1` `mode: full`), each a distinct `window: smoke4|smoke5` — the verification smoke plus one contingency window on the identical recipe (retry or an inconclusive-result follow-up; a newly named defect class is a §6-shaped readout, never a silently re-scoped third window).
- **Each ceiling ≤ 1 250 000 µ$** by the §3.10 formula (r·(D + restart) + residual disk at h_r + egress — re-minted from the live rate table, never hand-computed); **Σ ceilings ≤ 2 500 000 µ$**.
- **Non-evidence (A1 verbatim)** — results are labelled "non-evidence smoke (internal; never re-proof evidence)", never folded into `hh-reproof-packet/1` or `hh-reproof-register/1`, never cited as re-proof or parity evidence; `task_lists.state` stays `blocked` verbatim through this envelope — the smoke verifies machinery, it does not re-grade the member.

## What the readout must carry

1. GATE-G30's disposition as the authorization basis (the GATE DECISIONS row cited).
2. The A1 label rule stated verbatim.
3. **Each smoke's declaration and quote** — minted by the one quote path (`receipt_lib.py quoteid` over the canonical `hh-compute-quote/1`) — **appended to the readout as an agent-drafted row with its hash, under the envelope, before that window's PROV.** A quote outside the envelope stops for a separate operator answer.
4. The diagnostic recipe verbatim: the repaired runner's declared pinned commit (the exec-drain tip carrying `0c4e0547` + `4679635d`), the member/plane set, the pass/fail clauses keyed to the recorded signature.
5. The prediction-track table (A3) and the §6 re-scope ladder.
6. Dispositions: `envelope_authorized` (the named smokes may PROV under it) | `not_run`.

## Terms (pre-registered)

- Window shape per smoke: `mode: full`, e2-standard-8, debian-12, the declared podman/crun/netavark pins; boot disk 150 GiB; **D = 1.5 h** dead-man; ≤ 0.5 h harvest-only restart; h_r 6 h; C = 4; egress 1 GiB (estimated); `model_line: none`, `deliver-secret` ABSENT.
- Runner: the exec-drain tip carrying the repair (declared pinned commit on the declaration; R-10's smoke-runner rule applies verbatim — in-window rebuilds recorded, never evidence-window behavior).
- Cells: H-allow r = 1 on `pypi-server` and `make-doom-for-mips`; H-pub r = 1 on both — the two members carrying the recorded `agent_timeout` signature, on both planes it appeared on.
- Pass clauses (the recorded signature): every cell's agent exec completes (`exited`, never lapsed mid-stream), the captured agent transcript exceeds the recorded pipe bound (~64 KiB) end-to-end, `collect/agent-{stdout,stderr}.txt` land byte-pinned, zero 5xx / eof-only tunnels on the H-allow legs. Reward values are reported, never required — the smoke verifies machinery, not task outcome.

## Out of scope

- Any evidence-window act: the smoke results are never re-proof evidence, never a `hh-reproof-packet/1` input, never a frozen-list outcome; `task_lists.state` stays `blocked` verbatim.
- A Wc10 or any same-shape re-proof window — the A3/R-8 stop rule stands verbatim; the frozen-list path returns through its own gate after the diagnostic lands.
- Re-grading pypi-server — the member's `repair:machinery` classification stays verbatim through the smoke; what follows it (re-verified machinery → the ladder's next option) is a later gate's call.

> Dated note 2026-11-19 (DF-CAP6.1-4 / CAP6.2 cell-normalization record): the criterion heading
> read `## Criterion (the scope-decision this gate serves)` — a variant of the guard's required
> `## Criterion (verbatim …)` heading (`[gate-criterion]` diagnostic). The section's content is
> unchanged — the GATE-G30 scope-decision criterion verbatim (the diagnostic envelope this gate
> served); the heading variant predated execution. Normalized 2026-11-19 to
> `## Criterion (verbatim — the scope-decision this gate serves)`; the original heading text is
> preserved verbatim inside this note.
