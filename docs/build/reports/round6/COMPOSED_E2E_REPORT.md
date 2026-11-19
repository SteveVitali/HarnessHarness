# CAP6.1 — Composed Offline Verification Report (Stage 2)

Ticket: 321_CAP6.1 · Branch: `codex/harnessharness-cap6.1` · Base: `31fc815b` · Run date: 2026-11-19 · Spend: $0 (offline-only; no credentials, no paid jobs).

Stage-1 verdicts and artifacts (COVERAGE_MATRIX.csv, CAPSTONE_GAP_ANALYSIS.md, gap-hunt packet) were committed at `084a41eb` **before** run-ledger inspection, per the contract's two-stage ordering.

## Command ledger

| # | command | result | notes |
|---|---|---|---|
| 1 | `python3 docs/build/tools/cap6_gap_hunt.py` | FAIL→checks detailed | NC-1..NC-11, AC-1/AC-2: all pass **except NC-5b** (misplaced gate-decision rows — GAP-01). Matrix 456 rows emitted. |
| 2 | `bash docs/build/tools/check-supplement-guard.sh` (AGENT_SKILLS_ROOT set) | **FAIL**, `route: HOLD — gate GATE-G28i pending — RETURN PASS` | 8 violations: misplaced-region consequences (G28i pending, nextTicket, paid-body R6.47), annotated Depends cells (G28l, R6.51), forward-dep R6.45o, GATE-G28k criterion heading, stock `GATE-G30.md` false positive, skeleton-313 Run line. All → findings register. A HOLD route is RETURN PASS, not a missing implementation; the HOLD itself is GAP-01's symptom. |
| 3 | `python3 docs/build/tools/check_round6_coverage.py .` | **FAIL 34/35** | `primary_kind: ticket` (register) vs `skeleton` (plan table) on 10 ids → GAP-04. |
| 4 | `bash scripts/check-drift.sh` | **PASS** | generated artifacts in sync |
| 5 | `bash scripts/check-site.sh` | **FAIL** | build REFUSED — publication filename `phase-3-2026-11-19-flip.json` ≠ `<phase>-<YYYY-MM-DD>.json` contract → determinism/availability/a11y legs cascade; `phase-{2,3,4}/index.html` not rendered → GAP-13 |
| 6 | `bash scripts/check-position.sh` | **PASS** | 14 legs green |
| 7 | `bash scripts/check-live-evidence.sh` | **PASS** | 51 records, cumulative spend $25.2598, cap enforced |
| 8 | `bash scripts/check-bm-tools.sh` | **PASS** | |
| 9 | `bash scripts/check-living-docs.sh` | **PASS** | 554 files |
| 10 | `bash scripts/check-fp-select.sh` | **PASS** | comparative-quote recomputes byte-identically |
| 11 | `bash scripts/check-fp-partition.sh` | **PASS** | tests OK |
| 12 | `bash scripts/check-fp-prereg.sh` | **FAIL** | `emit-identical` FAIL on `2026-11-10-fp-prereg.{json,sha256}` — arm admission_sha256 drift (fixtures amended 11-16/17) → GAP-02 |
| 13 | `bash scripts/check-fp-results.sh` | **FAIL** | `reemit-full` + `byte-identical-retained:2026-11-19-fp-seal-open` FAIL; all other legs OK → GAP-14 |
| 14 | `bash scripts/check-compute-receipts.sh` | **FAIL (crash)** | `AttributeError: 'list' object` on `compute/2026-11-18-fp-ws1r-refold-diffs.json` (top-level `[]`) → GAP-09 |
| 15 | `bash scripts/gen-revisit-triggers.sh --check` | **FAIL** | ADR-0450, ADR-0451 unplaced in BACKLOG.csv sources → GAP-08 |
| 16 | `bash scripts/deferrals-open.sh` | output | `DF-R6.45l-1` (genuinely open), `DF-R6.50-1` (lead-OPEN, dated-DONE — GAP-06) |
| 17 | `bash $AGENT_SKILLS_ROOT/skills/build-memory/scripts/check-build-memory.sh .` | **1 violation / 3 warnings** | `docs/adr/README.md` stale vs fresh adr-index regeneration (ADR-0450/0451 admitted without regen — same family as GAP-08) → GAP-15 |
| 18 | `bash ~/agent-skills/skills/orchestrate-build/scripts/ci-boundary.sh --stack --no-wait --ledger docs/build/LEDGER.md` | **blockedOn** | record `ci-boundary/1`, `read_at: 2026-11-19T04:56:14Z` — see §2 |
| 19 | `cargo test -p hharness-runner --test r6_33 --test r6_48 --test r6_49 --test r6_16` | **PASS** | 41 tests, 0 failed (r6_33:11, r6_48:10, r6_49:15, r6_16:5) |
| 20 | `cargo test -p hharness-runner --test r6_8 --test r6_22 --test r6_23 --test r6_26 --test r6_27 --test r6_29 --test r6_30 --test r6_45k` | **PASS** | 73 tests, 0 failed, 6 ignored |
| 21 | `bash scripts/check-removability.sh` | **skipped(heavy-rebuild)** | tier-combination rebuild exceeds this session's bound; owed leg → DF-CAP6.1-5 |

