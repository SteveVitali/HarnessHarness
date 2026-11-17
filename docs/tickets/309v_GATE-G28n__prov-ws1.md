<!--
  Round-6 contract — run-time insert under GATE-G30 (post-Wc10 scope decision)'s
  recorded operator ruling ("use your best judgement. what happened exactly and
  why do you need my ruling? I just want you to finish all of the benchmarks and
  stuff", readout docs/build/readouts/GATE-G30-postwc10-scope.md, Reading 1,
  2026-11-17 — recorded PASSED, operator's own ruling: directed completion) and
  the follow-up operator verbatim "continue" (2026-11-17). GATE-G28n is a normal
  gate, presented to the operator — NOT pre-answered by the insert. This contract
  CITES the spec; it does not copy a design. Executed contracts are frozen:
  amend by appending a dated note.
-->
# GATE-G28n — PROV-Ws1: authorize the withheld subject-legs window's compute + metered model (W2's PP0 + L1 RECORD corpus and W3's L1.0/L1.1/L1.2 on gate-A — the frozen-list discharge of both windows' `withheld{un_reproofed_successor}` predicates)

- **Sequence:** 309v of 325 · **Phase:** Round 6 · **Kind:** gate
- **Block:** Ws1 · **Alias:** GATE PROV-Ws1
- **base_branch:** current checkout
- **Depends on:** R6.45p
- **Also depends (record, not a chain row):** the post-Wc10 scope-decision readout `docs/build/readouts/GATE-G30-postwc10-scope.md` (recorded ruling)
- **Readout:** `docs/build/readouts/GATE-G28n.md` (owed — presented to the operator; not pre-answered)

> **Milestone gate — NOT an `implement-spec` input.** The chain STOPS here until the operator
> dispositions the readout. Pending operator review is RETURN PASS; a failed technical prerequisite
> is `blockedOn`. Agents never sign or infer approval; silence is not approval.

## Criterion (verbatim from the spec)

REQ-XH-004 (verbatim):

> | **REQ-XH-004** | Every paid execution ticket SHALL be preceded by a GATE row whose readout records verbatim the model authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket authorizes spend. | Readout template fields; the guard refuses dispatch of an execution row without a PASSED gate. · `AC-10.3-1` · D |

Ws1 is a **recorded scope continuation — the withheld subject legs, not a new window shape**: the
frozen list (`task_lists.state: frozen`, minted 2026-11-17 under Wcs1 — 17/17 slots covering)
discharges the `withheld{un_reproofed_successor:task_lists_state=blocked}` predicate verbatim on
every suppressed leg. The legs are the already-declared W2/W3 subject work, not new scope. Under
the R-5/A2 accounting the first-pass compute ceiling is $30; the readout carries the amended budget
table, never the superseded Σ-of-ceilings rule.

## What the readout must contain (pre-registered — every element present before presentation)

1. **The frozen-list discharge** — the Wcs1 `hh-task-lists-outcome/1` (state `frozen`, 17/17
   covering) cited verbatim; W2's and W3's withheld stage records cited verbatim.
2. **The owed legs** — the dependency order: PP0 (3 dev tasks × 3 hosted arms, AG3/AG5/AG6/AG8
   evidence — never verdict, REQ-XH-121 — + REQ-XH-127 hold dispositions) → corpus-precondition
   (W2's passed `hh-ps0-stage/1` + the PP0 evidence — PS0 is not re-run) → the L1 RECORD corpus
   (7 dev tasks × 4 = 28 trajectories through the metered proxy — the REQ-XH-114/REQ-XD-107
   natural corpus) → L1.0/L1.1/L1.2 on the gate-A partition (sqlite-with-gcov +
   cobol-modernization, 8 trajectories) over the frozen H-set `set_digest sha256:c451ed97…`
   (carried verbatim from the W3 export — a rebuild is a new participant needing a gate,
   REQ-XC-330) → `hh-l1-conformance/1` pass-or-fail only, bound `l1_pass_id`, `hh-l1-diff/1` per
   divergence.
3. **The model authorization** — `model_line: authorized`; the metered legs are the FP-S-authorized
   ≈ $12.64 binding (PP0 ≈ $2.17 + corpus ≈ $10.47 planning-binding) deferred only by the withhold —
   inside the pre-authorized envelope (A_eff $60 → root $48; balance 59.99988 unchanged since W2).
   `deliver-secret OLLAMA_API_KEY` declared (env-file → ssh stdin → tmpfs 0400, destroyed with the
   VM; `provided: yes` only).
4. **The declared data** — the dev partition's covering members (cancel-async-tasks,
   build-pmars, openssl-selfsigned-cert), the corpus's 7 dev tasks, the gate-A partition digest
   `4fe2dd2d…`, `hh-l1-canon` C01–C07, the frozen list; `retry_trials` = REQ-XH-016's same-window
   allowance (4), never carried; the runner built in-window at the pinned machinery tip carrying
   the exec-drain/transcript/model-port lineage (agent trials produce exactly the traffic the
   original defect hung on — the repairs are load-bearing here).
5. **The quote** — minted by the one quote path (`receipt_lib.py quoteid`), ceiling by the §3.10
   formula; metered model inside the binding line, compute ceiling declared.
6. **The A2 budget table** — first-pass compute under the amended accounting (completed Σ
   $22.556368 + Ws1 ceiling + the $0 W3′-shaped remainder + W4 planned ≤ $30).
7. **The §6 ladder** — `run | not_run | re-scope{…} | end-first-pass-early`.
8. **The trigger state** — Q-X62's recorded trigger instant 2026-11-24T21:42:46Z (7 days out):
   this window is the only path to a `full` G-L1 inside the trigger bound; a `not_run` or a
   window that misses the trigger leaves G-L1 to its recorded `fallback` semantics.

## The window this gate would authorize

- **Shape:** one VM, one dependency-ordered leg chain — PP0 → corpus-precondition → corpus →
  L1 legs — since corpus → L1 is an in-guest data dependency and each leg emits its own typed
  records. **This is a subject window, not an evidence re-proof:** the legs run under their own
  declared contracts (AG evidence, corpus record, conformance packet); typed withholds stand
  verbatim; nothing is fabricated or rescued.
- **Runner:** built in-window at the pinned tip `ddbfe9ef…` (the verified machinery lineage —
  exec-drain, transcripts, model-port guard — records-only commits atop `b3f72ab7`); in-window
  digests recorded, never patched.
- **Legs:**
  - **PP0** — 3 dev tasks (cancel-async-tasks, build-pmars, openssl-selfsigned-cert) × 1 × 3
    hosted arms (original mini-swe-agent 2.4.6, Pi, Claude Code) through the metered proxy;
    effort `high` enforced at the proxy; `hh-pp0-ag-evidence/1` per trial (AG3/AG5/AG6/AG8,
    machinery evidence only); hold dispositions per REQ-XH-127 verbatim.
  - **CORPUS** — gated on `corpus-precondition` (W2's passed PS0 + original-mini's PP0 AG rows
    passing AG3/AG5/AG6/AG8): original mini-swe-agent on the 7 dev tasks × 4 = 28 trajectories,
    recorded through the proxy (the REQ-XH-114 natural corpus; the partition binds dev/gate-A/
    gate-B by task — the committed `hh-fp-partition/1` `f7ddbc1f…`).
  - **L1** — on the gate-A partition's 8 trajectories: L1.0 (O twice against REPLAY(S) —
    the replay-stable prefix s(S)), L1.1 (H with REPLAY(S) + observation-replay exec; whole
    trajectory; gate), L1.2 (H with REPLAY(S) + live execution in a fresh HH cell; gated on
    requests 1…s(S)); `hh-l1-canon` C01–C07; first-divergence-fails; every divergence mints
    `hh-l1-diff/1`; emits `hh-l1-conformance/1` (BM1–BM19 coverage + turn-1 census + frozen
    rule table + stable-prefix histogram; pass-or-fail only; `l1_pass_id` bound to the exact
    H — REQ-XD-102/109).
