# REC5.1 — Backlog reconcile readout (round 5, 2026-11-05)

> The round-5 `reconcile-build mode=backlog` record. Every owed thing lands in
> exactly one backlog row; old rows were never replaced — the register grew by
> append only (45 → 49 rows). Registers updated in place:
> `docs/build/BACKLOG.csv` + `BACKLOG.md` + `OPERATIONAL_READINESS.md` +
> `research/registers/risks.md`. Companion artifact: `READINESS_REPORT.md`.
> Historical same-named reports are untouched.

## What was gathered (BM-VERDICT-01) and where it went

| Source | Owed set | Home |
|---|---|---|
| `docs/tickets/DEFERRALS.md` OPEN/PARTIAL | 3 rows — DF-R5.1-1, DF-H9-1, DF-CAP5.1-1 (all carried forward OPEN by GATE-G17's verbatim 2026-11-05T00:13:07Z disposition — no waiver named) | BL-53 · BL-51 · BL-52 (one row each, all `open`) |
| round-5 matrix `reports/round5/COVERAGE_MATRIX.csv` | 73 PARTAL rows (0 MISSING / 0 AT-RISK / 0 WAIVED / 0 MET-DIFFERENTLY); 29 MET-ENGINEERED rows are covered by their owed-leg D-rows, never gathered | BL-50 (40 scope-disposition ids) · BL-51 (23 participant-predicate ids) · BL-52 (9 S1-residual ids) · BL-53 (REQ-H-006) |
| `docs/adr/` `## Revisit trigger` sections | 204 (ADR-0217…ADR-0420) — all homed pre-existing | BL-01 ×3 env-pending · BL-47 ×152 quiet-conditional index · BL-48 ×49 fired-answered index — no re-home needed |
| LEDGER `## OPEN FINDINGS` | 9 entries — all homed (BL-41/BL-30 mapping stands); CC4 nit adjudicated `no-action-accepted` at CAP5.3; the GATE-ACCEPT re-surface discharged by GATE-G17's fresh signature | (existing rows — no change) |
| spec registers (`conflicts.md`, `open-questions.md`, `spec-debt.md`, `scope.md`) | CF-* packages / OQ packages unchanged — BL-30 / BL-33…BL-39 carry them | (existing rows — no change) |
| `risks.md` | dated Round 5 review appended (11/11 medium; watch RK-02/05/09) | — |

## Residual classification by cause (AC-1)

- **human** — BL-51 (HUMAN-H9 participant supply; DF-H9-1 verbatim trigger).
- **window** — BL-50 (next-round scope dispositions for 40 carried R/P
  predicates), BL-52 (a named bounded S1 re-execution admission or a verbatim
  unmet-by-decision disposition).
- **evidence-record** — BL-53 (contributor cells cite each boundary's
  guard-invocation record; GATE-G17 carried DF-R5.1-1 OPEN with its trigger).
- **resource/environment** — BL-01 (foreign toolchains; E3 + live re-runs),
  standing pre-existing rows BL-02…BL-04 discharged history — carried
  `accepted` posture unchanged.
- **technical** — BL-49 (removability facade edge, `accepted` standing) — the
  only live technical item; the S1 machinery defect itself is discharged
  (R5.44), so no owed technical residual exists.

## Checks

- `check-backlog.sh docs/build/BACKLOG.csv docs/build docs/tickets` →
  **exit 0** (expected sources 204; sums 66/66 on the round-4 register).
- `check-backlog.sh docs/build/BACKLOG.csv docs/build/reports/round5 docs/tickets`
  → **exit 0** (expected sources 73 — every PARTIAL id homed exactly once;
  sums **107/180 · 78/180** matching CAP5.3's headline; JSON `backlog-check/2`).
- `deferrals-open.sh --bl` → BL-50/51/52/53 listed `open` (the only open rows).
- Checker blind spots compensated by hand (recorded in `runs/REC5.1.md`): the
  stock gather's DEFERRALS grep matches `D-` ids (this register uses `DF-`),
  and its leg-token match misses `src:state->DF-id` legs — the 3 DF rows and
  the 29 MET-ENGINEERED leg-routings were verified by eye/script instead.
- `docs/build/reports/2026-11-04-cap5.3-closure.json` regen seen uncommitted
  at SETUP (base `27decf54`, `packet_id b82a02e8…`, folded record identical) —
  reverted; the committed packet `9d756c68…` remains the BUILD_INDEX-cited
  artifact.

## Duplicates / drops

None — every gathered source id appears in exactly one `sources` cell and
every still-owed source sits on a non-closed row. No finding could not be
placed.
