# GATE-G30 — post-smoke5 scope decision: the diagnostic envelope is CLOSED and BOTH named machinery defect classes carry recorded repair + live verification (`machinery{exec-pipe-undrained}` smoke4, `machinery{model-port-collision}` smoke5 — 4/4 cells `exited{0}` reward 1 incl. the previously deterministic-timeout member+plane); no new defect class named → `blockedOn` lifts toward the frozen-list continuation's own gate per the 309q exit clause; no further spend under this envelope · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

Ticket `docs/tickets/309q_R6.45n__diag-smoke.md` exit conditions: "Verification recorded (the signature
absent on the repaired path) → `blockedOn` lifted toward the frozen-list continuation's own gate — this
row does not draft it." OR "envelope exhausted **or a newly named defect class** → `blockedOn` + a
§6-shaped readout; no further spend." smoke5 discharged **both arms at once, in the favorable
direction**: the recorded signature is absent on the repaired path for the second named defect class
**and** the envelope is exhausted (2 of 2 declared windows spent) → arm 1 governs the blockedOn
direction (lifted *toward* the frozen-list continuation's own gate — this readout does not draft it)
and arm 2 governs the spend stop (envelope closed; no further spend under GATE-G28k). Records-only, $0 —
a scope disposition, never a compute authorization; any `run`-shape continuation returns through its
own PROV readout under REQ-XH-004. The readout rides the GATE-G30 slot a third time — the chain sits
`blockedOn` upstream of FP-E exactly as it did at the post-Wc9 and post-smoke4 decisions (GATE-G30
`312_GATE-G30__fp-e.md` stays uncreated).

## Evidence (per item: source, `date -u` of the read, layer reached)

- The repair under test — `b07adfdd` (model-port guard: `MODEL_RELAY_PORT_MIN` 49_152 +
  `check_model_relay_port` typed refusal at every projection seam — `cell_env_projection`,
  `admit_allowlist_mediated`'s `model` pair, the hosted-arm `render()` lane; `hh-w1` defaults moved;
  egress stays spec-pinned 3128) + `b3f72ab7` (PHASE LOG) on `codex/harnessharness-model-port-guard`
  (PR #407); the smoke runner built in-window at stamped `b3f72ab7` (digests hh-w1 `sha256:621b9444…`,
  mediator `sha256:af3a0d86…`, helper `sha256:727b7b83…` — zero in-window code patches) — read
  2026-11-16 — layer: committed records.
- The negative leg — in-window `hh-w1 render-policy --freeze /opt/smoke5/freeze.json --phase agent
  --model-port 8080` → `hh-w1: Unsupported: egress_profile.allowlist_mediated (cell env: model relay
  port 8080 is in the registered-service range — the in-cell harness listener must use a
  dynamic/private port (≥49152) so a task-owned service can bind its port)`, rc=1, no policy emitted;
  `--model-port 49152` accepted (epoch-5 render rc=0) — the smoke4 collision value now fails loudly —
  read 2026-11-16 — layer: live window output recorded in `docs/build/runs/R6.45n-smoke5.md`.
- The recorded-signature check — `run-root-smoke5/trials/` + `smoke5-window-record.json`: **4/4 cells
  `exited{0}` `reward_ppm` 1000000** — pypi-server/H-allow ~96 s to completion (76,762 B transcript)
  vs smoke4's deterministic `agent_timeout` ×2 at the 900 s deadline on the identical member+plane;
  pypi-server/H-pub 76,794 B; make-doom-for-mips 65,678/65,706 B byte-identical to smoke4;
  `collect/agent-{stdout,stderr}.txt` on all 4 cell dirs; every leg again streamed past the ~64 KiB
  pipe bound — read 2026-11-16 — layer: harvested run-root
  (`docs/build/reports/compute/harvest/fp-smoke5/` manifest-verified).
- The mediator stores — committed walcheck fold `run-root-smoke5/store-check.json` **pass, 4 cells, 0
  failures**: every `security.egress.requested` paired to its `decided` by `request_ref` (185/185
  make-doom-for-mips, 179/179 pypi-server), all decisions `allow`, zero mediator 5xx, every
  `tunnel.closed` `close_cause: eof` with true integer byte counts, `denied_hosts []` both allow
  cells — read 2026-11-16 — layer: harvested run-root.
- The window lifecycle — receipts `docs/build/reports/compute/2026-11-16-fp-smoke5-{provision,verify,
  prepull,harvest,teardown,cost}.json`; provisioned ~21:49Z, cells 22:02:48Z→22:04:30Z (~102 s),
  harvest ~22:10Z, instance `hh-fp-smoke5` DELETED 22:11:08Z — inside D = 1.5 h and h_r = 6 h; the two
  recorded anomalies recurred verbatim (podman socket `660:root:root` with unpopulated `hh` group →
  `usermod -aG hh` + `podman.socket` restart; the registries `grep`-matches-commented-example check →
  the real `unqualified-search-registries` append) and were handled by their recorded repairs only;
  authorization `match` checked before any create — read 2026-11-16 — layer: committed records.
- The spend — `PHASE_SPEND.jsonl` `sp-87bfc1a85d580057`: smoke5 estimate **224,553 µ$ ≤ 830,480
  ceiling**; model spend $0 (`model_line: none`, `deliver-secret` ABSENT); cumulative $20.431526 —
  read 2026-11-16 — layer: committed records.
- The authorization chain — declaration `fp-smoke5.json` re-pinned `runner.commit` `6801febf` →
  `b3f72ab7` pre-PROV; `receipt_lib.py authcheck` binds the `authorization` block + `quote_id` only
  (the commit is declared data outside the quote hash) → quote recomputation held
  `pq-684bcc89e7f7701d` verbatim; GATE-G28k `envelope_authorized` (verbatim_sha256 `70e362d9…` stamped)
  + GATE-G30 `re-scope{harden-again}` — whose own ledger row designated verbatim "smoke5's minted
  declaration stands authorized-but-unrun under the GATE-G28k envelope for exactly this bounded
  purpose" — + operator verbatim "okay so let's ungate live verification and run the smoke5 stuff
  that's been authorized but not run; i'm ready, continue as you were" (interactive, 2026-11-16).
  The ticket's literal smoke5 clause covers inconclusive-smoke4 retries; smoke4 was conclusive, so
  this window executed under the recorded GATE-G30 designation + GATE-G28k envelope — recorded
  transparently — read 2026-11-16 — layer: committed records.

<!-- agent-drafted:begin sha256=2b5458fedfe0 -->
## Decision content for the operator

**What smoke5 answered.** The post-repair verification question — does the bounded
`machinery{model-port-collision}` repair remove the deterministic H-allow deadlock on the identical
recipe — is answered **yes**: the task's `pypi-server -p 8080` bound its port (the harness listener
moved to 49152), the upload POST returned, and the member+plane that timed out twice at 900 s in
smoke4 completed `exited{0}` reward 1 in ~96 s — the H-pub trajectory it could never reach before.
Byte-stable transcripts across windows confirm the underlying task work is deterministic; the smoke4
hang is fully accounted for by the collision. The negative leg proves the guard refuses the smoke4
value typed rather than colliding silently.

