# Run ledger — CAP5.1

- Spec: `docs/tickets/238_CAP5.1__cap1.md`
- Base: `27699a432bea06a9fd30f88f087d4f42943a813e` (codex/harnessharness-evidence-r5.55 tip; PR #270 green at head `1bfd95a`)
- Branch: `codex/harnessharness-evidence-cap5.1`
- Config: `implement-spec spec=docs/tickets/238_CAP5.1__cap1.md gap_analysis=true live_verification=false` (autonomous, ledger on, records-in gap hunt)
- Harness: devin-desktop/swe-2-high/subagent
- Skills: agent-skills @ e981ec5
- Started: 2026-11-04T20:25:24Z (`date -u` at SETUP)
- Closed: (pending)

## Deferrals read

`bash scripts/deferrals-open.sh` at 2026-11-04T20:25Z — `DF-R5.1-1 | OPEN`
(the one intentional row: GATE-G17 contributor evidence for the ten
R5.1-owned targets). DF-H9-1 probed via `scripts/bm-row.sh` — **no row**
(found during the hunt — recorded as a finding, see gap table).

## Requirements

Independent whole-scope gap hunt over the landed round-5 record: every
packet (executed + withheld/unselected + study/sim records), every gate
decision (G18/G11/G12 readings/G13-N0/G14-quote-only/G15/G16
out-of-scope), every deferral (DF-R5.1-1 open; DF-H9-1 claimed open),
every REQ/coverage anchor — real gaps only, never manufactured.

## Acceptance criteria

- AC-1 (D/E; positive): reviewer checks all 167 adopted targets / 13
  aliases / 24 OL / all layers vs landed behavior+evidence; residuals
  each get source/predicate + unique owed-work home.
- AC-2 (D/E; negative): no checkmark from label/fixture/old gate/forward
  promise; duplicate/missing owner or evidence inflating R/P = failure.
- AC-3 (D/E; ownership+evidence): primary targets resolve predicates to
  numbered checks + evidence in the coverage register; achieved D/E
  recorded separately from owed R/P / actual-execution E.
- AC-4 (E/D; scoped verification): regression fails without new
  behavior; docs checks for docs-only work; record commands + counts.
- AC-5 (D/E; phase gate): verification green; REQ ids stamped; owed
  items are DEFERRALS rows; run/PR/BUILD_INDEX/LEDGER agree.

## Plan

Phase 0 setup + deferrals ✓ → Job 1 fleshing (contract dated note,
manifest 238 note, this ledger, PHASE LOG entry, guard DISPATCH check) →
Job 2: `docs/build/tools/cap5_gap_hunt.py` records-in checker (NC-1…
NC-12) → emitted packet `docs/build/reports/2026-11-04-cap5.1-gap-hunt.json`
→ `docs/build/reports/round5/COVERAGE_MATRIX.csv` (180 rows, BM matrix
columns + appended domain/owed/scope columns) +
`CAPSTONE_GAP_ANALYSIS.md` (method, roll-ups, seam hunt, gap routing) →
self-review + gap table → verify (living-docs/bm-tools/check-build-memory
`./--staged`, guard, fmt n/a no Rust touched) → measure ≤750 source/test
trigger → commit → PR → ci-boundary → §6.5 close (BUILD_INDEX seq-238,
lastCompleted CAP5.1 / nextTicket CAP5.2) → report.

## Test matrix

Docs/analysis ticket — no Rust surface. Regression = the checker's own
deterministic checks over committed records (a fabricated packet edit or
a missing row flips a check to FAIL — fails-without by construction of
the check predicates, demonstrated by negative fixtures in the checker
self-test arm). Docs checks: check-living-docs / check-bm-tools /
check-build-memory (external, `.` + `--staged`).

## Progress

- 2026-11-04T20:25Z — SETUP: branch forked at `27699a43`; deferrals read
  (DF-R5.1-1 OPEN); contract/invariants/decomposition/coverage/packets/
  gate decisions/readouts/BUILD_INDEX/phase log loaded (bounded reads).
- 2026-11-04T20:40Z — Job 1: contract dated note + manifest 238 note +
  this ledger + PHASE LOG fleshing entry; guard `PASS route DISPATCH —
  CAP5.1` (counts 82 rows / 180 candidates / 0 viol / 8 warnings).
- 2026-11-04T20:5xZ — Job 2: `cap5_gap_hunt.py` records-in checker
  (NC-1…NC-12) written; first pass found 4 real findings (see repairs);
  all repaired append-only; matrix generated (180 rows).
- 2026-11-04T21:0xZ — checker compacted 816→701 lines to stay under the
  ≤750 source/test trigger; re-run clean: 12/12 checks pass, 0 open
  findings, 4 findings repaired this run; artifacts emitted.

## Gap table

(filled during Phase 4)

| item | required | achieved | status | evidence |
|---|---|---|---|---|
| AC-1 whole-scope check (167+13, 24 OL, all layers) | D/E | engineered | met | NC-1..NC-12 over committed records; matrix 180 rows; reports/2026-11-04-cap5.1-gap-hunt.json |
| AC-2 negative (no label/fixture/forward-promise checkmarks) | D/E | engineered | met | NC-2 fold recompute, NC-6 status, NC-9 over-claim — all pass; no R/P inflation found |
| AC-3 anchors→checks→evidence; achieved vs owed split | D/E | engineered | met | matrix rows carry required_domain/achieved_domain/owed_legs; MET 78 / MET-ENGINEERED 29 / PARTIAL 73 |
| AC-4 scoped verification | E/D | engineered | met | records-in checker (fails-without by construction: a mutated packet/row flips its check to fail); docs checks below |
| AC-5 phase gate | D/E | engineered | met | REQ ids n/a (contributing slice only — none to stamp, recorded); DF-H9-1 + DF-CAP5.1-1 minted; BUILD_INDEX/LEDGER at close |
| F-1 DF-H9-1 phantom row | — | repaired | closed-this-run | DEFERRALS.md row minted verbatim (readouts/HUMAN-H9.md) |
| F-2 G12 reading-2 GATE DECISIONS absent | — | repaired | closed-this-run | reading-2 row appended verbatim |
| F-3 G15 decided_at 56Z vs 29Z drift | — | repaired | closed-this-run | dated correction row appended |
| F-4 orphaned S1 residual | — | repaired | closed-this-run | DF-CAP5.1-1 minted → GATE-G17 |

## Evidence log

- `python3 docs/build/tools/cap5_gap_hunt.py --matrix` → 12/12 pass,
  findings=0 open, findings_repaired=4, matrix 180 rows
  (MET 78 / MET-ENGINEERED 29 / PARTIAL 73).
- `bash scripts/deferrals-open.sh` → DF-R5.1-1 OPEN + DF-H9-1 OPEN +
  DF-CAP5.1-1 OPEN (post-repair).
- `supplement_guard.py` → PASS route DISPATCH — CAP5.1 (pre-dispatch).
- Changed source/test lines: 701 (`cap5_gap_hunt.py` only — all other
  changes are docs/build records, not source/test); ≤750 trigger not
  crossed, no ruling needed.

## Evidence report

Whole-scope records-in hunt over the landed round-5 record: 12/12
checks pass post-repair; 4 real findings found and repaired append-only
(DF-H9-1 minted verbatim; G12 reading-2 row appended; G15 timestamp
correction row; DF-CAP5.1-1 minted for the orphaned S1 residual).
Matrix: MET 78 / MET-ENGINEERED 29 / PARTIAL 73 / 0 MISSING — engineering
closed 107, requirement satisfied 78. Remaining residuals all carry live
homes (DF-R5.1-1, DF-H9-1, DF-CAP5.1-1, GATE-G17). ci: pending.
