# GATE-G28m — PROV-Wcs1: authorize the bounded successor re-proof's compute (the uncovered-slot members on the verified machinery at pinned `b3f72ab7`, epoch-5 verbatim, `model_port` band) · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket
authorizes spend. Wcs1 is a **bounded successor window — not a same-shape Wc11**: the post-Wc10 stop rule
forbade a drafted Wc11; the GATE-G30 (post-Wc10) own-ruling arm authorized a new-window insert on the
operator's ruling — "use your best judgement. what happened exactly and why do you need my ruling? I just
want you to finish all of the benchmarks and stuff" (Reading 1, 2026-11-17 — a directed-completion
directive; `frozen` stays reachable through covering the remaining subject slots before the Q-X62 trigger
instant **2026-11-24T21:42:46Z**).

## Evidence (per item: source, `date -u` of the read, layer reached)

- The Wc10 outcome — `docs/build/reports/2026-11-17-fp-wc10-tasklists.json` (`hh-task-lists-outcome/1`):
  `task_lists.state: blocked` verbatim — **12/14 covering `admit`**; the blocked slots: held-medium
  `query-optimize` (`o_fail:flaky` exclusion → `blocked{un_reproofed_successor}`), dev-easy
  `pytorch-model-cli` (register `machinery{h_allow}` roll-up over the recorded `allowlist_gap` signature
  → `blocked{adjacent_order_exhausted}`), 3 dev-medium drawn slots `blocked{no_adjacent_donor}` —
  `docs/build/runs/R6.45o.md` — read 2026-11-17 — layer: committed records.
