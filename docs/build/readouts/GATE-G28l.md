# GATE-G28l — PROV-Wc10: authorize the Wc10 evidence re-proof's compute (the re-proof on the verified machinery at pinned `b3f72ab7`, epoch-5, D = 8 h, `model_port` in the dynamic/private band) · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

REQ-XH-004: every paid execution ticket is preceded by a GATE row whose readout records verbatim the model
authorization, the compute authorization (separately), the quote id and the hard-cap formula. No ticket
authorizes spend. Wc10 is a **recorded spec deviation — a tenth repair window**: PROV-Wc "at most once"
was discharged at R6.45; Wc2–Wc9 amendments discharged at R6.45b–R6.45m. The gate rides under GATE-G29's
recorded operator ruling (Reading 1, 2026-11-16 — "route to a re-proof PROV gate": neither `full` nor
`fallback` recorded at G-L1; `full` stays reachable through re-proof → frozen list → withheld L1 legs
before the Q-X62 trigger instant **2026-11-24T21:42:46Z**).

## Evidence (per item: source, `date -u` of the read, layer reached)

- The Wc9 outcome — `docs/build/reports/2026-11-16-fp-wc9-tasklists.json` (`hh-task-lists-outcome/1`):
  `task_lists.state: blocked` verbatim — 9/14 covering `reproofed`; the three declared residuals:
  pypi-server `repair:machinery` (append-only discordance), dev leg `incomplete{dead_man}` (74/76 cells
  at the in-guest STOP), pytorch-model-cli `allowlist_gap{download.pytorch.org}` (A5/R-7 legit
  exclusion) + 3 standing `blocked{no_adjacent_donor}` — `docs/build/runs/R6.45m.md` — read
  2026-11-16 — layer: committed records.
