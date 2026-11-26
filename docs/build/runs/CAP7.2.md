# CAP7.2 — Round-7 closure: O-3 scope-out record, BL-50 dispositions, reconciliation (manifest 336)

Disposition: **done — executed, offline (USD $0.00)**. Records-only capstone: the
conditional O-3 mint verified against CAP7.1's terminal state, BL-50's 40 carried
rows dispositioned in the same record, the register/lead-token reconciliation, and
the GATE-G37 presentation bundle. No window, spend, or gate signature in scope.

- Manifest: `docs/tickets/336_CAP7.2__closure-scope-out.md` (336).
- Branch `codex/harnessharness-r6.53`; depends on CAP7.1 (landed `29a8fac6`).
- Harness: devin-cli / swe-2-high / subagent (LEDGER `harness:` verbatim).
- Ruling: GATE-G34 PASSED / ADR-0454 — operator verbatim "O1 approve, O-2 i'm not
  sure, O-3 approved, yes fold in BL-50. go" (2026-11-20T19:15Z; cited, not
  re-signed).

## What landed

- **`docs/build/reports/round7/O3-scope-out.md`** (+`.sha256`) — the minted per-id
  scope-out record: the 16 non-§8.x routed-set remainder ids verbatim as
  permanently later/not-v1 under the recorded O-3 signature; BL-50's 40-row
  dispositions in the same record; carried-debt section verbatim.
- **`docs/build/readouts/GATE-G37.md`** — the acceptance readout presented
  PENDING with all six pre-registered elements (agent-drafted block sha256
  `8a62849e…`; an agent signs nothing).
- **`docs/tickets/DEFERRALS.md`** — 11 stale lead tokens normalized `(was: OPEN)` /
  `(was: **OPEN** …)` per their dated dispositions.
- **`docs/build/BACKLOG.csv`** — BL-50's dated closure note + `status` open→closed;
  BL-49's malformed landing cell normalized (quoting defect repaired, text
  verbatim).
- **`docs/build/BUILD_INDEX.md`** — row 336; **`docs/build/LEDGER.md`** —
  `lastCompleted: CAP7.2`, `nextTicket: GATE-G37`, dated OPEN FINDINGS bullet,
  PHASE LOG entry.

## AC mapping

- **AC-1 (the O-3 record exists, exactly one branch):** CAP7.1's canonical record
  re-verified — `764ec84f…` sidecar `shasum -a 256 -c` OK on `.json` + `.md`;
  `determination: complete` verbatim (16 satisfied + 2 typed_terminal
  `withheld{inert_at_admissible_window}`/`not_admitted{m_eq_1}`, 0
  unsatisfied/pending/absent) → the mint branch. `O3-scope-out.md` carries every
  named id verbatim: **16 non-§8.x ids** (§7.8 ×10, §5.5 ×2, §6.2/§6.7 ×2, §2.5 ×1,
  §3.8 ×1) + **0 §8.x additions** (the `o3_implication` verbatim). The
  `not_effective{…}` branch was not taken and does not exist.
- **AC-2 (all 40 BL-50 rows dispositioned):** 23 rows closed on cited evidence
  (the 2026-11-06 BL-RECONCILE discharges verbatim — parity tranche-3 30/30, S4
  300/300, efficacy, GATE-G19, study `unmet{s1_failed}`); 17 rows' remaining legs
  take the O-3 disposition verbatim (`R5.47:withheld` ×12; `R5.54/P-layer` ×4;
  `R5.55/P-layer` ×1). Honest attribution: **0 rows satisfied by the §8.x pass's
  own evidence**. The backlog row carries the dated note; `status` flipped per
  ADR-0454 ("status/landing cells updated by dated disposition").
- **AC-3 (deferrals-open reconciled):** re-run shows the true owed set — **8
  rows**: DF-R7.1-2, DF-R7.3-1/-2/-4, DF-R7.5-1/-2, DF-CAP7.1-1/-2 (19 → 8; the
  named set + CAP7.1's fresh stale-token findings DF-R7.1-1/DF-G35-1/DF-R7.3-3 all
  flipped). `--bl` empty.
- **AC-4 (run/PR/BUILD_INDEX/LEDGER agree):** row 336 appended; LEDGER
  `lastCompleted: CAP7.2` / `nextTicket: GATE-G37`; guard post-write route
  `HOLD — gate GATE-G37 pending — RETURN PASS` (gate presentation — the
  expected state). Discrepancies rowed, never
  silently repaired (below).

## Discrepancies rowed (never silently repaired)

- **BL-49 malformed row** — the 2026-11-19 closure note carried an unquoted comma
  (`…admitted, any other ext-tier edge still fails.`), splitting the row to 12
  fields; check-backlog read `status "s"` (baseline FAIL pre-write, verbatim:
  `status: BL-49 has invalid status "s"`). Normalized by quoting the landing cell
  verbatim — text unchanged, the embedded `"` pair doubled per CSV; the row reads
  11 fields / `accepted` post-write. Recorded, not silent.
- **check-backlog round-5 `home-closed` posture** — with BL-50 closed, the
  round-5-matrix run reports the 73 still-PARTIAL-verdict ids homed on a closed
  row. Expected O-3 aftermath: the checker gathers by raw matrix verdict and
  cannot see the O-3 record's dispositions; a `resolved`-class amendment to the
  invariant is guard-owner scope. Recorded verbatim at GATE-G37.
