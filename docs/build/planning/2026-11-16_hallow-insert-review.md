# Phase-4 review — H-allow hardening insert (rows 309i–309o)

**Scope reviewed.** The seven inserted chain rows + their contracts, authored per
`docs/build/planning/2026-11-15_hallow-hardening-insert.md` (sha256 `ff39c66f9f41cf6ffc6430e340c098c2fbbd3620d7e0b0059bb47c4bb8c22a6b`)
against `docs/build/planning/2026-11-15_hallow-diagnosis.md` (sha256 `a9ea310f971737c408595fbf592351d2b18bcc1647cdc68e6205234a1ec8d102`),
under orchestrate-build §2.4 Phase-4 (fresh-context review for scope, rigour and
acceptance-criteria soundness). Reviewed on the insert branch
`codex/harnessharness-hallow-insert` (base `origin/main` @`9a415e08`; PRs #390+#391
unmerged at review time — see finding F-1).

**Verdict: PASS with two preview-flagged items (F-1, F-2)** — the insert is sound for
operator preview; F-1 and F-2 are recorded conditions of the base state, not defects
in the inserted content.

## Row-by-row check

| Row | ID | Check | Result |
|---|---|---|---|
| 309i | R6.45i | scope = insert-spec §1 verbatim (residence check, A1–A7 + ADRs, dated notes, R6.45h note); records-only, $0; ACs = spec's acceptance | **sound** |
| 309j | R6.45j | scope = §2 (D15/D17/D18/D19); ACs = spec's AC-1…AC-5 incl. XO-05 pre-fix-fail rule | **sound** |
| 309k | R6.45k | scope = §3 (conformance harness + red baseline first, D8–D14 fixes, D16/epoch-4); split seam pre-declared | **sound** |
| 309l | GATE-G28i | envelope terms match §4 verbatim (≤3, ≤1,250,000 µ$ each, Σ ≤3,500,000 µ$, $0 model, quotes appended as minted); A1 label rule + prediction-track + ladder required; "status from the operator's words — agent signs nothing" | **sound** |
| 309m | R6.45l | members/labels/exit rule match §5 verbatim; in-window patches gated on hermetic regressions + conformance-leg addition | **sound** |
| 309n | GATE-G28j | all eight mandated readout contents present; "presented, not pre-answered" stated; dispositions = §6 ladder | **sound** |
| 309o | R6.45m | skeleton → fleshed after G28j; R6.45h clone + checkpoint; D formula + epoch-4-or-3 recorded; supersedes the 309i stub with a dated note | **sound** |

## Chain integrity

- **Placement:** after `309h` (R6.45h) and before `309` (GATE-G29) — matches §0. Insert spec's proposed ids adopted verbatim (309i…309o free on this base).
- **Dependency edges:** R6.45i→R6.45h; R6.45j→R6.45i; R6.45k→R6.45i (j/k independent per §0 — k does not depend on j); GATE-G28i→R6.45j+R6.45k (both land before smoke 1); R6.45l→GATE-G28i; GATE-G28j→R6.45l; R6.45m→GATE-G28j. Acyclic, matches §0's order.
- **No gate pre-signed, no landed row superseded, no frozen bytes touched** (spec body, executed contracts, registers untouched; all new content is additive rows/files).
- **Honesty:** smoke results walled off as non-evidence in the loop contract, the envelope gate and the manifest notes (never `hh-reproof-packet/1`, register, or parity evidence). `task_lists.state` unaffected.

## Findings

- **F-1 — base-state dependency (preview-blocking flag, not an insert defect):** this branch is cut from `origin/main` @`9a415e08` because PRs #390 (intervention records) and #391 (R6.45h closeout) are unmerged at insert time. On this base the guard cannot fully pass: `lastCompleted`/`Depends on: R6.45h` require #391's landed evidence (309g chain row, BUILD_INDEX row), and the GATE DECISIONS intervention PHASE LOG row is #390's content. The post-merge state was verified on a scratch merge tree (see Verification). Rebase resolution at merge: keep #390's intervention PHASE LOG row + this insert's `inserted` entry (both appended); keep this insert's manifest block and **delete/rename the #391 stub `309i_R6.45i__wc9-run.md`** in favor of `309o_R6.45m__wc9-run.md` (the stub's premise is superseded — recorded in the 309i manifest note + the 309o contract's dated note).
- **F-2 — verbatim ruling wording (preview-blocking flag):** the GATE DECISIONS R-rows record each ruling's substance as committed in the intervention PHASE LOG row + insert spec, because the rulings' literal session wording lives in the session's intervention prompts, not in the repo. **R-10's substance is not recoverable from committed records at all** — its row is an explicit placeholder marked owed-at-preview. The operator should splice the verbatim wording for every R-row (R-10 mandatory) at preview; R6.45i's AC-1 treats verbatim as owed until spliced. This is deliberately *not* asserted as verbatim already.
- **F-3 (note, non-blocking):** R-9 (harvest footprint) and R-11 (first-error-line tally) were exercised early on Wc8 — recorded in their rows' consequence cells citing `docs/build/runs/R6.45h.md` + `verifier-first-errors.txt`.
- **F-4 (note, non-blocking):** the two new gates are sequenced after their authorizing tickets per the 309g pattern (`309l`/`309n` gate column `—`, matching run-time gate inserts, not `STOP` — the STOP column marks the decomposed plan's milestone gates).

## Soundness pass on acceptance criteria

- 309i ACs are checkable (verbatim-once, register resolution, dated notes present, validator+guard clean) — no unverifiable clause.
- 309j/309k ACs name failing-pre-fix regressions with concrete fixtures (1800 s deadline, killed verifier tree, Wc7 query-optimize bytes, concurrent CONNECTs, 250 MiB stream) — meets XO-05.
- 309l requires the envelope numbers + label rule + per-smoke quote appends — matches R-3/A1 and stops on an out-of-envelope quote rather than silently proceeding.
- 309m's exit rule is three-valued (PASS / next smoke / exhausted→blockedOn) with no silent fourth outcome; every in-window patch must land a failing-pre-fix regression before Wc9 — prevents smoke-window drift into Wc9 evidence.
- 309n lists all eight required readout contents (Wc8 outcome, SMOKE-PASS evidence, conformance URL, D sizing, quote, A2 table, §6 ladder, prediction-track) — the two novel A3 elements are explicit.
- 309o is honestly a skeleton (no run line, fleshes after G28j) and records the stub supersession.

## Verification

- `docs/build/tools/check-supplement-guard.sh` — run on the insert branch **and** on a scratch tree merging the post-#390/#391 state; results recorded in the insert PR.
- `check-build-memory.sh` (stock validator) — same.
- `cargo fmt`/tests — n/a (records-only; no code changed).
