# GATE-G30 — post-smoke4 scope decision: the diagnostic window verified the exec-drain repair AND named a new machinery defect class (`machinery{model-port-collision}` — the in-cell model relay owns 127.0.0.1:8080, the task's own `pypi-server -p 8080` gets EADDRINUSE, the upload POST lands on the never-answering model stub → deterministic H-allow `agent_timeout` ×2) → `blockedOn` stands; the §6 ladder; no smoke5, no Wc10 · readout
Status: PENDING   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion

Ticket `docs/tickets/309q_R6.45n__diag-smoke.md` exit conditions: "Verification recorded … → `blockedOn`
lifted toward the frozen-list continuation's own gate" OR "envelope exhausted **or a newly named defect
class** → `blockedOn` + a §6-shaped readout; no further spend." smoke4 produced both clauses at once —
the recorded signature is absent on the repaired path **and** a newly named machinery defect class
stands → the second clause governs: `blockedOn` stands, this readout is the §6-shaped presentation, and
**smoke5 does not dispatch** (the contingency clause covers inconclusive/infra-noise outcomes only —
this one is conclusive). Records-only, $0 — a scope disposition, never a compute authorization; any
`run`-shape continuation returns through its own PROV readout under REQ-XH-004. The readout rides the
GATE-G30 slot again — the chain sits `blockedOn` upstream of FP-E exactly as it did at the post-Wc9
decision (GATE-G30 `312_GATE-G30__fp-e.md` stays uncreated).

## Evidence (per item: source, `date -u` of the read, layer reached)

