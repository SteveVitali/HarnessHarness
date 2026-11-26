<!--
  Readout for docs/tickets/337_GATE-G37__accept.md (manifest 337, Round 7, gate — the
  round's acceptance). APPEND-ONLY: the `Status:` line is the only line that ever changes;
  signing APPENDS the Signature block and never edits or deletes anything above it.
-->
# GATE-G37 readout — Round-7 acceptance: terminal state, the O-3 record, BL-50 and the reconciliation
Status: PASSED   <!-- the ONLY line that may change: → SIGNED | PASSED | SKIPPED-BY-OPERATOR | NOT-PASSABLE -->

> Round-7 acceptance gate. CAP7.1's terminal-state determination is what it recorded; the
> O-3 mint is cited, never re-signed. Agents never sign, tick or infer approval; a pending
> gate is RETURN PASS.

## Criterion (verbatim, from the contract)

"Round-7 acceptance over the minimal tail: the pass's honest terminal state is what CAP7.1
recorded, the O-3 disposition record minted (or `not_effective{…}` recorded + the ruling
re-presents — the operator's verbatim call), BL-50's 40 rows dispositioned as recorded, the
lead-token reconciliation stands against the machine-readable sets, and the Round-7 ledger
and index agree."

## 1 · CAP7.1's terminal-state record (verbatim)

`hh-round7-gap-analysis/1` `docs/build/reports/2026-11-26-round7-gap-analysis.json` sha256
`764ec84ff7bfa6bf0696b4a8af12fec9b75a9d760adbb80ea6ca7d844ea37557` (sidecar re-verified
byte-identical at CAP7.2 — `shasum -a 256 -c` OK on `.json` + `.md` `a80d7732…`) — read
2026-11-26 — `engineered`.

Counts verbatim: **16 satisfied · 2 typed_terminal · 0 unsatisfied · 0 pending · 0 silently
absent** over the 18 routed §8.x ids. `terminal_state.determination`: **`complete`** —
"every one of the 18 routed §8.x ids carries either a satisfied verdict on committed
evidence (16) or a spec-sanctioned typed terminal disposition (2: REQ-XD-205
`withheld{inert_at_admissible_window}`; REQ-XD-214 `not_admitted{m_eq_1}`); zero ids are
unsatisfied, pending, or silently absent." The typed terminals: REQ-XD-205
`withheld{inert_at_admissible_window}` (F(32768)=0.22 < 0.25 — V8/V9 declared, never
sealed, never run) and REQ-XD-214 `not_admitted{m_eq_1}` (C₁₂ pre-declared; `selected{m=1}`
mints no combination member). No routed id's verdict depends on a billed figure.

## 2 · The O-3 record — minted (the pass reached an honest terminal state)

`docs/build/reports/round7/O3-scope-out.md` (+`.sha256`) — read 2026-11-26 — `engineered`.

The conditional (ADR-0454: "conditional on the pass landing") is satisfied by element 1 —
the record minted, **not** the `not_effective{terminal_state_incomplete}` branch. Contents:

- The routed-set remainder — **16 non-§8.x ids** verbatim (§7.8 REQ-XE-002, XE-010…XE-013,
  XE-128…XE-132 ×10; §5.5 REQ-XC-314, XC-316; §6.2/§6.7 REQ-XD-013, XD-052; §2.5
  REQ-XA-126; §3.8 REQ-XH-033), each dispositioned **permanently later/not-v1** under the
  recorded signature — GATE-G34 PASSED / ADR-0454, operator verbatim (interactive, Devin
  CLI session, 2026-11-20T19:15Z): *"O1 approve, O-2 i'm not sure, O-3 approved, yes fold
  in BL-50. go"*. Cited, not re-signed.
- **§8.x additions: 0** — the `o3_implication` verbatim: "no §8.x routed member is left
  unsatisfied — the O-3 remainder set carries no §8.x additions on this record's evidence".
- BL-50's dispositions live in the same record (element 3).

No O-3 re-presentation is owed — the record exists with every named id verbatim; the
`PASSED — re-present{…}` branch stands available only if you read the record differently.

## 3 · BL-50's closure note (40 rows dispositioned; the dated backlog note)

- **40 rows dispositioned — zero silently dropped.** 23 closed on cited evidence (the
  2026-11-06 BL-RECONCILE discharges verbatim: parity tranche-3 30/30 EXACT
  `hh-parity-tranche-report/1`; S4 300/300 `hh-s4-30slot-repairs/1`; efficacy computed
  `hh-s4-efficacy/1`; GATE-G19 PASSED; study `reported` with `unmet{s1_failed}` verbatim);
  **17 rows' remaining legs take the O-3 disposition verbatim** (`R5.47:withheld` ×12;
  `R5.54:withheld`/`P-layer` ×4; `R5.55:withheld`/`P-layer` ×1). Honest attribution
  recorded: **0 rows satisfied by the §8.x pass's own evidence** — the closures cite the
  committed 2026-11-06 discharges, not this pass.
- Dated backlog note: `docs/build/BACKLOG.csv` BL-50's landing cell appended append-only —
  "Closed 2026-11-26 (CAP7.2 …)"; `status` `open → closed` by dated disposition (ADR-0454
  verbatim: "BL-50's `status`/`landing` cells updated by dated disposition"). The 33
  re-homed ids' routed homes are all discharged (DF-H9-1 DONE 2026-11-06 permanently
  unmet-by-decision; DF-CAP5.1-1 DONE; DF-R5.1-1 DONE). No Round-5 record rewritten — the
  matrix's PARTIAL verdicts stand verbatim.