- The machinery verification chain — `docs/build/runs/R6.45n.md` + `R6.45n-smoke5.md`: BOTH named
  defect classes carry repair + live verification on the identical recipe. smoke4: every leg past the
  ~64 KiB pipe bound, transcripts persisted, `machinery{model-port-collision}` diagnosed live
  (EADDRINUSE on the task's own `pypi-server -p 8080`). smoke5: 4/4 cells `exited{0}` reward_ppm
  1000000 — pypi-server/H-allow ~96 s vs deterministic `agent_timeout` ×2; `render-policy
  --model-port 8080` typed `Unsupported` refusal; stores clean (185/185 + 179/179 paired, all-allow,
  eof-only, zero 5xx, `denied_hosts []`); **no defect class stands** — read 2026-11-16 — layer:
  committed records.
- The machinery tip — declared pinned commit `b3f72ab784cedc73457c87c4817a23a45873650a`
  (`codex/harnessharness-model-port-guard`, PR #407 — R-10: one declared pinned commit; in-window
  digests recorded: hh-w1 `sha256:621b9444…`, mediator `sha256:af3a0d86…`, helper `sha256:727b7b83…`)
  carrying R6.45j/k/l repairs + `0c4e0547`/`4679635d`/`6801febf` + `b07adfdd` model-port guard —
  read 2026-11-16 — layer: committed code + recorded digests.
- The declared-data change — window cfgs' `model_port` moves into the band the guard mandates:
  held `8080` → `49152`, dev `8090` → `49153` (Wc9's values now refuse typed — the intended loud
  failure); `egress_port` unchanged (3128/3130 — spec-pinned; the egress collision class is
  out-of-scope per the repair's record) — read 2026-11-16 — layer: committed code + declared data.
- The freeze — epoch-5 `docs/build/reports/2026-11-16-fp-egress-allowlist-freeze-epoch-5.json`
  `sha256:e61a6ed9771d506e0606dd7a953ebe49a56dc96d8427d9eb410ae492e7f3b328` carried verbatim
  (DF-R6.45l-1 OPEN for the runtime-derivation arm — smoke5 surfaced zero runtime-transitive denies)
  — read 2026-11-16 — layer: committed records.
- The declaration + quote — `scripts/gce-backend/declarations/fp-wc10.json` minted (window `wc10`,
  ticket `R6.45o`, gate `PROV-Wc10`, `runner.commit b3f72ab7…`, `verbatim_sha256` all-zeros pending
  this gate's stamp); quote `docs/build/reports/2026-11-16-fp-wc10-quote.json` →
  **`pq-a2d99ef16c6c3814`, ceiling 3,193,860 µ$ (~$3.19)** — same recipe as Wc9 (38 trials, D 8 h,
  h_r 6 h, ≤ 1 h harvest-only restart, egress 1 GiB) — read 2026-11-16 — layer: committed records.
- The spend register — `PHASE_SPEND.jsonl`: completed Σ **$20.431526** (smoke5 $0.224553 latest);
  Wc10's ceiling takes worst-case to ~$23.62 ≤ $30 first-pass compute; model ≤ $70 untouched —
  read 2026-11-16 — layer: committed records.
- The ruling chain — GATE-G29 readout `docs/build/readouts/GATE-G29.md` Status PASSED (operator's own
  ruling: route to a re-proof PROV gate); GATE-G30-postsmoke5 `continuation{frozen-list-gate}`;
  GATE-G30-postsmoke4 `re-scope{harden-again}`; GATE-G28k `envelope_authorized` (closed — 2/2
  windows spent) — read 2026-11-16 — layer: committed records.

<!-- agent-drafted:begin sha256=713df7089baa -->
## Decision content for the operator

**What this gate authorizes.** One evidence re-proof window, Wc10 — the Wc9 recipe cloned on the
verified machinery: the 14 `needs: wc_reproof` members at their committed R (38 trials,
`retry_trials: 0`, evidence rules — no in-window fixes), epoch-5 verbatim, declared pinned commit
`b3f72ab7`, `model_port` moved into the dynamic/private band (49152/49153 — declared data), D = 8 h,
the R-8-shaped early-inspection checkpoint, the frozen-list outcome verbatim, teardown/harvest/
upload/settle. Ceiling 3,193,860 µ$ ~$3.19; $0 model.

**Why now.** The machinery line is discharged — both named defect classes carry repair + live
verification; the residual `repair:machinery` discordance that held the frozen list is repaired.
A re-proof minting `task_lists.state: frozen` re-opens the withheld L1 corpus legs (W3's
`withheld{un_reproofed_successor}` rows) toward G-L1 `full` — still ~8 days inside the Q-X62
trigger. The honest caveat stands verbatim: the dev leg's `incomplete{dead_man}` cells and the
`allowlist_gap` exclusion are not machinery — the re-proof records them as they land, and
`frozen` is earned only if every subject slot gets a covering verdict.

**Dispositions — your verbatim answer (the agent signs nothing).**
`run` — the evidence window as specified, under this readout's stamp onto `fp-wc10.json`.
`not_run` — the task list stays `blocked`; nothing is authorized.
`re-scope{harden-again}` — name a residual defect to repair first (none stands today).
`re-scope{exclude-and-replace}` — under A5/R-7's rules only (a named denied host / O-fail —
`pytorch-model-cli`'s `allowlist_gap` is already the recorded legit shape).
`re-scope{end-first-pass-early}` — record the honest outcome; skip W4; the withheld first-pass
claims never fabricate `frozen`.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
Operator decision (verbatim, received 2026-11-16T23:26:47Z via interactive): "ok, run and continue"
Disposition: run; basis: "ok, run and continue"
GATE DECISIONS row: 2026-11-16T23:26:47Z | GATE-G28l (PROV-Wc10)
Operator confirmation of agent-drafted text (verbatim, 2026-11-16T23:26:47Z): "ok, run and continue" — covers sha256:713df7089baa
Signed by: the operator. Recorded by devin-cli/swe-2-high, which does not sign.