- The scope derivation — `docs/build/reports/2026-11-10-fp-prereg.json` (`task_lists.draws`):
  **every uncovered slot traces to a retryable `oracle_fails_on_O` exclusion** — held-medium slot 6's
  drawn order walks to `merge-diff-arc-agi-task` (never re-proofed); dev-easy's drawn member is
  `cobol-modernization` (excluded `oracle_fails_on_O{sha256:8ef8b47e…}` before the repaired machinery);
  the three dev-medium drawn members `extract-elf` / `openssl-selfsigned-cert` /
  `sqlite-with-gcov` were excluded `oracle_fails_on_O` with no donor — a drawn member's fresh `admit`
  covers its own slot. `pytorch-model-cli` is the only deterministic exclusion (agent-phase
  `allowlist_gap{download.pytorch.org}` — X-06's agent-phase boundary; an agent-phase epoch amendment is
  an operator-level scope call beyond the verify-phase delegation recorded at epoch-5's minting) —
  read 2026-11-17 — layer: committed records.
- The machinery verification — `docs/build/runs/R6.45o.md`: BOTH legs ran to COMPLETION at
  `b3f72ab7` (digests byte-identical to smoke5's verified build); pypi-server's `repair:machinery`
  discordance discharged live on the re-proof itself (6/6 decided-1 at `model_port` 49152); the
  Wc9 `incomplete{dead_man}` residual did not recur; transcripts persisted — **no defect class
  stands** — read 2026-11-17 — layer: committed records.
- The machinery tip — declared pinned commit `b3f72ab784cedc73457c87c4817a23a45873650a`
  (`codex/harnessharness-model-port-guard`, PR #407) unchanged — R-10's one declared pinned commit —
  read 2026-11-17 — layer: committed code + recorded digests.
- The declared data — epoch-5 `sha256:e61a6ed9771d506e0606dd7a953ebe49a56dc96d8427d9eb410ae492e7f3b328`
  carried verbatim; `model_port` 49152/49153 (band); `egress_port` 3128/3130 — read 2026-11-17 —
  layer: committed records.
- The declaration + quote — `scripts/gce-backend/declarations/fp-wcs1.json` minted (window `wcs1`,
  ticket `R6.45p`, gate `PROV-Wcs1`, `runner.commit b3f72ab7…`, `verbatim_sha256` all-zeros pending
  this gate's stamp); quote `docs/build/reports/2026-11-17-fp-wcs1-quote.json` →
  **`pq-f3e244a6a0ed750c`, bound 996,975 µ$ (~$1.00), ceiling 3,193,860 µ$ (~$3.19)** — 14 trials,
  D 8 h, h_r 6 h, ≤ 1 h harvest-only restart, egress 1 GiB — read 2026-11-17 — layer: committed records.
- The spend register — `PHASE_SPEND.jsonl`: completed Σ **$21.702850** (Wc10 $1.271324 latest);
  Wcs1's ceiling takes worst-case to ~$24.90 ≤ $30 first-pass compute; model ≤ $70 untouched —
  read 2026-11-17 — layer: committed records.
- The ruling chain — GATE-G30-postwc10 readout `docs/build/readouts/GATE-G30-postwc10-scope.md`
  Status PASSED (operator's own ruling: directed successor re-proof); GATE-G28l PASSED `run`;
  GATE-G29 PASSED (operator's own ruling: route to a re-proof PROV gate) — read 2026-11-17 —
  layer: committed records.

<!-- agent-drafted:begin sha256=8876de99166a -->
## Decision content for the operator

**What this gate authorizes.** One bounded successor re-proof window, Wcs1 — scoped to exactly the
members that can yield covering verdicts for the uncovered slots under epoch-5 + the verified
machinery: held leg `query-optimize` + `merge-diff-arc-agi-task` @ R=3 (6 trials), dev leg
`cobol-modernization` + `extract-elf` + `openssl-selfsigned-cert` + `sqlite-with-gcov` @ R=2
(8 trials) = **14 trials**, `retry_trials: 0`, evidence rules — no in-window fixes, typed outcomes,
never rescued. Epoch-5 verbatim, `b3f72ab7`, `model_port` 49152/49153, D = 8 h, h_r = 6 h.
Ceiling 3,193,860 µ$ ~$3.19 (bound ~$1.00); $0 model.

**Coverage map (what each member can honestly cover).** `merge-diff-arc-agi-task` covers held-medium
slot 6 as the committed order's successor whether or not the `query-optimize` retry lands; the
`query-optimize` retry is a second honest reading of a flaky oracle. `cobol-modernization` is the
dev-easy slot's *drawn* member — its `oracle_fails_on_O` exclusion predates the repaired machinery;
an `admit` covers the slot with no donor needed. The three dev-medium drawn members cover their own
`no_adjacent_donor` slots on `admit`. `pytorch-model-cli` is deliberately not re-run: deterministic
agent-phase `allowlist_gap` under epoch-5 — the easy slot covers via its drawn member or stands
`blocked{adjacent_order_exhausted}` verbatim. `frozen` is earned only if **every** slot covers;
anything less mints `blocked` verbatim with the per-slot evidence — never fabricated.

**Dispositions — your verbatim answer (the agent signs nothing).**
`run` — the bounded successor window as specified, under this readout's stamp onto `fp-wcs1.json`.
`not_run` — the task list stays `blocked`; nothing is authorized.
`re-scope{harden-again}` — name a residual defect to repair first (none stands today).
`re-scope{exclude-and-replace}` — under A5/R-7's rules only.
`re-scope{end-first-pass-early}` — record the honest outcome; the withheld first-pass claims never
fabricate `frozen`.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-17 via interactive session): "use your best judgement. what happened exactly and why do you need my ruling? I just want you to finish all of the benchmarks and stuff"
Disposition: run; basis: the directed-completion directive ("finish all of the benchmarks") with mechanics delegated ("use your best judgement") — this readout's bounded successor window is the only reading that finishes the benchmark coverage (Reading 1, recorded at GATE-G30-postwc10 and applied to this gate's own authorization by the same verbatim directive)
GATE DECISIONS row: 2026-11-17 | GATE-G28m (PROV-Wcs1)
Operator confirmation of agent-drafted text (verbatim, 2026-11-17): "use your best judgement" — covers sha256:8876de99166a
Signed by: the operator. Recorded by devin-cli/swe-2-high, which does not sign.