## 4 · The reconciliation record

Stale lead tokens normalized per their dated dispositions — the `(was: …)` convention, 11
flips (each row's dated DONE/discharged marker preserved verbatim in place):

| row | dated disposition (governing) | flip |
|---|---|---|
| DF-CAP6.1-1…5 | DONE 2026-11-19 (CAP6.2, PR #418 — status-update rows) | `OPEN` → `DONE (was: OPEN)` |
| DF-R6.45l-1 | DONE 2026-11-19 (R6.51, PR #413 — rides `main`, not this chain — integration fact, never owed work; CAP7.1 verified) | `**OPEN** — minted …` → `**DONE** — 2026-11-19 …` + `(was: **OPEN** — minted …)` |
| DF-R6.50-1 | DONE 2026-11-19 (HUMAN-H17 verified) | `OPEN` → `DONE (was: OPEN)` |
| DF-R6.53-1 | DONE 2026-11-19/20 (HUMAN-H19; freeze minted R6.54) | `OPEN` → `DONE (was: OPEN)` |
| DF-R7.1-1 | DONE 2026-11-20 (coordinator, `resolved` member path) | `OPEN` → `DONE (was: OPEN)` |
| DF-G35-1 | DONE 2026-11-20T21:55Z (operator same-day withdrawal — "Oh okay. Let's actually just do Phase 4 now then") | `OPEN` → `DONE (was: OPEN)` |
| DF-R7.3-3 | live-discharged 2026-11-26 (R7.5 — repair `06b5847c`, `fence_overdue` 16× live, zero post-repair SIGTERMs) | `OPEN` → `DONE (was: OPEN)` |

Re-run machine-readable owed set — `bash scripts/deferrals-open.sh` → **8 rows, the true
owed set**: DF-R7.1-2, DF-R7.3-1, DF-R7.3-2, DF-R7.3-4, DF-R7.5-1, DF-R7.5-2, DF-CAP7.1-1,
DF-CAP7.1-2 (19 → 8; CAP7.1's two minted rows included). `--bl` → **empty** (BL-50 closed).
`bash scripts/gen-revisit-triggers.sh --check` → **in sync** (no regen owed).
`bash scripts/compute-open.sh` → `p4 | p4c` — one open provision leg (see residuals).
`check-backlog.sh` (agent-skills): default matrix → 238 expected sources, **0 issues** post
repair; `docs/build/reports/round5` matrix → the 73 still-PARTIAL-verdict ids report
`home-closed` on the closed BL-50 — the expected O-3 aftermath, recorded verbatim (the
checker gathers by raw verdict and cannot see the O-3 record; an invariant amendment is
guard-owner scope).

Discrepancy rowed, never silently repaired: **BL-49's row was malformed** — the
2026-11-19 closure note carried an unquoted comma (`…admitted, any other ext-tier edge
still fails.`), splitting the row to 12 fields so check-backlog read `status "s"`.
Normalized this pass by quoting the landing cell verbatim (text unchanged; the embedded
`"` pair doubled per CSV) — check-backlog now reads `accepted`.

## 5 · The accounting

- **Spend this ticket: USD $0.00** (offline, records-only).
- Phase cumulative **$68.828421** — PHASE_SPEND `sp-8b145cb4cd2a2b60` +
  `sp-2d500f70abb282a8` (compute estimates `2026-11-26-p4-p4c-cost.json` 3,861,264 µ$ vs
  12,229,780; `2026-11-26-p4-p4c-r2-cost.json` 6,471,091 µ$ vs 15,999,460).
- **HUMAN-H21's signed entries** (`readouts/HUMAN-H21.md`, PASSED, verbatim "proceed with
  B" sha `a6a80923…`): X0+X1 — model `within` ($21.08 WAL-authoritative recorded vs $24.21
  provider window-span boundary-mixed), compute `unavailable` (recorded estimates stand);
  P4-C — model `within` (**WAL-authoritative $16.90** — 16,896,165 µ$ vs provider ≈$16–18;
  the per-slot $16.64 stands with the known undercount class, DF-CAP7.1-1 rowed), compute
  `unavailable` ($3.86+$6.47 estimates stand), teardown confirmed (`hh-p4-p4c` + 300 GB
  deleted ~07:26Z, zero billable resources verified). Settlement records
  `hh-fp-settlement/1` `2026-11-26-fp-p4x-settlement.json` +
  `2026-11-26-fp-p4c-settlement.json` (+`.sha256`).
- **`pending{clock}` verbatim** — the P4-C ≥24 h post-teardown provider confirming read is
  due ≥2026-11-27 ~07:26Z — demoted to a confirming check (teardown complete, no
  post-teardown usage possible; amends never rewrites). Not a routed-id dependency.
- Guard: `bash docs/build/tools/check-supplement-guard.sh --no-checker` → **PASS**, route
  `HOLD — gate GATE-G37 pending — RETURN PASS` post-write (a gate HOLD is the
  expected state — the gate presents to the operator; pending review is RETURN
  PASS, not a block).
- Ledger/index agreement: LEDGER `lastCompleted: CAP7.2`, `nextTicket: GATE-G37`;
  BUILD_INDEX row 336 appended; the run/PR ledgers agree.
- Named residuals standing for your review (rowed, never silently dropped): the 8-row owed
  set above; `compute-open.sh` `p4 | p4c` (operator-side teardown verified — no
  `hh-compute-receipt/1` minted, index-empty requirement recorded); the site-publication
  pin (the §8.8 claims packet is not in `docs/results/site-data` — a
  `hh-site-publication/1` mint is a publication act outside this chain's authorization,
  R7.6 verbatim); the check-backlog round-5 `home-closed` posture above.

## 6 · Gate decision rows (spend evidence, REQ-XD-221's record)

- **GATE-G35** — PASSED `run` 2026-11-20T21:55Z (operator verbatim "Oh okay. Let's actually
  just do Phase 4 now then"; superseding the same-day deferral — DF-G35-1 discharged by
  ruling): model hard cap `cap_h` Σ 157,760,000 µ$ (declared §10.4 top point); compute
  `pq-59a49747bfa5bdf0` ceiling 13,486,340 µ$; X1 amendment c98cfcdc
  (`pq-4263f129db914f88`). `verbatim_sha256` `ca8b1353…` stamped `declarations/fp-p4x.json`.
- **GATE-G36** — PASSED `run` 2026-11-24T21:10Z (operator verbatim "run"): model `cap_h`
  Σ 46,353,600 µ$ X1-measured; compute `pq-3a4187759b2d8411` 12,229,780 µ$;
  `verbatim_sha256` `acba2551…` stamped `declarations/fp-p4c.json`. **Amendment
  2026-11-25** — resume-with-repair (operator verbatim "ah, ok. Let's resume with
  repair"): repair `06b5847c` + `pq-4e650fe71f863d22` 15,999,460 µ$, `verbatim_sha256`
  `718cad85…` stamped `declarations/fp-p4c2.json`.
- Every spend leg post-gate; hash-before-subjects verified (`1d55d48a…` frozen before any
  confirmation subject; selection `9c3c314c…` predates it). REQ-XD-221's verdict in
  CAP7.1's table: satisfied.

<!-- agent-drafted:begin sha256=8a62849e6c2770921081adccfbe3c292d63f016c6d55d276f00749e8643de402 -->
## Decision content for the operator

**What you are accepting.** CAP7.1's recorded terminal state — 16 satisfied + 2
typed_terminal over the 18 routed §8.x ids — and the records closure CAP7.2 landed on it:
the minted O-3 scope-out record (16 non-§8.x routed ids permanently later/not-v1 under
your recorded G34 signature; 0 §8.x additions), BL-50's 40 carried rows dispositioned
(23 evidence-closed, 17 O-3 verbatim — zero silently dropped; the backlog row closed by
dated disposition), and the 11-row lead-token reconciliation against the re-run
machine-readable set (19 → 8 true owed rows). The round's ledger and index agree.

**What stays open — the named residual set, verbatim.** DF-R7.1-2 (activation evidence
`unmeasured`), DF-R7.3-1 (fold-semantics ADR owed), DF-R7.3-2 (attempt-3 settlement
residuals — compute `unavailable`, recorded estimates), DF-R7.3-4 (harvest `rroot`
derivation leg (a)), DF-R7.5-1 (resume-unsafe artifacts), DF-R7.5-2 (per-container memory
bound), DF-CAP7.1-1 (cost_micros undercount class), DF-CAP7.1-2 (podman name collision);
plus the `pending{clock}` confirming read (due ≥2026-11-27 ~07:26Z — a confirming check,
not a routed-id dependency), the site-publication pin, the `compute-open` `p4 | p4c`
receipt-less-teardown posture, and the round-5 `home-closed` checker aftermath.

**The disposition ladder (pre-registered — your verbatim answer):**
- `PASSED` — the round's record is accepted; the O-3 record stands as minted.
- `PASSED — re-present{…}` — your fresh ruling where a conditional act re-presented
  (incomplete pass; a deferred disposition; a carried set needing words). Nothing in the
  verified record requires this branch.
- `NOT PASSABLE` / `FAILED` — recorded with basis; named residuals carry forward as a
  named set, never silently.
<!-- agent-drafted:end -->

## Signature   <!-- appended at signing; nothing above is edited or deleted -->
<!-- Appended only when the operator has answered; an agent never writes the signature lines
on its own authority. The appended lines' shape is the _TEMPLATE.md set (decision verbatim,
GATE DECISIONS row, agent-draft confirmation covering the block sha256, signed-by) — the
literal placeholder lines stay out of this file so the supplement guard's signature-reuse
leg never reads the template as a verbatim. -->

Operator decision (verbatim, received 2026-11-26 via Devin CLI session): "Accept and
continue" — **disposition `accept`**, accepting Round 7's honest terminal state as
recorded: CAP7.1's `complete` determination (16 satisfied / 2 typed_terminal / 0
unsatisfied over the 18 routed §8.x ids), the minted O-3 scope-out record (16 non-§8.x
routed ids dispositioned permanently later/not-v1 under the GATE-G34 signature), BL-50's
40 rows dispositioned as recorded, and the lead-token reconciliation against the
machine-readable sets. The P4-C scientific outcome stands as landed —
`inconclusive{unresolved_cell}` verbatim (REQ-XD-219): no selected member was confirmed
different from the baseline at Holm α = .05 on 37 held-out tasks; nothing is
re-presented.

verbatim_sha256 `ea9ac3e0bdc3638608618880844edc750dfc51de5fa8b57b742b5ad56d1f391a`.
GATE DECISIONS row: 2026-11-26 | GATE-G37
Signed by: the operator. Recorded by devin-cli/swe-2-high, which does not sign.