- **Deadline D = 11 h** (metered legs ≈ 3–6 h at C = 4 + L1 ≈ 3.75 h + load ≈ 1 h + overhead
  ≈ 1.25 h), in-guest dead-man 675 min, h_r = 6 h, C = 4, ≤ 1 h harvest-only restart declared.
- **Stop rules:** a held original-mini PP0 → the corpus precondition types
  `withheld{admission:pp0_infra_hold}` — the REQ-XH-127(b) scope decision stands; any
  `machinery{…}` discordance or `verifier_unavailable:*` → `blockedOn` + a §6-shaped readout;
  an L1 fail is resolved per §7.4 (fix H or a version-bumped gate-B rerun — never relabelled).

## Dispositions (pre-registered)

`run` (the withheld subject-legs window as specified) | `not_run` (the legs stay withheld;
G-L1 falls to its trigger semantics) | `re-scope{…}` | `end-first-pass-early` — the operator
answers when presented; nothing is pre-answered.

## Out of scope

- The window's execution — R6.45q's body (fleshed after this gate).
- GATE-G29's `full`/`fallback` disposition — this window produces the `hh-l1-conformance/1`
  input; the decision gate reads it at presentation, never here.
- A gate-B rerun on a version-bumped canon — the §7.4/REQ-XD-106 contingency, gated separately.
- The W4 window and its FP-E gate — downstream of G-L1's disposition.
- Any allowlist epoch amendment — a declared-data call beyond this gate's scope.