- **`compute-open.sh` `p4 | p4c`** — provision receipt with no
  `hh-compute-receipt/1` teardown receipt: the VM was deleted operator-side under
  H21 ruling B ~07:26Z (zero billable resources verified — readout verbatim); the
  index's "empty for closure" requirement is recorded, the receipt never
  fabricated. Surfaced in the GATE-G37 residual set.
- **`pending{clock}`** — the P4-C ≥24 h provider confirming read is due
  ≥2026-11-27 ~07:26Z — a confirming check (teardown complete); cited verbatim
  from HUMAN-H21, not resolved here.

## Reconciliation record (the flip table)

| row | dated disposition | normalization |
|---|---|---|
| DF-CAP6.1-1…5 | DONE 2026-11-19 status-update rows (CAP6.2, PR #418) | `OPEN` → `DONE (was: OPEN)` ×5 |
| DF-R6.45l-1 | DONE 2026-11-19 (R6.51, PR #413 rides `main` — integration fact; CAP7.1 verified stands) | `(V)`-format last cell → `**DONE** — 2026-11-19 …` + `(was: **OPEN** — minted 2026-11-16 …)` |
| DF-R6.50-1 | `**DONE 2026-11-19**` (HUMAN-H17) | `OPEN` → `DONE (was: OPEN)` |
| DF-R6.53-1 | `**DONE 2026-11-19**` + final note 2026-11-20 (HUMAN-H19; R6.54 `f5bda900`) | `OPEN` → `DONE (was: OPEN)` |
| DF-R7.1-1 | `**DONE 2026-11-20 (coordinator, same-day)**` (resolved-by-repair path) | `OPEN` → `DONE (was: OPEN)` |
| DF-G35-1 | `**DONE 2026-11-20T21:55Z (operator, same-day)**` (deferral withdrawn by ruling) | `OPEN` → `DONE (was: OPEN)` |
| DF-R7.3-3 | `**live-discharged 2026-11-26 (R7.5 closeout)**` (16 fence fires, zero post-repair SIGTERMs) | `OPEN` → `DONE (was: OPEN)` |

DF-R7.3-3's dated marker verified standing — repair `06b5847c` + 16
`fence_overdue` fires verbatim in R7.5's record. DF-R7.3-2/DF-R7.3-4's CAP7.1
dated notings verified standing (residuals — stay OPEN). Revisit triggers
`--check` → in sync, no regen owed.

## Verification (executed)

- `bash docs/build/tools/check-supplement-guard.sh --no-checker` — **PASS**, route
  `DISPATCH — CAP7.2` pre-write; **PASS**, route `HOLD — gate GATE-G37 pending — RETURN PASS` post-write
  verbatim (a gate HOLD is the expected state — the gate is presented to the
  operator, not dispatched; the working-tree check ran clean this pass — the
  half-written-LEDGER FAIL CAP7.1 recorded did not recur; committed-HEAD scratch
  verification recorded below).
- `bash scripts/deferrals-open.sh` — 8 open rows post-write (the true owed set);
  `bash scripts/bm-row.sh` per touched row re-read.
- `bash scripts/deferrals-open.sh --bl` — empty post-write (BL-50 closed).
- `bash scripts/gen-revisit-triggers.sh --check` — **in sync**.
- `bash scripts/compute-open.sh` — `p4 | p4c` verbatim (residual, surfaced).
- `bash ~/agent-skills/skills/reconcile-build/scripts/check-backlog.sh` — default
  matrix 238 expected, 0 issues post-repair (BL-49 `status "s"` pre-write);
  `docs/build/reports/round5` matrix — 73 `home-closed` issues post-write,
  recorded verbatim (expected O-3 aftermath).
- `bash scripts/check-bm-tools.sh` — see below.
- `shasum -a 256` sidecar minted on `O3-scope-out.md`; CAP7.1's sidecars
  re-verified.
- `git diff` reviewed on every shared-register write (BL-41) before commit.
- Not run: cargo suites / check-fp-* emitters (records-only ticket — no code or
  emitted artifact changed); external `check-build-memory.sh` — `AGENT_SKILLS_ROOT`
  checkout present (`~/agent-skills`), run recorded where executed; the
  `resolved`-class baseline amendment for `home-closed` aftermath is guard-owner
  scope, not this ticket's.

## Deferrals

- Opened: none. Closed (lead-token normalized per dated dispositions):
  DF-CAP6.1-1…5, DF-R6.45l-1, DF-R6.50-1, DF-R6.53-1, DF-R7.1-1, DF-G35-1,
  DF-R7.3-3. Carried OPEN (the true owed set): DF-R7.1-2, DF-R7.3-1/-2/-4,
  DF-R7.5-1/-2, DF-CAP7.1-1/-2.

## Deviations / notes

- LEDGER/DEFERRALS/BACKLOG/BUILD_INDEX written by python per BL-41; `git diff`
  verified before commit.
- The verbatim remainder set enumerates **16** non-§8.x ids (the routing
  register's 34 = 18 §8.x + 16); prose counts of "15" in earlier summaries are
  superseded by the verbatim enumeration — recorded in the O-3 record.
- Spend USD $0.00; phase cumulative $68.828421 unchanged; no live leg, no gate
  act, no merge.