## 1. Totals

- checks run: 20 + 1 skipped(heavy-rebuild) (script/cargo legs listed; the guard's internal legs counted once)
- passed: 10 · failed: 9 · informational: 2 · skipped: 1
- Every non-green result is a **typed finding** in `CAPSTONE_GAP_ANALYSIS.md` §2 and carries a proposed DF row (DF-CAP6.1-1…5); none is silently waived and none blocks the analysis artifacts themselves. They are pre-existing base-state seams, not CAP6.1 regressions (`git status` clean over base + artifacts only).

## 2. CI read of the open stack (BM-CI-01)

`ci-boundary.sh --stack --no-wait` — `read_at: 2026-11-19T04:56:14Z`, record schema `ci-boundary/1` (verbatim record committed in the run record):

| PR | head | state |
|---|---|---|
| #416 `codex/harnessharness-r6.50` (base) | `31fc815b` | **fail** — build·test·fmt·drift ubuntu+macos (run 44458421386) |
| #415 `codex/harnessharness-r6.49` | `be6315b2` | **fail** |
| #414 `codex/harnessharness-r6.48` | `7363013a` | **fail** |
| #412 `codex/harnessharness-r6.47` | `7dab4e25` | pass |
| #411 `gate-g30` | `9f27c49b` | pass |
| #409 `human-h15` | `9ece22d4` | pass |
| #408 `r6.46` | `662dab5e` | pass |
| #407 `model-port-guard` | `0d4e85b6` | pass |

Verdict verbatim: `blockedOn: CI fail on #416 (build · test · fmt · drift (ubuntu-latest)): fail (run 44458421386)`. Open PRs not in the stack read: #410, #413. **No production-state statement is made beyond this cited read** — the stack tip is CI-red at base; the drift findings above (site build refusal, fp-prereg/fp-results emit-identical, stale adr-index) are the likely local correlates, stated as correlation, not cause.

## 3. Bounded Round-6 battery scope statement

`cargo test -p hharness-runner` r6-targets run: r6_33, r6_48, r6_49, r6_16 (41 tests green) plus the r6_8/22/23/26/27/29/30/45k batch (73 tests green, 0 failed, 6 ignored). Full workspace `cargo test` was **skipped(budget)** — honest bound: CAP6.1's verdicts rest on committed records + the checkers above, not on a full-workspace re-run; owed leg if required → CAP6.2.

## 4. Environment blocks routed to the operator

- GCP compute: unavailable (recorded HUMAN-H18) — no new block.
- CI tip red on #414–#416: operator-visible; CAP6.1 does not assert stack health.

— end —
