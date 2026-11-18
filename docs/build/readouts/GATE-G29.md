# GATE-G29 — G-L1: record the L1 decision as `full` or `fallback` · readout
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> An operator or authorized human record supplies the decision; an agent must not sign or assume silence is
> approval.

## Criterion (verbatim)

Spec §10.3 gate table — **this gate carries the G-L1 part** (`hh-l1-conformance/1`: first divergence
fails, BM1–BM19 coverage complete, census complete, rule table frozen; pass or fail only); the PROV-W3
part was GATE-G27's. Pre-registered dispositions (ticket `docs/tickets/309_GATE-G29__g-l1.md`,
§10.1.4 verbatim): `full` — L1 passed for the exact H, frozen by digest here; the HH-native arms may
run in W4 (REQ-XD-102); R6.46 quotes five arms. `fallback` — the Q-X62 trigger fired, or L1 did not
pass on either gate partition, or L1 did not pass on gate-A and no gate-B rerun is runnable in the
first pass (§10.1.4 Wc row: at most one contingency window; §7.4: a fail is never relabelled — the
basis cites GATE-G28's readout and the Q-X62 trigger instant); FP-E is held in its hosted-only form;
P3-L2 and P4-FP are `withheld{l1_not_passed}` (REQ-XH-123); R6.46 quotes the three hosted arms.

## Evidence (per item: source, `date -u` of the read, layer reached)

- **`hh-l1-conformance/1` — ABSENT on both gate partitions.** Gate-A: W3's corpus + L1.0/L1.1/L1.2
  legs returned `withheld{un_reproofed_successor:task_lists_state=blocked}` — subject slots admit
  only against a frozen development list; `docs/build/reports/live/2026-11-11-corpus-w3.json`
  (`predicate.task_lists_state: blocked`, `result: withheld{…}`), `docs/build/runs/R6.44.md` — read
  2026-11-16 — layer: committed records. Gate-B: Wc's W3′ L1 rerun on gate-B never ran — GATE-G28's
  readout dispositioned `triggered{W1′}` (the O-surface repair re-proof, served through Wc9); the
  W3′-L1 trigger was never dispositioned — `docs/build/readouts/GATE-G28.md` — read 2026-11-16 —
  layer: committed records. L1 has not passed on either partition.
- **H freeze** — set_digest `sha256:c451ed97d0206821bedd6e2255e5f6bba4736646d7170601b4085492e711a8b9`,
  built once inside the pinned `rust@sha256:6ae102bd…` container at commit `3228ecf4` in W3
  (REQ-XC-330); members `hh_kernel acc91fd0…`, `hir 98934508…`, `compiled_artifacts 44e7a309…`;
  partition digests committed at R6.42 before it (`sha256(seed|fp_dev7|cell|slot)` ascending →
  dev 3 / gate-A 2 / gate-B 2, REQ-XD-106, REQ-XH-114) — `docs/build/runs/R6.44.md`,
  `docs/build/runs/R6.42.md` — read 2026-11-16 — layer: committed records.
- **Trigger state** — R6.33's FP-S readiness record cited at GATE-G26: readiness instant
  **2026-11-10T21:42:46Z**; Q-X62's 14 days → trigger instant **2026-11-24T21:42:46Z** — not yet
  passed at presentation (read 2026-11-16 ~8 days early); "a G-L1 failure on both gate partitions
  triggers Q-X52's fallback at once" — no failure is recorded (the legs were withheld/never ran,
  not failed) — `docs/build/readouts/GATE-G26.md` — layer: committed records.