- The repair under test — `0c4e0547` (`exec_guarded` drains both piped streams on reader threads for
  the child's life) + `4679635d` (`run_cell` persists non-empty agent streams as
  `collect/agent-{stdout,stderr}.txt`) on `codex/harnessharness-exec-drain` (PR #404); the smoke runner
  built in-window at stamped `6801febf` (digests hh-w1 `sha256:96663bcf…`, mediator `sha256:8f830a7f…`,
  helper `sha256:727b7b83…` — zero in-window code rebuilds) — read 2026-11-16 — layer: committed
  records.
- The recorded signature check — `run-root-smoke4/trials/` + `smoke4-window-record.json`: every leg's
  `collect/agent-stdout.txt` streamed past the ~64 KiB pipe bound the Wc9 freeze died at
  (pypi-server/H-allow 1,678,026 B r1 + **1,720,851 B r2**, pypi-server/H-pub 76,792 B,
  make-doom-for-mips 65,678/65,706 B) — the pipe never stalled on any leg; `agent-{stdout,stderr}.txt`
  landed on all 5 cell dirs; mediator stores zero 5xx, every `requested` paired to its `decided`
  (161/161 r1, 178/178 r2 on the pypi-server allow runs), eof-only `tunnel.closed`, `denied_hosts []`
  both allow runs; 3/4 declared cells `exited{0}` **reward_ppm 1000000** — read 2026-11-16 — layer:
  harvested run-root (`docs/build/reports/compute/harvest/fp-smoke4/` manifest-verified).
- The newly named defect — `docs/build/reports/2026-11-16-smoke4-model-port-collision-diagnosis.json`:
  `h_run_once`'s `if mediated` branch binds `RelayPair{model, cfg.model_port=8080}` in-cell before the
  agent starts (`crates/hh-runner/src/bin/hh_w1.rs` ll. 1110–1230); pypi-server's `pypi-server -p 8080`
  gets **`OSError: [Errno 98] Address already in use`** (read live from the rerun's in-cell
  `/var/log/pypiserver.log` at 19:17Z — the observability leg's byte-exact diagnosis working as
  designed); `twine upload` POSTs `localhost:8080` — `no_proxy` bypasses the mediator by design — onto
  the model relay → `model.sock`'s never-answering stub → the POST hangs to the `task_toml` 900 s
  deadline; H-pub installs no relays/env → identical task `exited{0}` reward 1 in ~82 s. Deterministic:
  reproduced verbatim in the window-local r2 diagnostic re-run — read 2026-11-16 — layer: harvested
  run-root + committed diagnosis record.
- The discriminator discharge — the same record: NOT the pipe defect (26× the bound streamed
  continuously to deadline); NOT egress mediation (0 requests for `localhost:8080` anywhere — the
  loopback bypass held; zero denies); NOT task-provability (identical `solve.sh` passes H-pub reward
  1); NOT infra noise (deterministic ×2 + a named mechanism). The collision is latent under the
  canonical drive config (`model_port` default 8080) — pypi-server/H-allow is unprovable under
  mediation until the defect is repaired or the port moves — layer: committed records.
- The window lifecycle — receipts `docs/build/reports/compute/2026-11-16-fp-smoke4-{provision,verify,
  prepull,harvest,teardown,cost}.json`, all schema-valid under `check-compute-receipts.sh` (72 + 15 +
  19 OK); provisioned ~18:44Z, cells 18:51:28Z→19:08:24Z, harvest+teardown ~19:39Z — inside D = 1.5 h
  and h_r = 6 h; the recorded podman-socket + registries anomalies recurred and were handled by their
  recorded repairs only — read 2026-11-16 — layer: committed records.
- The spend — `PHASE_SPEND.jsonl` `sp-eaed45f9d76e4fd8`: smoke4 estimate **399,565 µ$ ≤ 830,480
  ceiling**; model spend $0 (`model_line: none`, `deliver-secret` ABSENT) — read 2026-11-16 — layer:
  committed records.

<!-- agent-drafted:begin sha256=4909a2d7ab18 -->
## Decision content for the operator

**What smoke4 answered.** The diagnostic's one machinery question — does the repaired exec-capture path
carry the members' large-transcript agent execs to completion — is answered **yes**: every leg streamed
past the recorded pipe bound end-to-end, transcripts persisted, and the cells that could complete did
(3/4 decided reward 1, incl. make-doom-for-mips on both planes — the member whose Wc9 hangs the
diagnosis first pinned on the pipe defect).

**What smoke4 surfaced.** A second, distinct machinery defect — latent in every pypi-server/H-allow run
under the mediated drive (the canonical `model_port` default 8080 included), unmasked by the repair:
Wc9's allow cells died at the pipe bound long before reaching the task's server-bind step, so the
collision never had a chance to fire. The honest classification is `machinery{model-port-collision}`,
not `task_provability`: the harness's own listener occupies the port the task's declared solution
needs, and the failure mode is a silent 900 s deadlock rather than a typed refusal. It is also honestly
**smaller** than the pipe defect — a `model_port` change is declared data, and a collision-check +
typed refusal is a bounded lower-layer repair, same class as `0c4e0547`.

**The blockedOn state.** `task_lists.state` stays **`blocked` verbatim** — this window is non-evidence
(A1) and nothing folds into `hh-reproof-packet/1` or `hh-reproof-register/1`. The standing residuals
from Wc9 are unchanged: pypi-server's `repair:machinery` discordance now carries a second named
mechanism; the dev leg `incomplete{dead_man}`; pytorch-model-cli's recorded-exhausted donor walk; 3
standing `blocked{no_adjacent_donor}`.

**Spend register.** Completed Σ now ~$18.67 (smoke4 estimate $0.40 added); worst-case first-pass
~$27.49 ≤ $30 with smoke5's unrun $0.83 ceiling still authorized-but-unspent. Envelope: 1 of 2
windows used; Σ ceilings minted 1,660,960 µ$ ≤ 2,500,000.

**Dispositions — your verbatim answer (the agent signs nothing).**
`re-scope{harden-again}` · `re-scope{exclude-and-replace}` · `re-scope{end-first-pass-early}` ·
`not_run` — per the ladder. `harden-again` here means a bounded repair of the port collision (typed
refusal on bind conflict and/or a task-aware relay-port assignment, with a regression battery — the
same $0-repair → gated-diagnostic pattern the exec-drain followed), NOT another same-shape window.
There is no `run` of another same-shape proof window; smoke5 is spent only if your disposition calls
for a bounded verification of the next repair — and any such continuation returns through its own gate
and PROV readout under REQ-XH-004.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- Appended only when the operator has answered; an agent never writes these lines on its own authority:
Operator decision (verbatim, received <date -u +%FT%TZ> via <channel>): "<exact words>"
Disposition: <not_run | re-scope{harden-again} | re-scope{exclude-and-replace} | re-scope{end-first-pass-early}>; basis: "<words>"
GATE DECISIONS row: <date -u +%FT%TZ> | GATE-G30 (post-smoke4 scope decision)
Operator confirmation of agent-drafted text (verbatim, <date -u +%FT%TZ>): "<words>" — covers sha256:<first 12>
Signed by: the operator. Recorded by <harness/model>, which does not sign.
-->