**The discharged machinery line.** Both defect classes named across this thread now carry recorded
repair + live verification on the identical recipe:

| defect class | diagnosed | repaired | live-verified |
|---|---|---|---|
| `machinery{exec-pipe-undrained}` | Wc9 (60.5 KB freeze) | `0c4e0547`/`4679635d` (PR #404) | smoke4 — every leg past the bound, transcripts persisted |
| `machinery{model-port-collision}` | smoke4 (EADDRINUSE read live) | `b07adfdd` (PR #407) | smoke5 — 4/4 `exited{0}` reward 1, typed refusal proven |

No new defect class named in smoke5. Nothing remains on the machinery-repair ladder — no
`harden-again` option exists for a defect that is not standing.

**The blockedOn state.** `task_lists.state` stays **`blocked` verbatim** — every smoke artifact carries
the A1 label (non-evidence; never re-proof evidence); nothing folds into `hh-reproof-packet/1` or
`hh-reproof-register/1`. The Wc9 residuals that are NOT machinery defects stand unchanged: the dev leg
`incomplete{dead_man}` (pytorch-model-recovery's H-allow pair never minted a covering verdict at
74/76), pytorch-model-cli's recorded `allowlist_gap{download.pytorch.org}` (A5/R-7's legit class — a
frozen-policy datum, not a defect), and 3 standing `blocked{no_adjacent_donor}` slots. The
`repair:machinery` leg the stop rule fired on is now repaired and live-verified — what that discharges
is for the frozen-list continuation's own gate to record, not this row.

**Open follow-ups (honest inventory).** DF-R6.45l-1 (transitive verify-host derivation) stays OPEN —
smoke5's evidence pass surfaced zero runtime-transitive denies, epoch-5 held verbatim; the deferral
awaits its derivation follow-up ticket. DF-R6.23-1 (gate-citation predicate) stays OPEN — no
`archive_verified` index exists yet; UPLOAD for smoke4 **and** smoke5 is `pending` (no HF token
provisioned locally — an operator-supplied secret, never recorded). PR #407's merge is operator-owned.

**Spend register.** Completed Σ **$20.431526** (smoke5 estimate $0.224553 added; smoke4 $0.399565;
cumulative ledger verbatim). Envelope **closed**: 2 of 2 declared windows spent; Σ ceilings minted
1,660,960 µ$ ≤ 2,500,000; no third window exists or is minted. Worst-case first-pass ~$26.88 ≤ $30
(smoke5 actualized $0.61 under its ceiling vs the post-smoke4 estimate's carry).

**Dispositions — your verbatim answer (the agent signs nothing).**
`continuation{frozen-list-gate}` · `re-scope{end-first-pass-early}` · `not_run` — per the §6 ladder as
it stands with no defect class in play. `continuation{frozen-list-gate}` means the chain proceeds to
the frozen-list continuation's own gate presentation (G-L1 / GATE-G29 per the manifest chain — its
`full`|`fallback` dispositions, its own evidence chain, its own readout; any `run`-shape continuation
under it returns through its own PROV under REQ-XH-004). `end-first-pass-early` records the honest
outcome — the machinery verified live (drained pipes, typed refusals, persisted transcripts) while the
frozen list stays `blocked` on declared residuals; W4 skipped; the withheld first-pass claims never
fabricate `frozen`. `not_run` leaves `task_lists.state` `blocked` verbatim and dispatches nothing.
There is no `run` of another same-shape window — the envelope is closed; there is no `harden-again` —
no defect stands to repair.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- Appended only when the operator has answered; an agent never writes these lines on its own authority:
Operator decision (verbatim, received <date -u +%FT%TZ> via <channel>): "<exact words>"
Disposition: <continuation{frozen-list-gate} | re-scope{end-first-pass-early} | not_run>; basis: "<words>"
GATE DECISIONS row: <date -u +%FT%TZ> | GATE-G30 (post-smoke5 scope decision)
Operator confirmation of agent-drafted text (verbatim, <date -u +%FT%TZ>): "<words>" — covers sha256:<first 12>
Signed by: the operator. Recorded by <harness/model>, which does not sign.
-->

> **Signature (Reading 1, 2026-11-16)** — verbatim "continue" (operator, interactive) in response to the GATE-G30 post-smoke5 scope-decision presentation (agent-drafted block sha256 `2b5458fedfe0`). Disposition: **`continuation{frozen-list-gate}`** — the verbatim is terse continuation language, and `continuation{frozen-list-gate}` is the only ladder option that continues the chain (the operator's fourth consecutive continuation answer, same mapping as "continue" → `re-scope{harden-again}` at the post-smoke4 decision and "I authorize, please continue" → `envelope_authorized` at G28k); `re-scope{end-first-pass-early}` and `not_run` both stop the line, which "continue" cannot mean. The disposition: the chain proceeds to the frozen-list continuation's own gate presentation — G-L1 / GATE-G29 per the manifest chain, its `full`|`fallback` dispositions over its own evidence chain, created from `_TEMPLATE.md` at presentation; any `run`-shape continuation under it returns through its own PROV readout under REQ-XH-004. No further spend under GATE-G28k (envelope closed); `task_lists.state` stays `blocked` verbatim pending the continuation's own records.