- **`task_lists.state`** — `blocked` verbatim (`hh-task-lists-outcome/1`,
  `docs/build/reports/2026-11-16-fp-wc9-tasklists.json`): 9/14 covering `reproofed`; pypi-server
  `repair:machinery` (append-only discordance — **repaired and live-verified** at smoke5 under
  GATE-G30's `re-scope{harden-again}` + `continuation{frozen-list-gate}` dispositions —
  `docs/build/runs/R6.45n-smoke5.md`); dev leg `incomplete{dead_man}` (74/76 cells at the in-guest
  STOP — pytorch-model-recovery's H-allow pair never minted a covering verdict); pytorch-model-cli
  `allowlist_gap{download.pytorch.org}` (A5/R-7 legit exclusion, REQ-XH-127(c) walk recorded); 3
  standing `blocked{no_adjacent_donor}` — read 2026-11-16 — layer: committed records.
- **Masking** — no first-pass outcome field is cited anywhere in this readout; the L1 result is
  machinery evidence only (REQ-XH-121) — read 2026-11-16 — layer: presentation discipline.
- **DEFERRALS rule (rule 4: this gate does not pass while any OPEN row scoped to L1, W3 or Wc
  remains)** — `bash scripts/deferrals-open.sh` returns two OPEN rows, carried verbatim, never
  silently passed: **DF-R6.23-1** (REQ-XH-102 gate-citation predicate — owners are the paid windows'
  readouts incl. R6.44/W3 + R6.45/Wc; W3's own readout carries no `archive_verified` citation because
  no archive index exists — UPLOAD pending for every window; arguably W3/Wc-scoped) and
  **DF-R6.45l-1** (transitive verify-host derivation — egress-freeze derivation machinery adjacent
  to the Wc line; smoke5's evidence pass surfaced zero runtime-transitive denies). Whether either is
  in-scope for rule 4 is named here, not decided silently — read 2026-11-16 — layer:
  `docs/tickets/DEFERRALS.md`.

<!-- agent-drafted:begin sha256=b2212ee4974c -->
## Decision content for the operator

**The L1 evidence state.** No `hh-l1-conformance/1` exists and none can be fabricated: the corpus
plus L1.0/L1.1/L1.2 legs were withheld on gate-A under the frozen-list predicate, and the gate-B
rerun never ran — GATE-G28 dispositioned Wc's one contingency to the W1′ repair re-proof. The
blocked-freeze predicate was discharged as written — nothing admitted, nothing fabricated, no
substitute subject, no scored refusal. `full` is therefore **not recordable on evidence today**.

**The machinery discharge.** The residual that held the frozen list — pypi-server's
`repair:machinery` discordance — is now repaired and live-verified (the exec-drain and model-port
defects both carry smoke-window verification on the identical recipe; PR #404 + PR #407). A re-proof
window minting `task_lists.state: frozen` would re-open the withheld L1 corpus legs and make `full`
reachable before the Q-X62 trigger instant (2026-11-24T21:42:46Z, ~8 days). Any such window returns
through its own PROV readout under REQ-XH-004 — it is not authorized by this gate.

**The honest options.** `fallback` is dispositionable on the record now — its second arm holds
verbatim ("L1 did not pass on either gate partition": withheld on gate-A, never run on gate-B, and
no gate-B rerun is runnable — Wc's contingency served W1′) — recording FP-E held in hosted-only
form, P3-L2/P4-FP `withheld{l1_not_passed}` to a later pass, W4 the three hosted arms. `full` is
unearned: it requires a `hh-l1-conformance/1` pass for the exact H — reachable only through a
re-proof → frozen list → withheld-legs window before the trigger, gated by its own PROV. Answering
neither disposition and routing to a re-proof PROV gate keeps `full` reachable under the
frozen-list-continuation path the post-smoke5 decision named.

**Dispositions — your verbatim answer (the agent signs nothing).**
`full` · `fallback` — per the pre-registered pair. The basis is recorded verbatim: under `full`, the
frozen H digest + the conformance pass id (which does not exist — `full` is not presentable); under
`fallback`, GATE-G28's `triggered{W1′}` record + the Q-X62 trigger instant + "L1 did not pass on
either gate partition" verbatim. A third answer — route to a re-proof PROV gate — is not a
pre-registered disposition here and would be recorded as the operator's own ruling if given.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- Appended only when the operator has answered; an agent never writes these lines on its own authority:
Operator decision (verbatim, received <date -u +%FT%TZ> via <channel>): "<exact words>"
Disposition: <full | fallback | <operator's own ruling>>; basis: "<words>"
GATE DECISIONS row: <date -u +%FT%TZ> | GATE-G29 (G-L1)
Operator confirmation of agent-drafted text (verbatim, <date -u +%FT%TZ>): "<words>" — covers sha256:<first 12>
Signed by: the operator. Recorded by <harness/model>, which does not sign.
-->

> **Signature (Reading 1, 2026-11-16)** — verbatim "continue" (operator, interactive) in response to the GATE-G29 (G-L1) presentation (agent-drafted block sha256 `b2212ee4974c`). Disposition: **operator's own ruling — route to a re-proof PROV gate**. `full` is not presentable (no `hh-l1-conformance/1` exists — the legs were withheld, never failed) and `fallback` closes the frozen-list line for this pass; "continue" is terse continuation language mapped to the only reading that keeps the frozen-list line — and `full` — reachable (the operator's fifth consecutive continuation answer, same mapping as "continue" → `continuation{frozen-list-gate}` at the post-smoke5 decision; the readout named this third answer expressly: "not a pre-registered disposition here and would be recorded as the operator's own ruling if given"). The ruling dispatches a re-proof PROV gate insert — the Wc10-shaped re-proof window on the verified machinery (runner tip `b3f72ab7`, `model_port` band `49152`), returning through its own PROV readout under REQ-XH-004; a re-proof → `task_lists.state: frozen` → the withheld L1 corpus legs makes `full` reachable before the Q-X62 trigger instant (2026-11-24T21:42:46Z). GATE-G29 records neither `full` nor `fallback`; `task_lists.state` stays `blocked` verbatim pending the re-proof's own records.

---

## Return presentation — 2026-11-18 (Ws1 landed; Reading 2, agent-drafted)

<!-- agent-drafted:begin -->

The Reading-1 ruling ("route to a re-proof PROV gate") dispatched the chain
Wc10 → Wcs1 → Ws1. It is now discharged on evidence:

- **Re-proof:** Wcs1 minted `task_lists.state: frozen` honestly — 17/17 slots
  covered, 42 decided cells, `archive_verified` (run `docs/build/runs/R6.45p.md`).
- **Withheld legs:** Ws1 ran W2's PP0 (9 cells: 7 pass, 2 `agent_timeout`),
  the L1 corpus (28 recordings, 22 `exited{0}` + 6 `agent_timeout`,
  0 denied hosts, 0 withheld rows), and L1.0/L1.1/L1.2 on gate-A over the
  frozen H-set `sha256:c451ed97…` (run `docs/build/runs/R6.45q.md`).

**The L1 evidence state has changed.** `hh-l1-conformance/1` now **exists**,
bound to the exact frozen H and the gate-A partition — `result: fail`,
`parity: none`. The strict-replay endpoint measured a real structural
divergence: H's assembled first-turn request differs from O's recorded
mini-swe-agent prompt in `messages[0].content` (`hh_submit` tool framing,
1,944 B vs O's 6,152 B bash-tool prompt); 40 durable `hh-l1-diff/1` records
were minted; `s:1` uniform; `prefix:1: 8`. L1.0's 16/16 `exited{0}` proves
the replay plumbing serves the recorded sequence verbatim — the fail is a
genuine measurement, not a machinery void. The record self-labels "L1
precondition gate — not parity evidence."

**Dispositions on the record now.** `full` remains **not presentable** — its
arm requires a *pass* for the exact H, and the record is a fail. `fallback`
**is presentable on evidence**: its recorded arm holds verbatim — "L1 did
not pass on either gate partition" (gate-A executed and recorded `fail`;
gate-B never ran and needs no fabrication). Fallback records FP-E held in
hosted-only form, P3-L2/P4-FP `withheld{l1_not_passed}` to a later pass,
W4 the three hosted arms — per the pre-registered text.

**Cost honesty.** Ws1's compute estimate exceeds its PROV ceiling
(5,250,954 µ$ vs 4,136,280 µ$, ~127%) — the mid-corpus VM shutdown and
surgical wal-fold recovery stretched wall clock to ~24 h. Recorded, not
hidden; the FP-S envelope's aggregate is a reading for the operator.

**Dispositions — your verbatim answer (the agent signs nothing).**
`full` · `fallback` — per the pre-registered pair. Under `full`: the frozen
H digest + the conformance pass id (still does not exist — `full` is not
presentable). Under `fallback`: GATE-G28's `triggered{W1′}` record + the
Q-X62 trigger instant + "L1 did not pass on either gate partition" verbatim
— now backed by an executed gate-A `hh-l1-conformance/1` `result: fail`.
<!-- agent-drafted:end -->

> **Signature (Reading 2, 2026-11-18)** — verbatim "my disposition is to diagnose and fix any failures and to iterate until success" (operator, interactive) in response to the GATE-G29 return presentation. Disposition: **operator's own ruling — diagnose-and-iterate**: the `hh-l1-conformance/1` `result: fail` at gate-A is to be diagnosed and repaired through iterated legs until the strict-replay fold records `pass`, at which point `full` becomes presentable on evidence. Neither `full` nor `fallback` is recorded today; `task_lists.state` stays `frozen` verbatim (Ws1 discharged the withholding; the L1 question is now a divergence-repair problem, not an evidence-absence). Any live re-drive of the legs remains behind its own authorization; the diagnosis and replay-mode iteration is $0-model work. GATE DECISIONS row appended.
